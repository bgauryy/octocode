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
import { getSkillsCacheDir } from '../../../src/utils/skills-fetch.js';
import {
  readAllClientConfigs,
  analyzeSyncState,
} from '../../../src/features/sync.js';
import { getDirectorySizeBytes, formatBytes } from 'octocode-shared';

const { mockPaths } = vi.hoisted(() => ({
  mockPaths: {
    home: '/fake/octocode',
    repos: '/fake/repos',
    logs: '/fake/logs',
  },
}));

vi.mock('octocode-shared', () => ({
  paths: mockPaths,
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

vi.mock('../../../src/utils/skills-fetch.js', () => ({
  getSkillsCacheDir: vi.fn().mockReturnValue('/fake/skills'),
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
  let originalHome: string | undefined;
  let originalOctocodeHome: string | undefined;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    mockPaths.home = '/fake/octocode';
    mockPaths.repos = '/fake/repos';
    mockPaths.logs = '/fake/logs';
    originalHome = process.env.HOME;
    originalOctocodeHome = process.env.OCTOCODE_HOME;
    delete process.env.OCTOCODE_HOME;
    logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
    vi.mocked(getDirectorySizeBytes).mockReturnValue(1024);
    vi.mocked(formatBytes).mockImplementation((b: number) => `${b} B`);
    vi.mocked(getSkillsCacheDir).mockReturnValue('/fake/skills');
    vi.mocked(getMCPConfigPath).mockImplementation(
      (id: string) => `/cfg/${id}.json`
    );
    vi.mocked(configFileExists).mockReturnValue(false);
    vi.mocked(formatAuthStatusAsJson).mockReturnValue({
      authenticated: true,
      username: 'me',
      hostname: 'github.com',
    });
    vi.mocked(printAuthStatus).mockImplementation(() =>
      console.log('AUTH_STATUS')
    );
  });

  afterEach(() => {
    logSpy.mockRestore();
    process.exitCode = originalExitCode;
    if (originalHome === undefined) delete process.env.HOME;
    else process.env.HOME = originalHome;
    if (originalOctocodeHome === undefined) delete process.env.OCTOCODE_HOME;
    else process.env.OCTOCODE_HOME = originalOctocodeHome;
  });

  const out = (needle: string) =>
    logSpy.mock.calls.some((call: unknown[]) =>
      String(call.join(' ')).includes(needle)
    );

  it('exports expected command metadata', async () => {
    const cmd = await loadCommand();
    expect(cmd.name).toBe('status');
    expect(cmd.aliases).toContain('s');
  });

  it('prints full status with no MCP configs found', async () => {
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: {} });
    expect(out('Octocode Status')).toBe(true);
    expect(out('AUTH_STATUS')).toBe(true);
    expect(out('MCP Clients')).toBe(true);
    expect(out('No MCP config files found.')).toBe(true);
    expect(out('Cache')).toBe(true);
    expect(out('repos:')).toBe(true);
    expect(out('skills:')).toBe(true);
    expect(out('logs:')).toBe(true);
    expect(out('octocode status --sync')).toBe(true);
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
    expect(out('"mcpClients"')).toBe(true);
    expect(out('"cache"')).toBe(true);
    expect(out('"totalBytes"')).toBe(true);
    expect(process.exitCode).toBeUndefined();
  });

  it('--json -j alias works and sets exitCode 1 when not authenticated', async () => {
    vi.mocked(formatAuthStatusAsJson).mockReturnValue({
      authenticated: false,
      hostname: 'github.com',
    });
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: { j: true } });
    expect(out('"auth"')).toBe(true);
    expect(process.exitCode).toBe(1);
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
    expect(out('3 can be auto-synced')).toBe(true);
    expect(out('2 conflicts')).toBe(true);
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

  it('uses --hostname and -H alias', async () => {
    const cmd = await loadCommand();
    await cmd.handler({
      command: 'status',
      args: [],
      options: { hostname: 'ghe.corp.com' },
    });
    expect(formatAuthStatusAsJson).toHaveBeenCalledWith('ghe.corp.com');

    vi.clearAllMocks();
    vi.mocked(formatAuthStatusAsJson).mockReturnValue({
      authenticated: true,
      hostname: 'h',
    });
    const cmd2 = await loadCommand();
    await cmd2.handler({
      command: 'status',
      args: [],
      options: { H: 'alias.corp.com' },
    });
    expect(formatAuthStatusAsJson).toHaveBeenCalledWith('alias.corp.com');
  });

  it('falls back to OCTOCODE_HOME when paths.home falsy', async () => {
    mockPaths.home = '';
    mockPaths.repos = '';
    mockPaths.logs = '';
    process.env.OCTOCODE_HOME = '/env/oct';
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: {} });
    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/env/oct/repos');
    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/env/oct/logs');
  });

  it('falls back to HOME/.octocode when paths.home and OCTOCODE_HOME unset', async () => {
    mockPaths.home = '';
    mockPaths.repos = '';
    mockPaths.logs = '';
    process.env.HOME = '/users/tester';
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: {} });
    expect(getDirectorySizeBytes).toHaveBeenCalledWith(
      '/users/tester/.octocode/repos'
    );
  });

  it('falls back to relative .octocode when HOME unset', async () => {
    mockPaths.home = '';
    mockPaths.repos = '';
    mockPaths.logs = '';
    delete process.env.HOME;
    const cmd = await loadCommand();
    await cmd.handler({ command: 'status', args: [], options: {} });
    expect(
      vi
        .mocked(getDirectorySizeBytes)
        .mock.calls.some(([dir]) => String(dir).includes('.octocode'))
    ).toBe(true);
  });
});
