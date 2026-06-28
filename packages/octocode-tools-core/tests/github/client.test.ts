import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../src/serverConfig.js', () => ({
  getGitHubToken: vi.fn(),
  getServerConfig: vi.fn().mockReturnValue({
    githubApiUrl: 'https://api.github.com',
    timeout: 30000,
  }),
}));

vi.mock('../../src/session.js', () => ({
  recordRateLimit: vi.fn(),
}));

vi.mock('octokit', () => {
  function MockOctokit(this: Record<string, unknown>, opts: unknown) {
    this._opts = opts;
  }
  MockOctokit.plugin = vi.fn().mockReturnValue(MockOctokit);
  return { Octokit: MockOctokit };
});

vi.mock('@octokit/plugin-throttling', () => ({
  throttling: {},
}));

describe('getOctokit', () => {
  let getGitHubToken: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    vi.resetModules();
    const serverConfig = await import('../../src/serverConfig.js');
    getGitHubToken = serverConfig.getGitHubToken as ReturnType<typeof vi.fn>;
    getGitHubToken.mockReset();
  });

  async function loadClient() {
    const mod = await import('../../src/github/client.js');
    return mod;
  }

  it('calls getGitHubToken on every call when cache is cold', async () => {
    getGitHubToken.mockResolvedValue('token-abc');
    const { getOctokit } = await loadClient();

    await getOctokit();
    await getOctokit();

    expect(getGitHubToken).toHaveBeenCalledTimes(2);
  });

  it('returns a new Octokit instance when the resolved token changes', async () => {
    getGitHubToken.mockResolvedValueOnce('token-A').mockResolvedValueOnce('token-B');
    const { getOctokit } = await loadClient();

    const first = await getOctokit();
    const second = await getOctokit();

    expect(first).not.toBe(second);
  });

  it('returns the cached Octokit instance for the same token within TTL', async () => {
    getGitHubToken.mockResolvedValue('token-stable');
    const { getOctokit } = await loadClient();

    const first = await getOctokit();
    const second = await getOctokit();

    expect(first).toBe(second);
  });

  it('uses authInfo.token over resolved token when provided', async () => {
    getGitHubToken.mockResolvedValue('env-token');
    const { getOctokit } = await loadClient();

    const withAuth = await getOctokit({ token: 'explicit-token' });
    const withoutAuth = await getOctokit();

    expect(withAuth).not.toBe(withoutAuth);
  });
});
