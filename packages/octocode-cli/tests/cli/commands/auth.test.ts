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
  dim: (text: string) => text,
  bold: (text: string) => text,
}));

vi.mock('../../../src/ui/constants.js', () => ({
  CLIENT_INFO: {},
  IDE_INFO: {},
}));

vi.mock('../../../src/features/gh-auth.js', () => ({
  GH_CLI_URL: 'https://cli.github.com/',
}));

vi.mock('../../../src/features/github-oauth.js', () => ({
  login: vi.fn(),
  logout: vi.fn(),
  getAuthStatus: vi.fn(),
  getStoragePath: vi.fn().mockReturnValue('/mock/.octocode/credentials.json'),
  getOctocodeToken: vi.fn(),
  getGhCliToken: vi.fn(),
  getToken: vi.fn(),
  getTokenType: vi.fn(),
  refreshAuthToken: vi.fn(),
}));

vi.mock('../../../src/utils/prompts.js', () => ({
  loadInquirer: vi.fn().mockResolvedValue(undefined),
  select: vi.fn(),
  confirm: vi.fn(),
}));

vi.mock('../../../src/utils/spinner.js', () => ({
  Spinner: vi.fn(function SpinnerMock(this: unknown) {
    return {
      start: vi.fn().mockReturnThis(),
      stop: vi.fn(),
      succeed: vi.fn(),
      fail: vi.fn(),
    };
  }),
}));

