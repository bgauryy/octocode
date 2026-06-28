import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { EXIT } from '../../../src/cli/exit-codes.js';

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
  checkGitHubAuth: vi.fn().mockReturnValue({
    installed: true,
    authenticated: false,
  }),
  runGitHubAuthLogin: vi.fn().mockReturnValue({
    success: true,
    exitCode: 0,
  }),
  runGitHubAuthLogout: vi.fn().mockReturnValue({
    success: true,
    exitCode: 0,
  }),
}));

vi.mock('../../../src/features/github-oauth.js', () => {
  const getAuthStatus = vi
    .fn()
    .mockReturnValue({ authenticated: false, tokenSource: 'none' });
  return {
    login: vi.fn(),
    logout: vi.fn(),
    getAuthStatus,
    getAuthStatusAsync: vi
      .fn()
      .mockImplementation(async (...args: unknown[]) =>
        getAuthStatus(...(args as Parameters<typeof getAuthStatus>))
      ),
    getStoragePath: vi.fn().mockReturnValue('/mock/.octocode/credentials.json'),
    getOctocodeToken: vi.fn(),
    getGhCliToken: vi.fn(),
    getToken: vi.fn(),
    getTokenType: vi.fn(),
    refreshAuthToken: vi.fn(),
  };
});

