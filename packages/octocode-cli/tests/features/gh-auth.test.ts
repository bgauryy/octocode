import { describe, it, expect, vi, beforeEach } from 'vitest';
import { spawnSync } from 'node:child_process';

vi.mock('node:child_process', () => ({
  spawnSync: vi.fn(),
  spawn: vi.fn(),
}));

describe('GitHub Auth', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('isGitHubCLIInstalled', () => {
    it('should return true when gh is available', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '/usr/local/bin/gh',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { isGitHubCLIInstalled } =
        await import('../../src/features/gh-auth.js');
      expect(isGitHubCLIInstalled()).toBe(true);
    });

    it('should return false when gh is not available', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 1,
        stdout: '',
        stderr: 'not found',
        pid: 123,
        output: [],
        signal: null,
      });

      const { isGitHubCLIInstalled } =
        await import('../../src/features/gh-auth.js');
      expect(isGitHubCLIInstalled()).toBe(false);
    });
  });

  describe('checkGitHubAuth', () => {
    it('should return not installed when gh is missing', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 1,
        stdout: '',
        stderr: 'not found',
        pid: 123,
        output: [],
        signal: null,
      });

      const { checkGitHubAuth } = await import('../../src/features/gh-auth.js');
      const result = checkGitHubAuth();

      expect(result.installed).toBe(false);
      expect(result.authenticated).toBe(false);
    });

    it('should return authenticated with username', async () => {
      vi.mocked(spawnSync)
        .mockReturnValueOnce({
          status: 0,
          stdout: '/usr/local/bin/gh',
          stderr: '',
          pid: 123,
          output: [],
          signal: null,
        })
        .mockReturnValueOnce({
          status: 0,
          stdout: 'Logged in to github.com account testuser (keyring)',
          stderr: '',
          pid: 124,
          output: [],
          signal: null,
        });

      const { checkGitHubAuth } = await import('../../src/features/gh-auth.js');
      const result = checkGitHubAuth();

      expect(result.installed).toBe(true);
      expect(result.authenticated).toBe(true);
      expect(result.username).toBe('testuser');
    });

    it('should return not authenticated when auth fails', async () => {
      vi.mocked(spawnSync)
        .mockReturnValueOnce({
          status: 0,
          stdout: '/usr/local/bin/gh',
          stderr: '',
          pid: 123,
          output: [],
          signal: null,
        })
        .mockReturnValueOnce({
          status: 1,
          stdout: '',
          stderr: 'You are not logged in',
          pid: 124,
          output: [],
          signal: null,
        });

      const { checkGitHubAuth } = await import('../../src/features/gh-auth.js');
      const result = checkGitHubAuth();

      expect(result.installed).toBe(true);
      expect(result.authenticated).toBe(false);
    });
  });

  describe('getGitHubCLIVersion', () => {
    it('should return version string', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: 'gh version 2.40.0 (2024-01-15)',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { getGitHubCLIVersion } =
        await import('../../src/features/gh-auth.js');
      expect(getGitHubCLIVersion()).toBe('2.40.0');
    });

    it('should return null when gh is not installed', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 1,
        stdout: '',
        stderr: 'not found',
        pid: 123,
        output: [],
        signal: null,
      });

      const { getGitHubCLIVersion } =
        await import('../../src/features/gh-auth.js');
      expect(getGitHubCLIVersion()).toBeNull();
    });
  });

  describe('constants', () => {
    it('should export GH_CLI_URL', async () => {
      const { GH_CLI_URL } = await import('../../src/features/gh-auth.js');
      expect(GH_CLI_URL).toBe('https://cli.github.com/');
    });

    it('should export getAuthLoginCommand', async () => {
      const { getAuthLoginCommand } =
        await import('../../src/features/gh-auth.js');
      expect(getAuthLoginCommand()).toBe('gh auth login');
    });
  });

  describe('runGitHubAuthLogin', () => {
    it('should call gh auth login with no options', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogin } =
        await import('../../src/features/gh-auth.js');
      const result = runGitHubAuthLogin();

      expect(result.success).toBe(true);
      expect(spawnSync).toHaveBeenCalledWith(
        'gh',
        ['auth', 'login'],
        expect.any(Object)
      );
    });

    it('should add --web flag when web option is true', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogin } =
        await import('../../src/features/gh-auth.js');
      runGitHubAuthLogin({ web: true });

      expect(spawnSync).toHaveBeenCalledWith(
        'gh',
        ['auth', 'login', '--web'],
        expect.any(Object)
      );
    });

    it('should add --hostname flag when hostname is provided', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogin } =
        await import('../../src/features/gh-auth.js');
      runGitHubAuthLogin({ hostname: 'github.enterprise.com' });

      expect(spawnSync).toHaveBeenCalledWith(
        'gh',
        ['auth', 'login', '--hostname', 'github.enterprise.com'],
        expect.any(Object)
      );
    });

    it('should add --git-protocol flag when gitProtocol is provided', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogin } =
        await import('../../src/features/gh-auth.js');
      runGitHubAuthLogin({ gitProtocol: 'ssh' });

      expect(spawnSync).toHaveBeenCalledWith(
        'gh',
        ['auth', 'login', '--git-protocol', 'ssh'],
        expect.any(Object)
      );
    });

    it('should add --skip-ssh-key flag when skipSshKey is true', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogin } =
        await import('../../src/features/gh-auth.js');
      runGitHubAuthLogin({ skipSshKey: true });

      expect(spawnSync).toHaveBeenCalledWith(
        'gh',
        ['auth', 'login', '--skip-ssh-key'],
        expect.any(Object)
      );
    });

    it('should combine multiple options', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogin } =
        await import('../../src/features/gh-auth.js');
      runGitHubAuthLogin({
        web: true,
        hostname: 'enterprise.github.com',
        gitProtocol: 'https',
      });

      expect(spawnSync).toHaveBeenCalledWith(
        'gh',
        [
          'auth',
          'login',
          '--web',
          '--hostname',
          'enterprise.github.com',
          '--git-protocol',
          'https',
        ],
        expect.any(Object)
      );
    });

    it('should return failure when login fails', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 1,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogin } =
        await import('../../src/features/gh-auth.js');
      const result = runGitHubAuthLogin();

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(1);
    });
  });

  describe('runGitHubAuthLogout', () => {
    it('should call gh auth logout with no hostname', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogout } =
        await import('../../src/features/gh-auth.js');
      const result = runGitHubAuthLogout();

      expect(result.success).toBe(true);
      expect(spawnSync).toHaveBeenCalledWith(
        'gh',
        ['auth', 'logout'],
        expect.any(Object)
      );
    });

    it('should add --hostname flag when hostname is provided', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogout } =
        await import('../../src/features/gh-auth.js');
      runGitHubAuthLogout('github.enterprise.com');

      expect(spawnSync).toHaveBeenCalledWith(
        'gh',
        ['auth', 'logout', '--hostname', 'github.enterprise.com'],
        expect.any(Object)
      );
    });

    it('should return failure when logout fails', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 1,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runGitHubAuthLogout } =
        await import('../../src/features/gh-auth.js');
      const result = runGitHubAuthLogout();

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(1);
    });
  });
});
