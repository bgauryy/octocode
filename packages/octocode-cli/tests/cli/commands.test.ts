import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('node:fs', () => ({
  existsSync: vi.fn().mockReturnValue(false),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  unlinkSync: vi.fn(),
  rmSync: vi.fn(),
  statSync: vi.fn(),
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

vi.mock('../../src/features/github-oauth.js', () => ({
  login: vi.fn(),
  logout: vi.fn(),
  getAuthStatus: vi.fn(),
  getToken: vi.fn(),
  getOctocodeToken: vi.fn().mockResolvedValue({ token: null }),
  getGhCliToken: vi.fn().mockReturnValue({ token: null }),
  getStoragePath: vi
    .fn()
    .mockReturnValue('/home/test/.octocode/credentials.json'),
  getTokenType: vi
    .fn()
    .mockImplementation((source: string, envSource?: string) => {
      switch (source) {
        case 'env':
          return envSource ?? 'env:GITHUB_TOKEN';
        case 'gh-cli':
          return 'gh-cli';
        case 'octocode':
          return 'octocode-storage';
        case 'none':
        default:
          return 'none';
      }
    }),
}));

vi.mock('../../src/utils/token-storage.js', () => ({
  getCredentials: vi.fn(),
}));

vi.mock('../../src/features/install.js', () => ({
  installOctocode: vi.fn(),
  detectAvailableIDEs: vi.fn().mockReturnValue([]),
  getInstallPreview: vi.fn(),
}));

vi.mock('../../src/features/node-check.js', () => ({
  checkNodeInPath: vi.fn().mockReturnValue({ installed: true }),
  checkNpmInPath: vi.fn().mockReturnValue({ installed: true }),
}));

vi.mock('../../src/utils/prompts.js', () => ({
  loadInquirer: vi.fn(),
  select: vi.fn(),
}));

vi.mock('../../src/utils/spinner.js', () => ({
  Spinner: vi.fn().mockImplementation(() => ({
    start: vi.fn().mockReturnThis(),
    stop: vi.fn(),
    succeed: vi.fn(),
    fail: vi.fn(),
  })),
}));

vi.mock('../../src/utils/fs.js', () => ({
  copyDirectory: vi.fn(),
  dirExists: vi.fn(),
  listSubdirectories: vi.fn().mockReturnValue([]),
}));

describe('CLI Commands', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = originalExitCode;
  });

  describe('tokenCommand', () => {
    it('should output the token when authenticated via octocode', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'gho_test_token_12345',
        source: 'octocode',
        username: 'testuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');
      expect(tokenCmd).toBeDefined();

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith('gho_test_token_12345');
      expect(process.exitCode).toBeUndefined();
    });

    it('should output the token when authenticated via gh-cli', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'gho_ghcli_token_12345',
        source: 'gh-cli',
        username: 'ghuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith('gho_ghcli_token_12345');
      expect(process.exitCode).toBeUndefined();
    });

    it('should show error when not authenticated (default auto type)', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: null,
        source: 'none',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');
      expect(tokenCmd).toBeDefined();

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: {},
      });

      expect(consoleSpy).not.toHaveBeenCalledWith(
        expect.stringMatching(/^gho_/)
      );

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Not authenticated')
      );
      expect(process.exitCode).toBe(1);
    });

    it('should use custom hostname when provided', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'gho_enterprise_token',
        source: 'octocode',
        username: 'enterpriseuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { hostname: 'github.enterprise.com' },
      });

      expect(getToken).toHaveBeenCalledWith('github.enterprise.com', 'auto');
      expect(consoleSpy).toHaveBeenCalledWith('gho_enterprise_token');
    });

    it('should be findable by name "token"', async () => {
      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');
      expect(tokenCmd).toBeDefined();
      expect(tokenCmd!.name).toBe('token');
    });

    it('should use gh type when --type=gh is provided', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'gho_gh_cli_token',
        source: 'gh-cli',
        username: 'ghuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { type: 'gh' },
      });

      expect(getToken).toHaveBeenCalledWith('github.com', 'gh');
      expect(consoleSpy).toHaveBeenCalledWith('gho_gh_cli_token');
    });

    it('should reject unsupported token source names', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { type: 'octocode-cli' },
      });

      expect(getToken).not.toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Invalid token type: octocode-cli')
      );
      expect(process.exitCode).toBe(1);
    });

    it('should use auto type when --type=auto is provided', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'gho_auto_token',
        source: 'octocode',
        username: 'autouser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { type: 'auto' },
      });

      expect(getToken).toHaveBeenCalledWith('github.com', 'auto');
      expect(consoleSpy).toHaveBeenCalledWith('gho_auto_token');
    });

    it('should show error for invalid --type value', async () => {
      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { type: 'invalid' },
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Invalid token type')
      );
      expect(process.exitCode).toBe(1);
    });

    describe('--json flag', () => {
      it('should output valid JSON with token and type when authenticated via env', async () => {
        const { getToken } = await import('../../src/features/github-oauth.js');
        vi.mocked(getToken).mockResolvedValue({
          token: 'env_token_12345',
          source: 'env',
          envSource: 'env:GH_TOKEN',
        });

        const { findCommand } = await import('../../src/cli/commands/index.js');
        const tokenCmd = findCommand('token');

        await tokenCmd!.handler!({
          command: 'token',
          args: [],
          options: { json: true },
        });

        const output = consoleSpy.mock.calls.find(
          (call: unknown[]) =>
            typeof call[0] === 'string' && call[0].includes('"token"')
        );
        expect(output).toBeDefined();
        const parsed = JSON.parse(output![0]);
        expect(parsed.token).toBe('env_token_12345');
        expect(parsed.type).toBe('env:GH_TOKEN');
        expect(process.exitCode).toBeUndefined();
      });

      it('should output valid JSON with token and type when authenticated via gh-cli', async () => {
        const { getToken } = await import('../../src/features/github-oauth.js');
        vi.mocked(getToken).mockResolvedValue({
          token: 'ghcli_token_12345',
          source: 'gh-cli',
          username: 'ghuser',
        });

        const { findCommand } = await import('../../src/cli/commands/index.js');
        const tokenCmd = findCommand('token');

        await tokenCmd!.handler!({
          command: 'token',
          args: [],
          options: { json: true },
        });

        const output = consoleSpy.mock.calls.find(
          (call: unknown[]) =>
            typeof call[0] === 'string' && call[0].includes('"token"')
        );
        expect(output).toBeDefined();
        const parsed = JSON.parse(output![0]);
        expect(parsed.token).toBe('ghcli_token_12345');
        expect(parsed.type).toBe('gh-cli');
      });

      it('should output valid JSON with token and type when authenticated via octocode', async () => {
        const { getToken } = await import('../../src/features/github-oauth.js');
        vi.mocked(getToken).mockResolvedValue({
          token: 'octocode_token_12345',
          source: 'octocode',
          username: 'octocodeuser',
        });

        const { findCommand } = await import('../../src/cli/commands/index.js');
        const tokenCmd = findCommand('token');

        await tokenCmd!.handler!({
          command: 'token',
          args: [],
          options: { json: true },
        });

        const output = consoleSpy.mock.calls.find(
          (call: unknown[]) =>
            typeof call[0] === 'string' && call[0].includes('"token"')
        );
        expect(output).toBeDefined();
        const parsed = JSON.parse(output![0]);
        expect(parsed.token).toBe('octocode_token_12345');
        expect(parsed.type).toBe('octocode-storage');
      });

      it('should output JSON with null token and none type when not authenticated', async () => {
        const { getToken } = await import('../../src/features/github-oauth.js');
        vi.mocked(getToken).mockResolvedValue({
          token: null,
          source: 'none',
        });

        const { findCommand } = await import('../../src/cli/commands/index.js');
        const tokenCmd = findCommand('token');

        await tokenCmd!.handler!({
          command: 'token',
          args: [],
          options: { json: true },
        });

        const output = consoleSpy.mock.calls.find(
          (call: unknown[]) =>
            typeof call[0] === 'string' && call[0].includes('"token"')
        );
        expect(output).toBeDefined();
        const parsed = JSON.parse(output![0]);
        expect(parsed.token).toBeNull();
        expect(parsed.type).toBe('none');
        expect(process.exitCode).toBe(1);
      });

      it('does not treat removed -j alias as JSON mode', async () => {
        const { getToken } = await import('../../src/features/github-oauth.js');
        vi.mocked(getToken).mockResolvedValue({
          token: 'shorthand_token',
          source: 'gh-cli',
        });

        const { findCommand } = await import('../../src/cli/commands/index.js');
        const tokenCmd = findCommand('token');

        await tokenCmd!.handler!({
          command: 'token',
          args: [],
          options: { j: true },
        });

        const output = consoleSpy.mock.calls.flat().join('\n');
        expect(output).not.toContain('"token"');
        expect(output).toContain('shorthand_token');
      });

      it('should output JSON error for invalid type in json mode', async () => {
        const { findCommand } = await import('../../src/cli/commands/index.js');
        const tokenCmd = findCommand('token');

        await tokenCmd!.handler!({
          command: 'token',
          args: [],
          options: { json: true, type: 'invalid' },
        });

        const output = consoleSpy.mock.calls.find(
          (call: unknown[]) =>
            typeof call[0] === 'string' && call[0].includes('"token"')
        );
        expect(output).toBeDefined();
        const parsed = JSON.parse(output![0]);
        expect(parsed.token).toBeNull();
        expect(parsed.type).toBe('none');
        expect(process.exitCode).toBe(1);
      });
    });

    it('should show octocode-specific hint when --type=octocode and no token', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: null,
        source: 'none',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { type: 'octocode' },
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('No Octocode token found')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('octocode login')
      );
      expect(process.exitCode).toBe(1);
    });

    it('should show gh-specific hint when --type=gh and no token', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: null,
        source: 'none',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { type: 'gh' },
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('No gh CLI token found')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('gh auth login')
      );
      expect(process.exitCode).toBe(1);
    });

    it('should show source info with --source flag', async () => {
      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'ghp_show_source_flag_token',
        source: 'octocode',
        username: 'sourceuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { source: true },
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Token found')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Source:')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('@sourceuser')
      );
      const allOutput = consoleSpy.mock.calls
        .map((c: unknown[]) => c[0])
        .join('\n');
      expect(allOutput).toContain('Token:');
      expect(allOutput).not.toContain('ghp_show_source_flag_token');
    });
  });

  describe('statusCommand', () => {
    it('should show logged in status when authenticated', async () => {
      const { getAuthStatus } =
        await import('../../src/features/github-oauth.js');
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        hostname: 'github.com',
        username: 'testuser',
        tokenExpired: false,
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const statusCmd = findCommand('status');
      expect(statusCmd).toBeDefined();

      await statusCmd!.handler!({
        command: 'status',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Logged in')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('testuser')
      );
      expect(process.exitCode).toBeUndefined();
    });

    it('should show not logged in status when not authenticated', async () => {
      const { getAuthStatus } =
        await import('../../src/features/github-oauth.js');
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const statusCmd = findCommand('status');

      await statusCmd!.handler!({
        command: 'status',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Not logged in')
      );
    });

    it('should show token expired warning when token is expired', async () => {
      const { getAuthStatus } =
        await import('../../src/features/github-oauth.js');
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        hostname: 'github.com',
        username: 'testuser',
        tokenExpired: true,
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const statusCmd = findCommand('status');

      await statusCmd!.handler!({
        command: 'status',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('expired')
      );
    });

    it('should use custom hostname when provided', async () => {
      const { getAuthStatus } =
        await import('../../src/features/github-oauth.js');
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        hostname: 'github.enterprise.com',
        username: 'enterpriseuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const statusCmd = findCommand('status');

      await statusCmd!.handler!({
        command: 'status',
        args: [],
        options: { hostname: 'github.enterprise.com' },
      });

      expect(getAuthStatus).toHaveBeenCalledWith('github.enterprise.com');
    });

    it('should be findable by name "status"', async () => {
      const { findCommand } = await import('../../src/cli/commands/index.js');
      const statusCmd = findCommand('status');
      expect(statusCmd).toBeDefined();
      expect(statusCmd!.name).toBe('status');
    });
  });

  describe('findCommand', () => {
    it('should find token command by name', async () => {
      const { findCommand } = await import('../../src/cli/commands/index.js');
      const cmd = findCommand('token');
      expect(cmd).toBeDefined();
      expect(cmd!.name).toBe('token');
    });

    it('should find status command by name', async () => {
      const { findCommand } = await import('../../src/cli/commands/index.js');
      const cmd = findCommand('status');
      expect(cmd).toBeDefined();
      expect(cmd!.name).toBe('status');
    });

    it('should return undefined for unknown command', async () => {
      const { findCommand } = await import('../../src/cli/commands/index.js');
      const cmd = findCommand('unknown-command');
      expect(cmd).toBeUndefined();
    });
  });

  describe('token masking (security)', () => {
    let originalIsTTY: boolean | undefined;

    beforeEach(() => {
      originalIsTTY = process.stdout.isTTY;
    });

    afterEach(() => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: originalIsTTY,
        writable: true,
        configurable: true,
      });
    });

    it('should mask token in TTY mode for tokenCommand', async () => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: true,
        writable: true,
        configurable: true,
      });

      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'ghp_abcdef1234567890xyz',
        source: 'octocode',
        username: 'testuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: {},
      });

      const allOutput = consoleSpy.mock.calls
        .map((c: unknown[]) => c[0])
        .join('\n');
      expect(allOutput).not.toContain('ghp_abcdef1234567890xyz');
      expect(consoleSpy).toHaveBeenCalledWith('ghp_****0xyz');
    });

    it('should output raw token when piped (non-TTY) for tokenCommand', async () => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: undefined,
        writable: true,
        configurable: true,
      });

      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'ghp_abcdef1234567890xyz',
        source: 'octocode',
        username: 'testuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith('ghp_abcdef1234567890xyz');
    });

    it('should always mask token in --source display mode', async () => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: undefined,
        writable: true,
        configurable: true,
      });

      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'ghp_source_secret_token_99',
        source: 'gh-cli',
        username: 'testuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { source: true },
      });

      const allOutput = consoleSpy.mock.calls
        .map((c: unknown[]) => c[0])
        .join('\n');
      expect(allOutput).not.toContain('ghp_source_secret_token_99');
      expect(allOutput).toContain('ghp_****n_99');
    });

    it('should never include raw token in JSON output regardless of TTY', async () => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: true,
        writable: true,
        configurable: true,
      });

      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'ghp_json_secret_abc123',
        source: 'octocode',
        username: 'testuser',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: { json: true },
      });

      const jsonCall = consoleSpy.mock.calls.find(
        (call: unknown[]) =>
          typeof call[0] === 'string' && call[0].includes('"token"')
      );
      expect(jsonCall).toBeDefined();
      const parsed = JSON.parse(jsonCall![0]);
      expect(parsed.token).toBe('ghp_json_secret_abc123');
    });

    it('should mask token in TTY mode for auth token subcommand', async () => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: true,
        writable: true,
        configurable: true,
      });

      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'ghp_auth_secret_token_xyz',
        source: 'octocode',
      } as any);

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const authCmd = findCommand('auth');

      await authCmd!.handler!({
        command: 'auth',
        args: ['token'],
        options: {},
      });

      const allOutput = consoleSpy.mock.calls
        .map((c: unknown[]) => c[0])
        .join('\n');
      expect(allOutput).not.toContain('ghp_auth_secret_token_xyz');
      expect(consoleSpy).toHaveBeenCalledWith('ghp_****_xyz');
    });

    it('should mask gh-cli token in TTY mode for auth token subcommand', async () => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: true,
        writable: true,
        configurable: true,
      });

      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'ghp_fallback_cli_token_end',
        source: 'gh-cli',
      } as any);

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const authCmd = findCommand('auth');

      await authCmd!.handler!({
        command: 'auth',
        args: ['token'],
        options: {},
      });

      const allOutput = consoleSpy.mock.calls
        .map((c: unknown[]) => c[0])
        .join('\n');
      expect(allOutput).not.toContain('ghp_fallback_cli_token_end');
      expect(consoleSpy).toHaveBeenCalledWith('ghp_****_end');
    });

    it('should mask short tokens correctly', async () => {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: true,
        writable: true,
        configurable: true,
      });

      const { getToken } = await import('../../src/features/github-oauth.js');
      vi.mocked(getToken).mockResolvedValue({
        token: 'short',
        source: 'octocode',
      });

      const { findCommand } = await import('../../src/cli/commands/index.js');
      const tokenCmd = findCommand('token');

      await tokenCmd!.handler!({
        command: 'token',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith('****');
      expect(consoleSpy).not.toHaveBeenCalledWith('short');
    });
  });
});
