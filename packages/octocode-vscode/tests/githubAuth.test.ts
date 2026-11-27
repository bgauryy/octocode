import { beforeEach, describe, expect, it, vi } from 'vitest';

const host = vi.hoisted(() => ({
  getSession: vi.fn(),
  manualToken: undefined as string | undefined,
}));

vi.mock('vscode', () => ({
  authentication: { getSession: host.getSession },
  workspace: {
    getConfiguration: (section: string) => ({
      get: (key: string) =>
        section === 'octocode' && key === 'githubToken'
          ? host.manualToken
          : undefined,
    }),
  },
}));

const { checkGitHubAuthStatus, getGitHubSession, getGitHubToken } =
  await import('../src/githubAuth');

let lines: string[];
const log = (line: string) => lines.push(line);
const session = { accessToken: 'oauth-token', account: { label: 'octo' } };

beforeEach(() => {
  lines = [];
  host.manualToken = undefined;
  host.getSession.mockReset().mockResolvedValue(undefined);
});

describe('getGitHubSession', () => {
  it('asks the GitHub provider for the repo and read:user scopes', async () => {
    host.getSession.mockResolvedValue(session);
    expect(await getGitHubSession({ createIfNone: true })).toBe(session);
    expect(host.getSession).toHaveBeenCalledWith('github', ['repo', 'read:user'], {
      createIfNone: true,
    });
  });
});

describe('getGitHubToken', () => {
  it('prefers the OAuth session token', async () => {
    host.getSession.mockResolvedValue(session);
    host.manualToken = 'manual';
    expect(await getGitHubToken(log)).toBe('oauth-token');
  });

  it('falls back to the manual token without a session', async () => {
    host.manualToken = 'manual';
    expect(await getGitHubToken(log)).toBe('manual');
    expect(lines).toEqual([]);
  });

  it('logs why the session check failed, then falls back', async () => {
    host.getSession.mockRejectedValue(new Error('keychain locked'));
    expect(await getGitHubToken(log)).toBeUndefined();
    expect(lines).toEqual(['Error checking GitHub session: Error: keychain locked']);
  });
});

describe('checkGitHubAuthStatus', () => {
  it('reports the OAuth account', async () => {
    host.getSession.mockResolvedValue(session);
    expect(await checkGitHubAuthStatus(log)).toEqual({
      authenticated: true,
      accountName: 'octo',
      tokenSource: 'oauth',
    });
  });

  it('reports a manual token after a failed session check', async () => {
    host.getSession.mockRejectedValue(new Error('keychain locked'));
    host.manualToken = 'manual';
    expect(await checkGitHubAuthStatus(log)).toEqual({
      authenticated: true,
      tokenSource: 'manual',
    });
    expect(lines).toEqual(['Error checking GitHub session: Error: keychain locked']);
  });

  it('reports no credentials', async () => {
    expect(await checkGitHubAuthStatus(log)).toEqual({
      authenticated: false,
      tokenSource: 'none',
    });
  });
});
