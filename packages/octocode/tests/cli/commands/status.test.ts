import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  formatAuthStatusAsJson,
  printAuthStatus,
} from '../../../src/cli/commands/shared.js';
import {
  getMCPConfigPath,
  configFileExists,
  MCP_CLIENTS,
} from '../../../src/utils/mcp-paths.js';
import { readMCPConfig } from '../../../src/utils/mcp-io.js';
import {
  readAllClientConfigs,
  analyzeSyncState,
} from '../../../src/features/sync.js';
import {
  getDirectorySizeBytes,
  formatBytes,
} from '@octocodeai/octocode-tools-core/fs-utils';

const { mockPaths } = vi.hoisted(() => ({
  mockPaths: {
    home: '/fake/octocode',
    tmp: '/fake/tmp',
    clone: '/fake/tmp/clone',
    tree: '/fake/tmp/tree',
    binary: '/fake/tmp/binary',
    unzip: '/fake/tmp/unzip',
    repos: '/fake/tmp/clone',
  },
}));

vi.mock('@octocodeai/octocode-tools-core/paths', () => ({
  paths: mockPaths,
}));

vi.mock('@octocodeai/octocode-tools-core/fs-utils', () => ({
  getDirectorySizeBytes: vi.fn().mockReturnValue(1024),
  formatBytes: vi.fn((b: number) => `${b} B`),
}));

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_tag: string, text: string) => text,
  bold: (text: string) => text,
  dim: (text: string) => text,
}));

vi.mock('../../../src/cli/commands/shared.js', () => ({
  formatAuthStatusAsJson: vi.fn(),
  printAuthStatus: vi.fn(() => console.log('AUTH_STATUS')),
}));

vi.mock('../../../src/utils/mcp-paths.js', () => ({
  DETECTABLE_MCP_CLIENTS: ['claude-code', 'cursor'],
  getMCPConfigPath: vi.fn((id: string) => `/cfg/${id}.json`),
  configFileExists: vi.fn().mockReturnValue(false),
  MCP_CLIENTS: {
    'claude-code': { name: 'Claude Code' },
    cursor: { name: 'Cursor' },
  },
}));

vi.mock('../../../src/utils/mcp-io.js', () => ({
  readMCPConfig: vi.fn(),
}));

vi.mock('../../../src/features/sync.js', () => ({
  readAllClientConfigs: vi.fn().mockReturnValue([]),
  analyzeSyncState: vi.fn(),
}));

async function loadCommand() {
  const mod = await import('../../../src/cli/commands/status.js');
  return mod.statusCommand;
}

