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

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_tag: string, text: string) => text,
  bold: (text: string) => text,
  dim: (text: string) => text,
}));

vi.mock('../../../src/ui/constants.js', () => ({
  CLIENT_INFO: {
    cursor: { name: 'Cursor Client' },
  },
  IDE_INFO: {
    windsurf: { name: 'Windsurf IDE' },
  },
}));

const authMocks = vi.hoisted(() => ({
  getAuthStatusAsync: vi.fn().mockResolvedValue({
    authenticated: false,
    hostname: 'github.com',
  }),
  getStoragePath: vi.fn().mockReturnValue('/home/test/.octocode/credentials'),
}));

vi.mock('../../../src/features/github-oauth.js', () => ({
  getAuthStatusAsync: authMocks.getAuthStatusAsync,
  getStoragePath: authMocks.getStoragePath,
}));

describe('cli/commands/shared', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;
  let originalIsTTY: boolean | undefined;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    authMocks.getAuthStatusAsync.mockResolvedValue({
      authenticated: false,
      hostname: 'github.com',
    });
    authMocks.getStoragePath.mockReturnValue(
      '/home/test/.octocode/credentials'
    );
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
    originalIsTTY = process.stdout.isTTY;
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = originalExitCode;
    Object.defineProperty(process.stdout, 'isTTY', {
      value: originalIsTTY,
      configurable: true,
      writable: true,
    });
  });

  describe('MCP_CLIENT_IDS / normalizeMCPClient', () => {
    it('maps every canonical id to itself (case-insensitive, trimmed)', async () => {
      const { MCP_CLIENT_IDS, normalizeMCPClient } =
        await import('../../../src/cli/commands/shared.js');

      for (const [clientId, expected] of Object.entries(MCP_CLIENT_IDS)) {
        expect(normalizeMCPClient(clientId)).toBe(expected);
        expect(normalizeMCPClient(`  ${clientId.toUpperCase()}  `)).toBe(
          expected
        );
      }
    });

    it('returns null for unknown ids', async () => {
      const { normalizeMCPClient } =
        await import('../../../src/cli/commands/shared.js');
      expect(normalizeMCPClient('unknown-ide')).toBeNull();
      expect(normalizeMCPClient('')).toBeNull();
    });
  });

  describe('getIDEDisplayName', () => {
    it('uses CLIENT_INFO name when present', async () => {
      const { getIDEDisplayName } =
        await import('../../../src/cli/commands/shared.js');
      expect(getIDEDisplayName('cursor')).toBe('Cursor Client');
    });

    it('uses IDE_INFO name when not in CLIENT_INFO', async () => {
      const { getIDEDisplayName } =
        await import('../../../src/cli/commands/shared.js');
      expect(getIDEDisplayName('windsurf')).toBe('Windsurf IDE');
    });

    it('capitalizes unknown IDE keys', async () => {
      const { getIDEDisplayName } =
        await import('../../../src/cli/commands/shared.js');
      expect(getIDEDisplayName('foobar')).toBe('Foobar');
    });
  });

  describe('formatSupportedMCPClients', () => {
    it('formats canonical clients from the MCP registry', async () => {
      const { formatSupportedMCPClients } =
        await import('../../../src/cli/commands/shared.js');

      expect(formatSupportedMCPClients()).toContain('cursor');
      expect(formatSupportedMCPClients()).toContain('claude-code');
      expect(formatSupportedMCPClients()).not.toContain('custom');
    });

    it('does not add install shortcuts', async () => {
      const { formatSupportedMCPClients } =
        await import('../../../src/cli/commands/shared.js');

      expect(formatSupportedMCPClients().split(', ')).not.toContain('claude');
    });
  });

  describe('maskToken', () => {
    it('masks short tokens as asterisks only', async () => {
      const { maskToken } = await import('../../../src/cli/commands/shared.js');
      expect(maskToken('')).toBe('****');
      expect(maskToken('12345678')).toBe('****');
      expect(maskToken('short')).toBe('****');
    });

    it('masks long tokens with head and tail', async () => {
      const { maskToken } = await import('../../../src/cli/commands/shared.js');
      expect(maskToken('123456789')).toBe('1234****6789');
      expect(maskToken('gho_abcdefghijklmnopqrstuvwxyz')).toBe('gho_****wxyz');
    });
  });

  describe('safeTokenOutput', () => {
    it('returns raw token when stdout is not a TTY', async () => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: false,
        configurable: true,
      });
      const { safeTokenOutput } =
        await import('../../../src/cli/commands/shared.js');
      const raw = 'gho_super_secret_token_value';
      expect(safeTokenOutput(raw)).toBe(raw);
    });

    it('masks token when stdout is a TTY', async () => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: true,
        configurable: true,
      });
      const { safeTokenOutput } =
        await import('../../../src/cli/commands/shared.js');
      expect(safeTokenOutput('gho_abcdefghijklmnopqrstuvwxyz')).toBe(
        'gho_****wxyz'
      );
    });
  });

  describe('printLoginHint', () => {
    it('prints login hints', async () => {
      const { printLoginHint } =
        await import('../../../src/cli/commands/shared.js');
      printLoginHint();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('To login')
      );
      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('login'));
      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('or'));
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('gh auth login')
      );
    });
  });

  describe('printAuthStatus', () => {
    it('prints the shared authenticated status shape', async () => {
      authMocks.getAuthStatusAsync.mockResolvedValue({
        authenticated: true,
        hostname: 'github.com',
        username: 'octo',
        tokenSource: 'octocode',
      });
      const { printAuthStatus } =
        await import('../../../src/cli/commands/shared.js');

      await printAuthStatus();

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('GitHub Authentication')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Authenticated as')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Credentials stored in')
      );
    });

    it('prints login hints for unauthenticated status', async () => {
      authMocks.getAuthStatusAsync.mockResolvedValue({
        authenticated: false,
        hostname: 'github.com',
      });
      const { printAuthStatus } =
        await import('../../../src/cli/commands/shared.js');

      await printAuthStatus();

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Not authenticated')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('anonymous requests may work')
      );
      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('login'));
    });
  });

  describe('printNodeDoctorHintCLI', () => {
    it('prints node-doctor hint and blank line', async () => {
      const { printNodeDoctorHintCLI } =
        await import('../../../src/cli/commands/shared.js');
      printNodeDoctorHintCLI();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('npx node-doctor')
      );
      expect(consoleSpy).toHaveBeenCalledWith();
    });
  });

  describe('formatTokenSource', () => {
    it('formats octocode, gh-cli, env branches, and default', async () => {
      const { formatTokenSource } =
        await import('../../../src/cli/commands/shared.js');
      expect(formatTokenSource('octocode')).toBe('octocode');
      expect(formatTokenSource('gh-cli')).toBe('gh cli');
      expect(formatTokenSource('env', 'env:GITHUB_TOKEN')).toBe('GITHUB_TOKEN');
      expect(formatTokenSource('env')).toBe('environment variable');
      expect(formatTokenSource('none')).toBe('none');
    });
  });

  describe('parseMCPEnv', () => {
    it('returns empty values for undefined or blank input', async () => {
      const { parseMCPEnv } =
        await import('../../../src/cli/commands/shared.js');
      expect(parseMCPEnv(undefined)).toEqual({ values: {} });
      expect(parseMCPEnv('')).toEqual({ values: {} });
      expect(parseMCPEnv('   ')).toEqual({ values: {} });
    });

    it('parses single and multiple KEY=VALUE pairs', async () => {
      const { parseMCPEnv } =
        await import('../../../src/cli/commands/shared.js');
      expect(parseMCPEnv('FOO=bar')).toEqual({
        values: { FOO: 'bar' },
      });
      expect(parseMCPEnv('A=1,B=two')).toEqual({
        values: { A: '1', B: 'two' },
      });
      expect(parseMCPEnv(' KEY = value with spaces ')).toEqual({
        values: { KEY: ' value with spaces' },
      });
    });

    it('errors on invalid pairs and invalid variable names', async () => {
      const { parseMCPEnv } =
        await import('../../../src/cli/commands/shared.js');
      expect(parseMCPEnv('noequals')).toMatchObject({
        values: {},
        error: expect.stringContaining('Invalid --env pair'),
      });
      expect(parseMCPEnv('=onlyvalue')).toMatchObject({
        values: {},
        error: expect.stringContaining('Invalid --env pair'),
      });
      expect(parseMCPEnv('123BAD=x')).toMatchObject({
        values: {},
        error: expect.stringContaining('Invalid env var name'),
      });
    });

    it('accepts valid names with digits after first character', async () => {
      const { parseMCPEnv } =
        await import('../../../src/cli/commands/shared.js');
      expect(parseMCPEnv('_V1=y')).toEqual({ values: { _V1: 'y' } });
    });
  });
});

describe('statusCommand', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    authMocks.getAuthStatusAsync.mockResolvedValue({
      authenticated: false,
      hostname: 'github.com',
    });
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    process.exitCode = undefined;
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = undefined;
  });

  it('outputs JSON when --json is provided', async () => {
    const { statusCommand } =
      await import('../../../src/cli/commands/status.js');

    await statusCommand.handler({
      command: 'status',
      args: [],
      options: { json: true },
    });

    const allOutput = consoleSpy.mock.calls.flat().join('\n');
    const jsonLine = allOutput.split('\n').find((line: string) => {
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine!);
    const auth = parsed.auth ?? parsed;
    expect(typeof auth.authenticated).toBe('boolean');
    expect(typeof auth.hostname).toBe('string');
  });

  it('does not fail read-only --json status when not authenticated', async () => {
    const { statusCommand } =
      await import('../../../src/cli/commands/status.js');

    await statusCommand.handler({
      command: 'status',
      args: [],
      options: { json: true },
    });

    expect(process.exitCode).toBeUndefined();
  });
});
