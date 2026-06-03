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

const { httpRequestMock } = vi.hoisted(() => ({
  httpRequestMock: vi.fn(),
}));

// Default: HEAD request succeeds (response callback fires immediately).
function makeReachableRequest() {
  httpRequestMock.mockImplementation(
    (_opts: unknown, cb: (res: unknown) => void) => {
      const req = {
        on: vi.fn(),
        destroy: vi.fn(),
        end: vi.fn(),
      };
      if (cb) cb({});
      return req;
    }
  );
}

vi.mock('node:https', () => ({
  default: { request: httpRequestMock },
  request: httpRequestMock,
}));

vi.mock('node:http', () => ({
  default: { request: httpRequestMock },
  request: httpRequestMock,
}));

const MCP_REGISTRY_FIXTURE = [
  {
    id: 'test-mcp',
    name: 'Test MCP',
    description: 'A test MCP',
    category: 'developer-tools',
    repository: 'https://github.com/test/test',
    installationType: 'npx',
    npmPackage: '@scope/test-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'test-mcp'],
    },
    tags: ['test'],
    requiredEnvVars: [
      { name: 'TEST_TOKEN', description: 'A token' },
      { name: 'TEST_OPTIONAL', description: 'Optional thing' },
    ],
  },
  {
    id: 'another-mcp',
    name: 'Another MCP',
    description: 'Another test',
    category: 'database',
    repository: 'https://github.com/test/another',
    installationType: 'npx',
    npmPackage: 'another-mcp',
    installConfig: {
      command: 'npx',
      args: ['-y', 'another-mcp'],
      env: { API_KEY: 'default' },
    },
    tags: ['db'],
  },
  {
    id: 'py-mcp',
    name: 'Python MCP',
    description: 'A python MCP',
    category: 'developer-tools',
    repository: 'https://github.com/test/py',
    installationType: 'uvx',
    pythonPackage: 'py-mcp-pkg',
    installConfig: {
      command: 'uvx',
      args: ['py-mcp-pkg'],
    },
    tags: ['py'],
  },
  {
    id: 'local-mcp',
    name: 'Local MCP',
    description: 'A local binary MCP',
    category: 'developer-tools',
    repository: 'https://github.com/test/local',
    installationType: 'binary',
    installConfig: {
      command: 'node',
      args: ['server.js'],
    },
    tags: ['local'],
  },
  {
    id: 'repo-only-mcp',
    name: 'Repo Only MCP',
    description: 'Falls back to repo url',
    category: 'developer-tools',
    repository: 'https://github.com/test/repo-only',
    installationType: 'npx',
    installConfig: {
      command: 'npx',
      args: ['-y', 'repo-only-mcp'],
    },
    tags: ['repo'],
  },
];

const mcpIoMocks = vi.hoisted(() => ({
  readMCPConfig: vi.fn().mockReturnValue({ mcpServers: {} }),
  writeMCPConfig: vi.fn().mockReturnValue({ success: true }),
}));

