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
}));

vi.mock('../../../src/utils/prompts.js', () => ({
  loadInquirer: vi.fn().mockResolvedValue(undefined),
  select: vi.fn(),
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
        options: {},
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
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Logout failed')
      );
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
        options: {},
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
        expect.stringContaining('octocode-cli login')
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
        expect.stringContaining('octocode-cli login')
      );
    });
  });
});
