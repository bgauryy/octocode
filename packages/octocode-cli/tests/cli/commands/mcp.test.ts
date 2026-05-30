

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('node:fs', () => ({
  existsSync: vi.fn().mockReturnValue(false),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  unlinkSync: vi.fn(),
  rmSync: vi.fn(),
  statSync: vi.fn(),
  symlinkSync: vi.fn(),
  promises: {
    readFile: vi.fn(),
    writeFile: vi.fn(),
    mkdir: vi.fn(),
    unlink: vi.fn(),
    stat: vi.fn(),
  },
}));

vi.mock('node:crypto', () => ({
  randomBytes: vi.fn().mockReturnValue(Buffer.alloc(32)),
  createCipheriv: vi.fn().mockReturnValue({
    update: vi.fn().mockReturnValue('encrypted'),
    final: vi.fn().mockReturnValue(''),
    getAuthTag: vi.fn().mockReturnValue(Buffer.alloc(16)),
  }),
  createDecipheriv: vi.fn().mockReturnValue({
    update: vi.fn().mockReturnValue('{}'),
    final: vi.fn().mockReturnValue(''),
    setAuthTag: vi.fn(),
  }),
}));

const MCP_REGISTRY_FIXTURE = [
  {
    id: 'test-mcp',
    name: 'Test MCP',
    description: 'A test MCP',
    category: 'developer-tools',
    repository: 'https://github.com/test/test',
    installationType: 'npx',
    installConfig: {
      command: 'npx',
      args: ['-y', 'test-mcp'],
    },
    tags: ['test'],
  },
  {
    id: 'another-mcp',
    name: 'Another MCP',
    description: 'Another test',
    category: 'database',
    repository: 'https://github.com/test/another',
    installationType: 'npx',
    installConfig: {
      command: 'npx',
      args: ['-y', 'another-mcp'],
      env: { API_KEY: 'default' },
    },
    tags: ['db'],
  },
];

const mcpIoMocks = vi.hoisted(() => ({
  readMCPConfig: vi.fn().mockReturnValue({ mcpServers: {} }),
  writeMCPConfig: vi.fn().mockReturnValue({ success: true }),
}));

const mcpPathsMocks = vi.hoisted(() => ({
  getMCPConfigPath: vi.fn().mockReturnValue('/fake/config.json'),
}));

vi.mock('../../../src/configs/mcp-registry.js', () => ({
  MCP_REGISTRY: MCP_REGISTRY_FIXTURE,
}));

vi.mock('../../../src/utils/mcp-paths.js', () => ({
  MCP_CLIENTS: {
    'claude-code': { name: 'Claude Code' },
    cursor: { name: 'Cursor' },
  },
  DETECTABLE_MCP_CLIENTS: ['claude-code', 'cursor'],
  getMCPConfigPath: mcpPathsMocks.getMCPConfigPath,
}));

vi.mock('../../../src/utils/mcp-io.js', () => ({
  readMCPConfig: mcpIoMocks.readMCPConfig,
  writeMCPConfig: mcpIoMocks.writeMCPConfig,
}));