const mcpPathsMocks = vi.hoisted(() => ({
  getMCPConfigPath: vi.fn().mockReturnValue('/fake/config.json'),
  configFileExists: vi.fn().mockReturnValue(false),
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
  configFileExists: mcpPathsMocks.configFileExists,
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
    vi.mocked(paths.configFileExists).mockReturnValue(false);

    httpRequestMock.mockReset();
    makeReachableRequest();
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = originalExitCode;
  });

  async function loadCommand() {
    const mod = await import('../../../src/cli/commands/mcp.js');
    return mod.mcpCommand;
  }

  it('list: scans OS config files by default', async () => {
    const paths = await import('../../../src/utils/mcp-paths.js');
    vi.mocked(paths.configFileExists).mockImplementation(
      (client: string) => client === 'cursor'
    );
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: { 'test-mcp': { command: 'npx', args: [] } },
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('MCP Configs on OS')
    );
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('test-mcp')
      )
    ).toBe(true);
    expect(process.exitCode).toBeUndefined();
  });

  it('list: shows no-configs message when none found', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('No MCP config files found')
    );
  });

  it('list: prints registry entries when --client is provided', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: { client: 'cursor' },
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
      expect.stringContaining('Installed:')
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

  it('defaults to list subcommand (OS scan)', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: [],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('MCP Configs on OS')
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

  it('list --json (OS scan) outputs configs array', async () => {
    const paths = await import('../../../src/utils/mcp-paths.js');
    vi.mocked(paths.configFileExists).mockImplementation(
      (client: string) => client === 'cursor'
    );
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: { 'test-mcp': { command: 'npx', args: [] } },
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: { json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(Array.isArray(parsed.configs)).toBe(true);
    expect(
      parsed.configs.some((c: { client: string }) => c.client === 'cursor')
    ).toBe(true);
  });

  it('list (OS scan) shows "no servers configured" for empty existing config', async () => {
    const paths = await import('../../../src/utils/mcp-paths.js');
    vi.mocked(paths.configFileExists).mockImplementation(
      (client: string) => client === 'cursor'
    );
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({ mcpServers: {} });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: {},
    });

    expect(
      consoleSpy.mock.calls.some(c =>
        String(c[0]).includes('(no servers configured)')
      )
    ).toBe(true);
  });

  it('list --json with --client outputs registry results', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['list'],
      options: { client: 'cursor', json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.client).toBe('cursor');
    expect(
      parsed.results.some((r: { id: string }) => r.id === 'test-mcp')
    ).toBe(true);
    // requiredEnvVars only present in installedOnly mode
    expect(parsed.results[0].requiredEnvVars).toBeUndefined();
  });

  it('list --installed --json includes requiredEnvVars with status', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: { 'test-mcp': { command: 'npx', args: [] } },
    });
    process.env.TEST_TOKEN = 'present';
    delete process.env.TEST_OPTIONAL;

    try {
      const mcpCommand = await loadCommand();
      await mcpCommand.handler({
        command: 'mcp',
        args: ['list'],
        options: { installed: true, json: true },
      });

      const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
      const parsed = JSON.parse(out.trim());
      const entry = parsed.results.find(
        (r: { id: string }) => r.id === 'test-mcp'
      );
      const envs = entry.requiredEnvVars;
      expect(
        envs.find((e: { name: string }) => e.name === 'TEST_TOKEN').status
      ).toBe('set');
      expect(
        envs.find((e: { name: string }) => e.name === 'TEST_OPTIONAL').status
      ).toBe('missing');
    } finally {
      delete process.env.TEST_TOKEN;
    }
  });

  it('list --installed (non-json) prints env var icons for installed entry', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: { 'test-mcp': { command: 'npx', args: [] } },
    });
    process.env.TEST_TOKEN = 'present';
    delete process.env.TEST_OPTIONAL;

    try {
      const mcpCommand = await loadCommand();
      await mcpCommand.handler({
        command: 'mcp',
        args: ['list'],
        options: { installed: true },
      });

      expect(
        consoleSpy.mock.calls.some(c => String(c[0]).includes('TEST_TOKEN'))
      ).toBe(true);
      expect(
        consoleSpy.mock.calls.some(c => String(c[0]).includes('(missing)'))
      ).toBe(true);
    } finally {
      delete process.env.TEST_TOKEN;
    }
  });

  it('status --json outputs sorted server ids', async () => {
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
      options: { json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.servers).toEqual(['alpha', 'zoo']);
  });

  it('install: missing --id --json outputs error object', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(false);
    expect(parsed.error).toContain('Missing required option');
    expect(process.exitCode).toBe(1);
  });

  it('install: invalid --env --json outputs error object', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'test-mcp', env: 'bad-env', json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(false);
    expect(process.exitCode).toBe(1);
  });

  it('install: single success --json outputs success object', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'py-mcp', json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(true);
    expect(parsed.id).toBe('py-mcp');
    expect(process.exitCode).toBeUndefined();
  });

  it('install: python package preflights PyPI url', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'py-mcp' },
    });

    const opts = httpRequestMock.mock.calls[0][0];
    expect(opts.hostname).toBe('pypi.org');
  });

  it('install: npm package preflights npm registry url', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'test-mcp' },
    });

    const opts = httpRequestMock.mock.calls[0][0];
    expect(opts.hostname).toBe('registry.npmjs.org');
  });

  it('install: repo-only entry preflights repository url', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'repo-only-mcp' },
    });

    const opts = httpRequestMock.mock.calls[0][0];
    expect(opts.hostname).toBe('github.com');
  });

  it('install: non-internet command skips preflight', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'local-mcp' },
    });

    expect(httpRequestMock).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Installed:')
    );
  });

  it('install: preflight failure (request error) blocks install without --force', async () => {
    httpRequestMock.mockReset();
    httpRequestMock.mockImplementation(() => {
      const req = {
        on: vi.fn((event: string, handler: () => void) => {
          if (event === 'error') handler();
          return req;
        }),
        destroy: vi.fn(),
        end: vi.fn(),
      };
      return req;
    });

    const io = await import('../../../src/utils/mcp-io.js');
    const write = vi.mocked(io.writeMCPConfig);

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'test-mcp' },
    });

    expect(write).not.toHaveBeenCalled();
    expect(process.exitCode).toBe(1);
    expect(
      consoleSpy.mock.calls.some(c =>
        String(c[0]).includes('Pre-flight check failed')
      )
    ).toBe(true);
  });

  it('install: preflight timeout destroys request and reports failure', async () => {
    httpRequestMock.mockReset();
    httpRequestMock.mockImplementation(() => {
      const req = {
        on: vi.fn((event: string, handler: () => void) => {
          if (event === 'timeout') handler();
          return req;
        }),
        destroy: vi.fn(),
        end: vi.fn(),
      };
      return req;
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'test-mcp' },
    });

    expect(process.exitCode).toBe(1);
    expect(
      consoleSpy.mock.calls.some(c => String(c[0]).includes('unreachable'))
    ).toBe(true);
  });

  it('install: preflight URL parse throw resolves unreachable', async () => {
    httpRequestMock.mockReset();
    httpRequestMock.mockImplementation(() => {
      throw new Error('boom');
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'test-mcp' },
    });

    expect(process.exitCode).toBe(1);
  });

  it('install: --force skips preflight failure and installs', async () => {
    httpRequestMock.mockReset();
    httpRequestMock.mockImplementation(() => {
      const req = {
        on: vi.fn((event: string, handler: () => void) => {
          if (event === 'error') handler();
          return req;
        }),
        destroy: vi.fn(),
        end: vi.fn(),
      };
      return req;
    });

    const io = await import('../../../src/utils/mcp-io.js');
    const write = vi.mocked(io.writeMCPConfig);

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'test-mcp', force: true },
    });

    expect(write).toHaveBeenCalled();
    expect(process.exitCode).toBeUndefined();
  });

  it('install: batch with mixed success/failure --json outputs results array', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({ mcpServers: {} });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'py-mcp,local-mcp,ghost-mcp', json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(Array.isArray(parsed.results)).toBe(true);
    const ghost = parsed.results.find(
      (r: { id: string }) => r.id === 'ghost-mcp'
    );
    expect(ghost.success).toBe(false);
    expect(process.exitCode).toBe(1);
  });

  it('install: batch (non-json) prints summary footer', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'py-mcp,local-mcp' },
    });

    expect(
      consoleSpy.mock.calls.some(c =>
        String(c[0]).includes('Installed: py-mcp')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some(c => String(c[0]).includes('Config:'))
    ).toBe(true);
  });

  it('install: write failure --json outputs error', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.writeMCPConfig).mockReturnValue({
      success: false,
      error: 'disk full',
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'local-mcp', json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(false);
    expect(parsed.error).toBe('disk full');
    expect(process.exitCode).toBe(1);
  });

  it('install: write returns no error message uses default text', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.writeMCPConfig).mockReturnValue({ success: false });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['install'],
      options: { id: 'local-mcp' },
    });

    expect(
      consoleSpy.mock.calls.some(c =>
        String(c[0]).includes('Failed to write MCP config')
      )
    ).toBe(true);
    expect(process.exitCode).toBe(1);
  });

  it('remove: missing --id --json outputs error', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['remove'],
      options: { json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(false);
    expect(process.exitCode).toBe(1);
  });

  it('remove: not installed --json outputs error', async () => {
    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['remove'],
      options: { id: 'ghost', json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.error).toContain('MCP not installed');
    expect(process.exitCode).toBe(1);
  });

  it('remove: write failure --json outputs error', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: { 'test-mcp': { command: 'npx', args: [] } },
    });
    vi.mocked(io.writeMCPConfig).mockReturnValue({
      success: false,
      error: 'perm denied',
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['remove'],
      options: { id: 'test-mcp', json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(false);
    expect(parsed.error).toBe('perm denied');
    expect(process.exitCode).toBe(1);
  });

  it('remove: success --json outputs success object', async () => {
    const io = await import('../../../src/utils/mcp-io.js');
    vi.mocked(io.readMCPConfig).mockReturnValue({
      mcpServers: { 'test-mcp': { command: 'npx', args: [] } },
    });

    const mcpCommand = await loadCommand();
    await mcpCommand.handler({
      command: 'mcp',
      args: ['remove'],
      options: { id: 'test-mcp', json: true },
    });

    const out = consoleSpy.mock.calls.map(c => String(c[0])).join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(true);
    expect(parsed.id).toBe('test-mcp');
  });
});