describe('cli/commands/auth', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;
  let originalIsTTY: boolean | undefined;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
    originalIsTTY = process.stdout.isTTY;
    Object.defineProperty(process.stdout, 'isTTY', {
      value: true,
      configurable: true,
    });
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

  async function loadAuthModule() {
    const oauth = await import('../../../src/features/github-oauth.js');
    const prompts = await import('../../../src/utils/prompts.js');
    const auth = await import('../../../src/cli/commands/auth.js');
    return { ...oauth, ...prompts, ...auth };
  }

  // Collect every console.log argument that parses as JSON.
  function jsonLines(): Array<Record<string, unknown>> {
    const out: Array<Record<string, unknown>> = [];
    for (const line of consoleSpy.mock.calls.flat()) {
      if (typeof line !== 'string') continue;
      try {
        out.push(JSON.parse(line));
      } catch {
        /* not json */
      }
    }
    return out;
  }

  // Return the (single) JSON line logged; fails if none found.
  function findJsonLine(): Record<string, unknown> {
    const lines = jsonLines();
    expect(lines.length).toBeGreaterThan(0);
    return lines[0]!;
  }

  describe('loginCommand', () => {
    it('shows already-authenticated message and skips login', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'existing',
        hostname: 'github.com',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Already authenticated')
      );
      expect(login).not.toHaveBeenCalled();
    });

    it('completes successful login', async () => {
      const { login, loginCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      vi.mocked(login).mockResolvedValue({
        success: true,
        username: 'newuser',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: {},
      });

      expect(login).toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Authentication complete')
      );
      expect(process.exitCode).toBeUndefined();
    });

    it('uses short -p git protocol alias', async () => {
      const { login, loginCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      vi.mocked(login).mockResolvedValue({
        success: true,
        username: 'newuser',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { p: 'ssh' },
      });

      expect(login).toHaveBeenCalledWith(
        expect.objectContaining({ gitProtocol: 'ssh' })
      );
    });

    it('rejects invalid git protocol values', async () => {
      const { login, loginCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { p: 'ftp' },
      });

      expect(login).not.toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Invalid git protocol')
      );
      expect(process.exitCode).toBe(1);
    });

    it('shows verification UI when OAuth provides verification info', async () => {
      const { login, loginCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      vi.mocked(login).mockImplementation(async (options = {}) => {
        const onVerification = (options as Record<string, unknown>)
          .onVerification as ((v: Record<string, unknown>) => void) | undefined;
        onVerification?.({
          device_code: 'DC',
          user_code: 'ABCD-1234',
          verification_uri: 'https://github.com/login/device',
          expires_in: 900,
          interval: 5,
        });
        return { success: true, username: 'dev' };
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('ABCD-1234')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('github.com/login/device')
      );
    });

    it('sets exitCode on failed login', async () => {
      const { login, loginCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      vi.mocked(login).mockResolvedValue({
        success: false,
        error: 'access_denied',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Authentication failed')
      );
      expect(process.exitCode).toBe(1);
    });

    it('already-authenticated in json mode outputs json and skips login', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'existing',
        hostname: 'github.com',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { json: true },
      });

      const parsed = findJsonLine();
      expect(parsed.success).toBe(true);
      expect(parsed.username).toBe('existing');
      expect(parsed.alreadyAuthenticated).toBe(true);
      expect(login).not.toHaveBeenCalled();
    });

    it('--force logs out first then re-logs in', async () => {
      const { loginCommand, getAuthStatus, login, logout } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'old',
        hostname: 'github.com',
      });
      vi.mocked(logout).mockResolvedValue({ success: true });
      vi.mocked(login).mockResolvedValue({ success: true, username: 'old' });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { force: true },
      });

      expect(logout).toHaveBeenCalledWith('github.com');
      expect(login).toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Logging out')
      );
    });

    it('--force in json mode skips the logging-out message line', async () => {
      const { loginCommand, getAuthStatus, login, logout } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'old',
        hostname: 'github.com',
      });
      vi.mocked(logout).mockResolvedValue({ success: true });
      vi.mocked(login).mockResolvedValue({ success: true, username: 'old' });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { force: true, json: true },
      });

      expect(logout).toHaveBeenCalledWith('github.com');
      expect(
        consoleSpy.mock.calls.some((call: unknown[]) =>
          String(call[0]).includes('Logging out')
        )
      ).toBe(false);
    });

    it('blocks login in non-TTY environments', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      Object.defineProperty(process.stdout, 'isTTY', {
        value: false,
        configurable: true,
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: {},
      });

      expect(login).not.toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('requires browser interaction')
      );
      expect(process.exitCode).toBe(1);
    });

    it('blocks login in non-TTY with json output', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      Object.defineProperty(process.stdout, 'isTTY', {
        value: false,
        configurable: true,
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { json: true },
      });

      expect(login).not.toHaveBeenCalled();
      const parsed = findJsonLine();
      expect(parsed.success).toBe(false);
      expect(parsed.requiresInteraction).toBe(true);
      expect(process.exitCode).toBe(1);
    });

    it('json mode emits verification and result steps', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      vi.mocked(login).mockImplementation(async (options = {}) => {
        const onVerification = (options as Record<string, unknown>)
          .onVerification as ((v: Record<string, unknown>) => void) | undefined;
        onVerification?.({
          device_code: 'DC',
          user_code: 'WXYZ-9999',
          verification_uri: 'https://github.com/login/device',
          expires_in: 600,
          interval: 5,
        });
        return { success: true, username: 'jsondev' };
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { json: true },
      });

      const lines = jsonLines();
      const verification = lines.find(l => l.step === 'verification')!;
      const result = lines.find(l => l.step === 'result')!;
      expect(verification.userCode).toBe('WXYZ-9999');
      expect(verification.expiresIn).toBe(600);
      expect(result.success).toBe(true);
      expect(result.username).toBe('jsondev');
      expect(process.exitCode).toBeUndefined();
    });

    it('json mode sets exitCode on failed result', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      vi.mocked(login).mockResolvedValue({
        success: false,
        error: 'denied',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { json: true },
      });

      const result = jsonLines().find(l => l.step === 'result')!;
      expect(result.success).toBe(false);
      expect(result.error).toBe('denied');
      expect(process.exitCode).toBe(1);
    });

    it('invalid git protocol in json mode outputs json error', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { 'git-protocol': 'ftp', json: true },
      });

      expect(login).not.toHaveBeenCalled();
      const parsed = findJsonLine();
      expect(parsed.success).toBe(false);
      expect(parsed.error).toContain('Invalid git protocol');
      expect(process.exitCode).toBe(1);
    });
  });

  describe('logoutCommand', () => {
    it('warns when not authenticated', async () => {
      const { logoutCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });

      await logoutCommand.handler!({
        command: 'logout',
        args: [],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Not currently authenticated')
      );
      expect(process.exitCode).toBeUndefined();
    });

    it('logs out successfully', async () => {
      const { logout, logoutCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'alice',
        hostname: 'github.com',
      });
      vi.mocked(logout).mockResolvedValue({ success: true });

      await logoutCommand.handler!({
        command: 'logout',
        args: [],
        options: { yes: true }, // skip TTY confirm in tests
      });

      expect(logout).toHaveBeenCalledWith('github.com');
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Successfully logged out')
      );
    });

    it('sets exitCode on failed logout', async () => {
      const { logout, logoutCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'alice',
        hostname: 'github.com',
      });
      vi.mocked(logout).mockResolvedValue({
        success: false,
        error: 'revoke failed',
      });

      await logoutCommand.handler!({
        command: 'logout',
        args: [],
        options: { yes: true }, // skip TTY confirm in tests
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Logout failed')
      );
      expect(process.exitCode).toBe(1);
    });

    it('not authenticated in json mode outputs alreadyLoggedOut json', async () => {
      const { logoutCommand, getAuthStatus, logout } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });

      await logoutCommand.handler!({
        command: 'logout',
        args: [],
        options: { json: true },
      });

      const parsed = findJsonLine();
      expect(parsed.success).toBe(true);
      expect(parsed.alreadyLoggedOut).toBe(true);
      expect(logout).not.toHaveBeenCalled();
    });

    it('prompts for confirmation in TTY and cancels when declined', async () => {
      const { logoutCommand, getAuthStatus, logout, confirm } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'carol',
        hostname: 'github.com',
      });
      vi.mocked(confirm).mockResolvedValue(false);

      await logoutCommand.handler!({
        command: 'logout',
        args: [],
        options: {},
      });

      expect(confirm).toHaveBeenCalled();
      expect(logout).not.toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Logout cancelled')
      );
    });

    it('prompts for confirmation in TTY and proceeds when accepted', async () => {
      const { logoutCommand, getAuthStatus, logout, confirm } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'carol',
        hostname: 'github.com',
      });
      vi.mocked(confirm).mockResolvedValue(true);
      vi.mocked(logout).mockResolvedValue({ success: true });

      await logoutCommand.handler!({
        command: 'logout',
        args: [],
        options: {},
      });

      expect(confirm).toHaveBeenCalled();
      expect(logout).toHaveBeenCalledWith('github.com');
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Successfully logged out')
      );
    });

    it('successful logout in json mode outputs success json', async () => {
      const { logoutCommand, getAuthStatus, logout } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'dave',
        hostname: 'github.com',
      });
      vi.mocked(logout).mockResolvedValue({ success: true });

      await logoutCommand.handler!({
        command: 'logout',
        args: [],
        options: { json: true },
      });

      const parsed = findJsonLine();
      expect(parsed.success).toBe(true);
      expect(parsed.hostname).toBe('github.com');
      expect(process.exitCode).toBeUndefined();
    });

    it('failed logout in json mode sets exitCode', async () => {
      const { logoutCommand, getAuthStatus, logout } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'dave',
        hostname: 'github.com',
      });
      vi.mocked(logout).mockResolvedValue({
        success: false,
        error: 'boom',
      });

      await logoutCommand.handler!({
        command: 'logout',
        args: [],
        options: { json: true },
      });

      const parsed = findJsonLine();
      expect(parsed.success).toBe(false);
      expect(parsed.error).toBe('boom');
      expect(process.exitCode).toBe(1);
    });
  });

  describe('authCommand', () => {
    it('delegates to login for subcommand login', async () => {
      const { login, authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      vi.mocked(login).mockResolvedValue({ success: true, username: 'x' });

      await authCommand.handler!({
        command: 'auth',
        args: ['login'],
        options: {},
      });

      expect(login).toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Authentication complete')
      );
    });

    it('delegates to logout for subcommand logout', async () => {
      const { logout, authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'bob',
        hostname: 'github.com',
      });
      vi.mocked(logout).mockResolvedValue({ success: true });

      await authCommand.handler!({
        command: 'auth',
        args: ['logout'],
        options: { yes: true }, // skip TTY confirm in tests
      });

      expect(logout).toHaveBeenCalledWith('github.com');
    });

    it('shows status for subcommand status', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'statususer',
        hostname: 'github.com',
        tokenSource: 'octocode',
        tokenExpired: false,
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Authenticated as')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('statususer')
      );
    });

    it('auth status --json outputs structured JSON', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'jsonuser',
        hostname: 'github.com',
        tokenSource: 'octocode',
        tokenExpired: false,
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: { json: true },
      });

      const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
        if (typeof line !== 'string') return false;
        try {
          JSON.parse(line);
          return true;
        } catch {
          return false;
        }
      });
      expect(jsonLine).toBeDefined();
      const parsed = JSON.parse(jsonLine as string);
      expect(parsed.authenticated).toBe(true);
      expect(parsed.username).toBe('jsonuser');
    });

    it('auth without subcommand in non-TTY with --json outputs JSON', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        username: undefined,
        hostname: 'github.com',
        tokenSource: undefined,
      });

      // Simulate non-TTY
      const originalIsTTY = process.stdout.isTTY;
      Object.defineProperty(process.stdout, 'isTTY', {
        value: false,
        configurable: true,
      });

      await authCommand.handler!({
        command: 'auth',
        args: [],
        options: { json: true },
      });

      Object.defineProperty(process.stdout, 'isTTY', {
        value: originalIsTTY,
        configurable: true,
      });

      const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
        if (typeof line !== 'string') return false;
        try {
          JSON.parse(line);
          return true;
        } catch {
          return false;
        }
      });
      expect(jsonLine).toBeDefined();
      expect(process.exitCode).toBe(1);
    });

    it('passes hostname alias through auth status', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'enterprise',
        hostname: 'github.enterprise.test',
        tokenSource: 'octocode',
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: { H: 'github.enterprise.test' },
      });

      expect(getAuthStatus).toHaveBeenCalledWith('github.enterprise.test');
    });

    it('shows tokenExpired warning on status when applicable', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'expired',
        hostname: 'github.com',
        tokenExpired: true,
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Token has expired')
      );
    });

    it('subcommand token delegates to token command and prints octocode token', async () => {
      const { authCommand, getToken } = await loadAuthModule();
      vi.mocked(getToken).mockResolvedValue({
        token: 'gho_1234567890abcdefghijklmnopqrst',
        source: 'octocode',
      } as never);

      await authCommand.handler!({
        command: 'auth',
        args: ['token'],
        options: {},
      });

      expect(getToken).toHaveBeenCalledWith('github.com', 'auto');
      expect(consoleSpy).toHaveBeenCalledWith('gho_****qrst');
    });

    it('subcommand token uses short hostname alias', async () => {
      const { authCommand, getToken } = await loadAuthModule();
      vi.mocked(getToken).mockResolvedValue({
        token: 'gho_1234567890abcdefghijklmnopqrst',
        source: 'octocode',
      } as never);

      await authCommand.handler!({
        command: 'auth',
        args: ['token'],
        options: { H: 'github.enterprise.test' },
      });

      expect(getToken).toHaveBeenCalledWith('github.enterprise.test', 'auto');
    });

    it('subcommand token falls back to gh-cli token', async () => {
      const { authCommand, getToken } = await loadAuthModule();
      vi.mocked(getToken).mockResolvedValue({
        token: 'ghp_zzzzzzzzzzzzzzzzzzzzzzzzzzzzzz',
        source: 'gh-cli',
      } as never);

      await authCommand.handler!({
        command: 'auth',
        args: ['token'],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith('ghp_****zzzz');
    });

    it('subcommand token with no token shows help and sets exitCode', async () => {
      const { authCommand, getToken } = await loadAuthModule();
      vi.mocked(getToken).mockResolvedValue({
        token: null,
        source: 'none',
      } as never);

      await authCommand.handler!({
        command: 'auth',
        args: ['token'],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Not authenticated')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('octocode login')
      );
      expect(process.exitCode).toBe(1);
    });

    it('without subcommand when authenticated shows menu (back)', async () => {
      const { authCommand, getAuthStatus, select } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'menuuser',
        hostname: 'github.com',
      });
      vi.mocked(select).mockResolvedValue('back');

      await authCommand.handler!({
        command: 'auth',
        args: [],
        options: {},
      });

      expect(select).toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('GitHub Authentication')
      );
    });

    it('without subcommand when not authenticated shows menu (back)', async () => {
      const { authCommand, getAuthStatus, select } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });
      vi.mocked(select).mockResolvedValue('back');

      await authCommand.handler!({
        command: 'auth',
        args: [],
        options: {},
      });

      expect(select).toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Not authenticated')
      );
    });

    it('menu login keeps the selected hostname', async () => {
      const { authCommand, getAuthStatus, select, login } =
        await loadAuthModule();

      vi.mocked(getAuthStatus)
        .mockReturnValueOnce({
          authenticated: false,
          hostname: 'enterprise.github.com',
        })
        .mockReturnValueOnce({
          authenticated: false,
          hostname: 'enterprise.github.com',
        })
        .mockReturnValue({
          authenticated: false,
          hostname: 'enterprise.github.com',
        });

      vi.mocked(select).mockResolvedValue('login');
      vi.mocked(login).mockResolvedValue({
        success: true,
        username: 'frommenu',
      });

      await authCommand.handler!({
        command: 'auth',
        args: [],
        options: { hostname: 'enterprise.github.com' },
      });

      expect(login).toHaveBeenCalledWith(
        expect.objectContaining({ hostname: 'enterprise.github.com' })
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Authentication complete')
      );
    });

    it('menu logout calls oauth logout', async () => {
      const { authCommand, getAuthStatus, select, logout } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'lu',
        hostname: 'enterprise.github.com',
      });
      vi.mocked(select).mockResolvedValue('logout');
      vi.mocked(logout).mockResolvedValue({ success: true });

      await authCommand.handler!({
        command: 'auth',
        args: [],
        options: { hostname: 'enterprise.github.com' },
      });

      expect(logout).toHaveBeenCalledWith('enterprise.github.com');
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Successfully logged out')
      );
    });

    it('menu switch logs out globally then logs in again', async () => {
      const { authCommand, getAuthStatus, select, logout, login } =
        await loadAuthModule();

      vi.mocked(getAuthStatus)
        .mockReturnValueOnce({
          authenticated: true,
          username: 'switcher',
          hostname: 'github.com',
        })
        .mockReturnValueOnce({
          authenticated: true,
          username: 'switcher',
          hostname: 'github.com',
        })
        .mockReturnValue({
          authenticated: false,
          hostname: 'github.com',
        });

      vi.mocked(select).mockResolvedValue('switch');
      vi.mocked(logout).mockResolvedValue({ success: true });
      vi.mocked(login).mockResolvedValue({ success: true, username: 'new' });

      await authCommand.handler!({
        command: 'auth',
        args: [],
        options: {},
      });

      expect(logout).toHaveBeenCalledWith('github.com');
      expect(login).toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Starting new login')
      );
    });

    it('status shows login hints when unauthenticated', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Not authenticated')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('octocode login')
      );
    });

    it('status --json sets exitCode 1 when unauthenticated', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: { json: true },
      });

      const parsed = findJsonLine();
      expect(parsed.authenticated).toBe(false);
      expect(process.exitCode).toBe(1);
    });

    it('without subcommand in non-TTY (no json) prints status text', async () => {
      const { authCommand, getAuthStatus, select } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'ttyless',
        hostname: 'github.com',
        tokenSource: 'octocode',
      });
      Object.defineProperty(process.stdout, 'isTTY', {
        value: false,
        configurable: true,
      });

      await authCommand.handler!({
        command: 'auth',
        args: [],
        options: {},
      });

      expect(select).not.toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Authenticated as')
      );
    });

    describe('refresh subcommand', () => {
      it('rejects env-sourced tokens with hint', async () => {
        const { authCommand, getAuthStatus, refreshAuthToken } =
          await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: true,
          username: 'envuser',
          hostname: 'github.com',
          tokenSource: 'env',
          envTokenSource: 'GITHUB_TOKEN',
        } as never);

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: {},
        });

        expect(refreshAuthToken).not.toHaveBeenCalled();
        expect(consoleSpy).toHaveBeenCalledWith(
          expect.stringContaining('environment variable (GITHUB_TOKEN)')
        );
        expect(process.exitCode).toBe(1);
      });

      it('rejects env-sourced tokens in json mode (no env var name)', async () => {
        const { authCommand, getAuthStatus } = await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: true,
          hostname: 'github.com',
          tokenSource: 'env',
        } as never);

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: { json: true },
        });

        const parsed = findJsonLine();
        expect(parsed.success).toBe(false);
        expect(parsed.refreshable).toBe(false);
        expect(parsed.tokenSource).toBe('env');
        expect(process.exitCode).toBe(1);
      });

      it('rejects gh-cli tokens with gh auth refresh hint', async () => {
        const { authCommand, getAuthStatus, refreshAuthToken } =
          await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: true,
          hostname: 'github.com',
          tokenSource: 'gh-cli',
        } as never);

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: {},
        });

        expect(refreshAuthToken).not.toHaveBeenCalled();
        expect(consoleSpy).toHaveBeenCalledWith(
          expect.stringContaining('gh CLI')
        );
        expect(
          consoleSpy.mock.calls.some((call: unknown[]) =>
            String(call[0]).includes('gh auth refresh')
          )
        ).toBe(true);
        expect(process.exitCode).toBe(1);
      });

      it('rejects gh-cli tokens in json mode with enterprise hostname', async () => {
        const { authCommand, getAuthStatus } = await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: true,
          hostname: 'ghe.example.com',
          tokenSource: 'gh-cli',
        } as never);

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: { json: true, hostname: 'ghe.example.com' },
        });

        const parsed = findJsonLine();
        expect(parsed.success).toBe(false);
        expect(parsed.hint).toBe('gh auth refresh');
        expect(process.exitCode).toBe(1);
      });

      it('shows enterprise hostname hint in non-json gh-cli message', async () => {
        const { authCommand, getAuthStatus } = await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: true,
          hostname: 'ghe.example.com',
          tokenSource: 'gh-cli',
        } as never);

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: { hostname: 'ghe.example.com' },
        });

        expect(
          consoleSpy.mock.calls.some((call: unknown[]) =>
            String(call[0]).includes('--hostname ghe.example.com')
          )
        ).toBe(true);
      });

      it('errors when no token source / not authenticated', async () => {
        const { authCommand, getAuthStatus, refreshAuthToken } =
          await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: false,
          hostname: 'github.com',
          tokenSource: 'none',
        } as never);

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: {},
        });

        expect(refreshAuthToken).not.toHaveBeenCalled();
        expect(consoleSpy).toHaveBeenCalledWith(
          expect.stringContaining('Not authenticated')
        );
        expect(process.exitCode).toBe(1);
      });

      it('errors in json mode when not authenticated', async () => {
        const { authCommand, getAuthStatus } = await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: false,
          hostname: 'github.com',
          tokenSource: 'none',
        } as never);

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: { json: true },
        });

        const parsed = findJsonLine();
        expect(parsed.success).toBe(false);
        expect(parsed.refreshable).toBe(false);
        expect(process.exitCode).toBe(1);
      });

      it('refreshes an octocode token successfully', async () => {
        const { authCommand, getAuthStatus, refreshAuthToken } =
          await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: true,
          username: 'refreshme',
          hostname: 'github.com',
          tokenSource: 'octocode',
        } as never);
        vi.mocked(refreshAuthToken).mockResolvedValue({
          success: true,
          username: 'refreshme',
        });

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: {},
        });

        expect(refreshAuthToken).toHaveBeenCalledWith('github.com');
        expect(consoleSpy).toHaveBeenCalledWith(
          expect.stringContaining('Token refreshed')
        );
        expect(process.exitCode).toBeUndefined();
      });

      it('refreshes an octocode token successfully in json mode', async () => {
        const { authCommand, getAuthStatus, refreshAuthToken } =
          await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: true,
          username: 'refreshme',
          hostname: 'github.com',
          tokenSource: 'octocode',
        } as never);
        vi.mocked(refreshAuthToken).mockResolvedValue({
          success: true,
          username: 'refreshme',
        });

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: { json: true },
        });

        const parsed = findJsonLine();
        expect(parsed.success).toBe(true);
        expect(parsed.refreshable).toBe(true);
        expect(parsed.username).toBe('refreshme');
        expect(process.exitCode).toBeUndefined();
      });

      it('reports failed refresh with tip', async () => {
        const { authCommand, getAuthStatus, refreshAuthToken } =
          await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: true,
          username: 'refreshme',
          hostname: 'github.com',
          tokenSource: 'octocode',
        } as never);
        vi.mocked(refreshAuthToken).mockResolvedValue({
          success: false,
          error: 'expired refresh token',
        });

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: {},
        });

        expect(consoleSpy).toHaveBeenCalledWith(
          expect.stringContaining('Token refresh failed')
        );
        expect(
          consoleSpy.mock.calls.some((call: unknown[]) =>
            String(call[0]).includes('octocode login')
          )
        ).toBe(true);
        expect(process.exitCode).toBe(1);
      });

      it('reports failed refresh in json mode and sets exitCode', async () => {
        const { authCommand, getAuthStatus, refreshAuthToken } =
          await loadAuthModule();
        vi.mocked(getAuthStatus).mockReturnValue({
          authenticated: true,
          username: 'refreshme',
          hostname: 'github.com',
          tokenSource: 'octocode',
        } as never);
        vi.mocked(refreshAuthToken).mockResolvedValue({
          success: false,
          error: 'expired refresh token',
        });

        await authCommand.handler!({
          command: 'auth',
          args: ['refresh'],
          options: { json: true },
        });

        const parsed = findJsonLine();
        expect(parsed.success).toBe(false);
        expect(parsed.error).toBe('expired refresh token');
        expect(process.exitCode).toBe(1);
      });
    });
  });
});