describe('statusCommand', () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    mockPaths.home = '/fake/octocode';
    mockPaths.tmp = '/fake/tmp';
    mockPaths.clone = '/fake/tmp/clone';
    mockPaths.tree = '/fake/tmp/tree';
    mockPaths.binary = '/fake/tmp/binary';
    mockPaths.unzip = '/fake/tmp/unzip';
    mockPaths.repos = '/fake/tmp/clone';
    logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
    vi.mocked(getDirectorySizeBytes).mockReturnValue(1024);
    vi.mocked(formatBytes).mockImplementation((b: number) => `${b} B`);
    vi.mocked(getMCPConfigPath).mockImplementation(
      (id: string) => `/cfg/${id}.json`
    );
    vi.mocked(configFileExists).mockReturnValue(false);
    vi.mocked(formatAuthStatusAsJson).mockResolvedValue({
      authenticated: true,
      username: 'me',
      hostname: 'github.com',
      tokenPresent: true,
      tokenConfigured: true,
      tokenSource: 'env',
    });
    vi.mocked(printAuthStatus).mockImplementation(async () => {
      console.log('AUTH_STATUS');
    });
  });

  afterEach(() => {
    logSpy.mockRestore();
    process.exitCode = originalExitCode;
  });

  const out = (needle: string) =>
    logSpy.mock.calls.some((call: unknown[]) =>
      String(call.join(' ')).includes(needle)
    );

  it('exports expected command metadata', async () => {
    const cmd = await loadCommand();
    expect(cmd.name).toBe('status');
  });

  it('prints full status with no MCP configs found', async () => {
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: {} });
    expect(out('Octocode Status')).toBe(true);
    expect(out('AUTH_STATUS')).toBe(true);
    expect(out('MCP Clients')).toBe(true);
    expect(out('No MCP config files found.')).toBe(true);
    expect(out('Cache')).toBe(true);
    expect(out('clone:')).toBe(true);
    expect(out('tree:')).toBe(true);
    expect(out('binary:')).toBe(true);
    expect(out('unzip:')).toBe(true);
    expect(out('status --sync')).toBe(true);
    expect(process.exitCode).toBeUndefined();
  });

  it('prints installed clients with octocode installed and not installed', async () => {
    vi.mocked(configFileExists).mockReturnValue(true);
    vi.mocked(readMCPConfig).mockImplementation((p: string) => {
      if (p.includes('claude-code')) {
        return { mcpServers: { 'octocode-mcp': {}, other: {} } } as never;
      }
      return { mcpServers: { foo: {} } } as never;
    });
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: {} });
    expect(out('Claude Code')).toBe(true);
    expect(out('Cursor')).toBe(true);
    expect(out('2 MCPs')).toBe(true);
    expect(out('1 MCPs')).toBe(true);
    expect(out('2/2 configured')).toBe(true);
  });

  it('handles readMCPConfig returning null (no mcpServers)', async () => {
    vi.mocked(configFileExists).mockReturnValue(true);
    vi.mocked(readMCPConfig).mockReturnValue(null as never);
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: {} });
    expect(out('0 MCPs')).toBe(true);
  });

  it('uses MCP_CLIENTS name fallback to client id when name missing', async () => {
    (MCP_CLIENTS as Record<string, { name?: string }>)['cursor'] = {};
    vi.mocked(configFileExists).mockReturnValue(true);
    vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} } as never);
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: {} });
    expect(out('cursor')).toBe(true);
    (MCP_CLIENTS as Record<string, { name?: string }>)['cursor'] = {
      name: 'Cursor',
    };
  });

  it('--json outputs structured json and no exitCode when authenticated', async () => {
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: { json: true } });
    expect(out('"auth"')).toBe(true);
    expect(out('"tokenPresent"')).toBe(true);
    expect(out('"mcpClients"')).toBe(true);
    expect(out('"cache"')).toBe(true);
    expect(out('"totalBytes"')).toBe(true);
    expect(process.exitCode).toBeUndefined();
  });

  it('uses shared Octocode paths for cache reporting', async () => {
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: { json: true } });

    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/fake/tmp/clone');
    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/fake/tmp/tree');
    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/fake/tmp/binary');
    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/fake/tmp/unzip');

    const payload = JSON.parse(String(logSpy.mock.calls.at(-1)?.[0])) as {
      cache: {
        home: string;
        tmp: { path: string };
        clone: { path: string };
        tree: { path: string };
        binary: { path: string };
        unzip: { path: string };
      };
    };
    expect(payload.cache.home).toBe('/fake/octocode');
    expect(payload.cache.tmp.path).toBe('/fake/tmp');
    expect(payload.cache.clone.path).toBe('/fake/tmp/clone');
    expect(payload.cache.tree.path).toBe('/fake/tmp/tree');
    expect(payload.cache.binary.path).toBe('/fake/tmp/binary');
    expect(payload.cache.unzip.path).toBe('/fake/tmp/unzip');
  });

  it('--json reports unauthenticated state without failing read-only status', async () => {
    vi.mocked(formatAuthStatusAsJson).mockResolvedValue({
      authenticated: false,
      hostname: 'github.com',
    });
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: { json: true } });
    expect(out('"auth"')).toBe(true);
    expect(process.exitCode).toBeUndefined();
  });

  it('--sync (json) includes sync data', async () => {
    vi.mocked(analyzeSyncState).mockReturnValue({
      summary: {
        needsSyncCount: 1,
        conflictCount: 1,
        consistentMCPs: 2,
        totalUniqueMCPs: 4,
      },
      needsSync: [{ mcpId: 'm1', missingIn: ['cursor'] }],
      conflicts: [{ mcpId: 'm2', presentIn: ['claude-code'] }],
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'status',
      args: [],
      options: { json: true, sync: true },
    });
    expect(readAllClientConfigs).toHaveBeenCalled();
    expect(out('"sync"')).toBe(true);
    expect(out('"needsSyncCount"')).toBe(true);
  });

  it('--sync (pretty) prints synced/auto-sync/conflict lines', async () => {
    vi.mocked(analyzeSyncState).mockReturnValue({
      summary: {
        needsSyncCount: 3,
        conflictCount: 2,
        consistentMCPs: 5,
        totalUniqueMCPs: 10,
      },
      needsSync: [],
      conflicts: [],
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: { sync: true } });
    expect(out('Sync')).toBe(true);
    expect(out('5 fully synced')).toBe(true);
    expect(out('3 missing in some configs')).toBe(true);
    expect(out('2 conflicts across MCP configs')).toBe(true);
  });

  it('--sync (pretty) omits optional lines when counts are zero', async () => {
    vi.mocked(analyzeSyncState).mockReturnValue({
      summary: {
        needsSyncCount: 0,
        conflictCount: 0,
        consistentMCPs: 0,
        totalUniqueMCPs: 0,
      },
      needsSync: [],
      conflicts: [],
    } as never);
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: { sync: true } });
    expect(out('Sync')).toBe(true);
    expect(out('fully synced')).toBe(false);
    expect(out('auto-synced')).toBe(false);
    expect(out('conflicts')).toBe(false);
  });

  it('uses --hostname', async () => {
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'status',
      args: [],
      options: { hostname: 'ghe.corp.com' },
    });
    expect(formatAuthStatusAsJson).toHaveBeenCalledWith('ghe.corp.com');
  });
});