vi.mock('../../../src/utils/token-storage.js', () => ({
  getCredentials: vi.fn().mockResolvedValue(null),
  hasEnvToken: vi.fn().mockReturnValue(false),
  getEnvTokenSource: vi.fn().mockReturnValue(null),
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
    const gh = await import('../../../src/features/gh-auth.js');
    const storage = await import('../../../src/utils/token-storage.js');
    const prompts = await import('../../../src/utils/prompts.js');
    const auth = await import('../../../src/cli/commands/auth.js');
    return { ...oauth, ...gh, ...storage, ...prompts, ...auth };
  }

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

  function findJsonLine(): Record<string, unknown> {
    const lines = jsonLines();
    expect(lines.length).toBeGreaterThan(0);
    return lines[0]!;
  }

  describe('loginCommand', () => {
    it('shows provider picker when already authenticated interactively', async () => {
      const { loginCommand, getAuthStatus, login, select, getCredentials } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'existing',
        hostname: 'github.com',
        tokenSource: 'octocode',
      });
      vi.mocked(getCredentials).mockResolvedValue({
        hostname: 'github.com',
        username: 'existing',
        token: { token: 'x', tokenType: 'oauth' },
        gitProtocol: 'https',
        createdAt: '2026-01-01T00:00:00.000Z',
        updatedAt: '2026-01-01T00:00:00.000Z',
      });
      vi.mocked(select).mockResolvedValue('back');

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: {},
      });

      expect(select).toHaveBeenCalled();
      expect(login).not.toHaveBeenCalled();
    });

    it('completes successful login', async () => {
      const { login, loginCommand, getAuthStatus, select } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });
      vi.mocked(select).mockResolvedValue('login');
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

    it('uses --git-protocol', async () => {
      const { login, loginCommand, getAuthStatus, select } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });
      vi.mocked(select).mockResolvedValue('login');
      vi.mocked(login).mockResolvedValue({
        success: true,
        username: 'newuser',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { 'git-protocol': 'ssh' },
      });

      expect(login).toHaveBeenCalledWith(
        expect.objectContaining({ gitProtocol: 'ssh' })
      );
    });

    it('rejects invalid git protocol values', async () => {
      const { login, loginCommand, getAuthStatus, select } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { 'git-protocol': 'ftp' },
      });

      expect(login).not.toHaveBeenCalled();
      expect(select).not.toHaveBeenCalled();
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Invalid git protocol')
      );
      expect(process.exitCode).toBe(EXIT.USAGE);
    });

    it('shows verification UI when OAuth provides verification info', async () => {
      const { login, loginCommand, getAuthStatus, select } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });
      vi.mocked(select).mockResolvedValue('login');
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
      const { login, loginCommand, getAuthStatus, select } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });
      vi.mocked(select).mockResolvedValue('login');
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
      expect(process.exitCode).toBe(EXIT.AUTH);
    });

    it('already-authenticated in json mode outputs json and skips login', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'existing',
        hostname: 'github.com',
        tokenSource: 'octocode',
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

    it('--force signs out first then signs in again', async () => {
      const { loginCommand, getAuthStatus, login, logout } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'old',
        hostname: 'github.com',
        tokenSource: 'octocode',
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
        expect.stringContaining('Signing out')
      );
    });

    it('--force in json mode skips the sign-out message line', async () => {
      const { loginCommand, getAuthStatus, login, logout } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'old',
        hostname: 'github.com',
        tokenSource: 'octocode',
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
        tokenSource: 'none',
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
      expect(process.exitCode).toBe(EXIT.USAGE);
    });

    it('blocks login in non-TTY with json output', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
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
      expect(process.exitCode).toBe(EXIT.USAGE);
    });

    it('json mode emits verification and result steps', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
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
        tokenSource: 'none',
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
      expect(process.exitCode).toBe(EXIT.AUTH);
    });

    it('invalid git protocol in json mode outputs json error', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
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
      expect(process.exitCode).toBe(EXIT.USAGE);
    });

    // Issue 3: dead branch removed — authenticated+tokenSource=undefined no longer treated as octocode
    it('does not treat authenticated-but-missing-tokenSource as already-octocode', async () => {
      const { loginCommand, getAuthStatus, login } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        hostname: 'github.com',
        tokenSource: undefined as never,
      });
      vi.mocked(login).mockResolvedValue({ success: true, username: 'x' });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { json: true },
      });

      // Should NOT show "alreadyAuthenticated: true" — should proceed with OAuth
      const lines = jsonLines();
      const alreadyMsg = lines.find(l => l.alreadyAuthenticated === true);
      expect(alreadyMsg).toBeUndefined();
    });

    // Issue 4: env token priority warning
    it('warns in plain mode when an env token is active before starting OAuth', async () => {
      const {
        loginCommand,
        getAuthStatus,
        login,
        hasEnvToken,
        getEnvTokenSource,
      } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });
      vi.mocked(hasEnvToken).mockReturnValue(true);
      vi.mocked(getEnvTokenSource).mockReturnValue('env:OCTOCODE_TOKEN');
      vi.mocked(login).mockResolvedValue({ success: true, username: 'x' });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { json: false },
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('OCTOCODE_TOKEN')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('takes priority')
      );
    });

    it('emits json warning step when env token is active', async () => {
      const {
        loginCommand,
        getAuthStatus,
        login,
        hasEnvToken,
        getEnvTokenSource,
      } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });
      vi.mocked(hasEnvToken).mockReturnValue(true);
      vi.mocked(getEnvTokenSource).mockReturnValue(
        'env:GITHUB_PERSONAL_ACCESS_TOKEN'
      );
      vi.mocked(login).mockResolvedValue({ success: true, username: 'x' });

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: { json: true },
      });

      const lines = jsonLines();
      const warning = lines.find(l => l.step === 'warning');
      expect(warning).toBeDefined();
      expect(String(warning!.envVar)).toContain('GITHUB_PERSONAL_ACCESS_TOKEN');
    });

    // Issue 2: auth menu env var hint
    it('includes env var hint in choices when no auth found', async () => {
      const { loginCommand, getAuthStatus, select, hasEnvToken } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });
      vi.mocked(hasEnvToken).mockReturnValue(false);
      vi.mocked(select).mockResolvedValue('back');

      await loginCommand.handler!({
        command: 'login',
        args: [],
        options: {},
      });

      const callArgs = vi.mocked(select).mock.calls[0]?.[0] as {
        choices: Array<{ type?: string; separator?: string }>;
      };
      const separators = callArgs.choices.filter(c => c.type === 'separator');
      const envHint = separators.some(
        s => s.separator && /OCTOCODE_TOKEN|GITHUB_TOKEN/i.test(s.separator)
      );
      expect(envHint).toBe(true);
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

    it('signs out successfully', async () => {
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
        options: { yes: true },
      });

      expect(logout).toHaveBeenCalledWith('github.com');
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Successfully signed out')
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
        options: { yes: true },
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
        expect.stringContaining('Successfully signed out')
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
    it('opens the provider picker for subcommand login', async () => {
      const { login, authCommand, getAuthStatus, select } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });
      vi.mocked(select).mockResolvedValue('login');
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

    it('allows Octocode login even when gh CLI is the active source', async () => {
      const { loginCommand, getAuthStatus, select, login, checkGitHubAuth } =
        await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'gh-user',
        hostname: 'github.com',
        tokenSource: 'gh-cli',
      });
      vi.mocked(checkGitHubAuth).mockReturnValue({
        installed: true,
        authenticated: true,
        username: 'gh-user',
      });
      vi.mocked(select).mockResolvedValue('login');
      vi.mocked(login).mockResolvedValue({
        success: true,
        username: 'octo-user',
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
    });

    it('runs gh auth login when gh CLI is selected', async () => {
      const {
        authCommand,
        getAuthStatus,
        select,
        runGitHubAuthLogin,
        checkGitHubAuth,
      } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.enterprise.test',
        tokenSource: 'none',
      });
      vi.mocked(checkGitHubAuth).mockReturnValue({
        installed: true,
        authenticated: false,
      });
      vi.mocked(select).mockResolvedValue('gh-login');

      await authCommand.handler!({
        command: 'auth',
        args: ['login'],
        options: {
          hostname: 'github.enterprise.test',
          'git-protocol': 'ssh',
        },
      });

      expect(runGitHubAuthLogin).toHaveBeenCalledWith({
        web: true,
        hostname: 'github.enterprise.test',
        gitProtocol: 'ssh',
      });
      expect(process.exitCode).toBeUndefined();
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
        options: { yes: true },
      });

      expect(logout).toHaveBeenCalledWith('github.com');
    });

    it('prints auth status subcommand', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('GitHub Authentication')
      );
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('anonymous requests may work')
      );
      expect(process.exitCode).toBeUndefined();
    });

    it('prints auth status subcommand in json mode without failing unauthenticated checks', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        tokenSource: 'none',
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: { json: true },
      });

      const parsed = findJsonLine();
      expect(parsed.success).toBe(true);
      expect(parsed.authenticated).toBe(false);
      expect(parsed.hostname).toBe('github.com');
      expect(parsed.tokenPresent).toBe(false);
      expect(parsed.publicGitHubAccess).toBe('anonymous');
      expect(process.exitCode).toBeUndefined();
    });

    it('auth without subcommand in non-TTY with --json outputs JSON', async () => {
      const { authCommand } = await loadAuthModule();

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
      const parsed = JSON.parse(jsonLine as string);
      expect(parsed.success).toBe(false);
      expect(String(parsed.error)).toContain('Provide an auth action');
      expect(process.exitCode).toBe(EXIT.USAGE);
    });

    it('supports --status flag for auth status checks', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        hostname: 'github.enterprise.test',
        username: 'agent',
        tokenSource: 'octocode',
      });

      await authCommand.handler!({
        command: 'auth',
        args: [],
        options: {
          status: true,
          json: true,
          hostname: 'github.enterprise.test',
        },
      });

      const parsed = findJsonLine();
      expect(parsed.success).toBe(true);
      expect(parsed.authenticated).toBe(true);
      expect(parsed.username).toBe('agent');
      expect(parsed.hostname).toBe('github.enterprise.test');
      expect(parsed.publicGitHubAccess).toBe('authenticated');
      expect(getAuthStatus).toHaveBeenCalledWith('github.enterprise.test');
    });

    it('rejects removed auth token subcommand', async () => {
      const { authCommand, getToken } = await loadAuthModule();

      await authCommand.handler!({
        command: 'auth',
        args: ['token'],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('auth token was removed')
      );
      expect(getToken).not.toHaveBeenCalled();
      expect(process.exitCode).toBe(EXIT.USAGE);
    });

    it('rejects removed auth token subcommand in json mode', async () => {
      const { authCommand, getToken } = await loadAuthModule();

      await authCommand.handler!({
        command: 'auth',
        args: ['token'],
        options: { json: true },
      });

      const parsed = findJsonLine();
      expect(parsed.success).toBe(false);
      expect(String(parsed.error)).toContain('auth token was removed');
      expect(getToken).not.toHaveBeenCalled();
      expect(process.exitCode).toBe(EXIT.USAGE);
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
        expect.stringContaining('Successfully signed out')
      );
    });

    it('menu switch signs out globally then signs in again', async () => {
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

    it('status subcommand points users to auth-only status output', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: false,
        hostname: 'github.com',
        tokenSource: 'none',
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: {},
      });

      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('GitHub Authentication')
      );
      expect(process.exitCode).toBeUndefined();
    });

    it('status subcommand in json mode returns auth-only status output', async () => {
      const { authCommand, getAuthStatus } = await loadAuthModule();
      vi.mocked(getAuthStatus).mockReturnValue({
        authenticated: true,
        username: 'status-user',
        hostname: 'github.com',
        tokenSource: 'gh-cli',
      });

      await authCommand.handler!({
        command: 'auth',
        args: ['status'],
        options: { json: true },
      });

      const parsed = findJsonLine();
      expect(parsed.success).toBe(true);
      expect(parsed.authenticated).toBe(true);
      expect(parsed.username).toBe('status-user');
      expect(parsed.tokenSource).toBe('gh-cli');
      expect(parsed.publicGitHubAccess).toBe('authenticated');
      expect(process.exitCode).toBeUndefined();
    });

    it('without subcommand in non-TTY (no json) prints usage error', async () => {
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
        expect.stringContaining('Provide an auth action')
      );
      expect(process.exitCode).toBe(EXIT.USAGE);
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
        expect(process.exitCode).toBe(EXIT.USAGE);
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
        expect(process.exitCode).toBe(EXIT.USAGE);
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
        expect(process.exitCode).toBe(EXIT.USAGE);
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
        expect(process.exitCode).toBe(EXIT.USAGE);
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
        expect(process.exitCode).toBe(EXIT.AUTH);
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
        expect(process.exitCode).toBe(EXIT.AUTH);
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
            String(call[0]).includes('login')
          )
        ).toBe(true);
        expect(process.exitCode).toBe(EXIT.AUTH);
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
        expect(process.exitCode).toBe(EXIT.AUTH);
      });
    });
  });
});