describe('mcpCommand', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;

    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({ mcpServers: {} });
    vi.mocked(io.writeMCPConfig).mockReturnValue({ success: true });

    const paths = await import('../../../src/utils/mcp-paths.js');
    vi.mocked(paths.getMCPConfigPath).mockReturnValue('/fake/config.json');
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = originalExitCode;
  });

  async function loadCommand() {
    const mod = await import('../../../src/cli/commands/mcp.js');
    return mod.mcpCommand;
  }

  it('list: prints all registry entries', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Results:')
    );
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('test-mcp')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('another-mcp')
      )
    ).toBe(true);
    expect(process.exitCode).toBeUndefined();
  });

  it('list: filters by --search', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: { search: 'another' },
    });
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('another-mcp')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('test-mcp')
      )
    ).toBe(false);
  });

  it('list: filters by --category', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: { category: 'database' },
    });
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('another-mcp')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('test-mcp')
      )
    ).toBe(false);
  });

  it('list: --installed filters to MCPs present in config', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: {
        'test-mcp': { command: 'npx', args: ['-y', 'x'] },
      },
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: { installed: true },
    });
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('test-mcp')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('another-mcp')
      )
    ).toBe(false);
  });

  it('list: empty filter result shows no-match message', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: { category: 'nonexistent-category-xyz' },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('No MCP entries matched')
    );
  });

  it('status: prints installed MCP ids', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: {
        zoo: { command: 'z', args: [] },
        alpha: { command: 'a', args: [] },
      },
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['status'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Installed MCPs:')
    );
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('alpha')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) => String(c[0]).includes('zoo'))
    ).toBe(true);
  });

  it('status: empty config shows empty state message', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['status'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('No MCP servers configured yet.')
    );
  });

  it('install: missing --id errors', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Missing required option')
    );
  });

  it('install: MCP not found in registry errors', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'missing-mcp' },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('not found in registry')
    );
  });

  it('install: already installed without --force errors', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: {
        'test-mcp': { command: 'npx', args: ['-y', 'test-mcp'] },
      },
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'test-mcp' },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('already installed')
    );
  });

  it('install: succeeds and merges optional --env', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    const write = vi.mocked(io.writeMCPConfig);

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: {
        id: 'another-mcp',
        env: 'FOO=bar,BAZ=qux',
      },
    });

    expect(write).toHaveBeenCalledWith(
      '/fake/config.json',
      expect.objectContaining({
        mcpServers: expect.objectContaining({
          'another-mcp': expect.objectContaining({
            env: expect.objectContaining({
              API_KEY: 'default',
              FOO: 'bar',
              BAZ: 'qux',
            }),
          }),
        }),
      })
    );
    expect(process.exitCode).toBeUndefined();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Installed MCP:')
    );
  });

  it('install: invalid --env errors', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: {
        id: 'test-mcp',
        env: 'not-a-valid-pair',
      },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Invalid --env pair')
    );
  });

  it('install: write failure sets exit code', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.writeMCPConfig).mockReturnValue({
      success: false,
      error: 'disk full',
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'test-mcp', force: true },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Failed to write MCP config')
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('disk full')
    );
  });

  it('remove: missing --id errors', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['remove'],
      options: {},
    });
    expect(process.exitCode).toBe(1);
  });

  it('remove: MCP not installed warns and exits 1', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['remove'],
      options: { id: 'ghost' },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('MCP not installed')
    );
  });

  it('remove: succeeds case-insensitively', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: {
        'Test-MCP': { command: 'npx', args: [] },
      },
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['remove'],
      options: { id: 'test-mcp' },
    });
    expect(io.writeMCPConfig).toHaveBeenCalledWith(
      '/fake/config.json',
      expect.objectContaining({ mcpServers: {} })
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('remove: write failure sets exit code', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: {
        'test-mcp': { command: 'npx', args: [] },
      },
    });
    vi.mocked(io.writeMCPConfig).mockReturnValue({
      success: false,
      error: 'perm denied',
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['remove'],
      options: { id: 'test-mcp' },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Failed to update MCP config')
    );
  });

  it('unknown subcommand errors', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['oops'],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unknown mcp subcommand')
    );
  });

  it('invalid --client value errors', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: { client: 'not-a-real-client' },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Invalid --client value')
    );
  });

  it('custom --config uses custom client path', async () => {
    const paths = await import('../../../src/utils/mcp-paths.js');
    vi.mocked(paths.getMCPConfigPath).mockImplementation((client, custom) =>
      client === 'custom' && custom ? custom : '/fake/config.json'
    );

    const mcpCommand = await loadCommand();

    await mcpCommand.handler({
      command: 'mcp',
      args: ['status'],
      options: { config: '/custom/mcp.json' },
    });

    expect(paths.getMCPConfigPath).toHaveBeenCalledWith(
      'custom',
      '/custom/mcp.json'
    );
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('/custom/mcp.json')
    );
  });

  it('defaults to list subcommand', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: [],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('MCP Marketplace')
    );
  });

  it('valid --client normalizes target client', async () => {
    const paths = await import('../../../src/utils/mcp-paths.js');
    const mcpCommand = await loadCommand();

    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: { client: 'cursor' },
    });

    expect(paths.getMCPConfigPath).toHaveBeenCalledWith('cursor', undefined);
  });
});
