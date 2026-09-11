import { beforeEach, describe, expect, it, vi } from 'vitest';

const githubSearch = vi.hoisted(() => ({
  searchCode: vi.fn(),
  searchRepos: vi.fn(),
}));
const githubContent = vi.hoisted(() => ({ getFileContent: vi.fn() }));
const githubPullRequests = vi.hoisted(() => ({ searchPullRequests: vi.fn() }));
const githubStructure = vi.hoisted(() => ({ getRepoStructure: vi.fn() }));
const githubErrors = vi.hoisted(() => ({ handleGitHubAPIError: vi.fn() }));
const githubClient = vi.hoisted(() => ({ resolveDefaultBranch: vi.fn() }));
const githubIssues = vi.hoisted(() => ({ fetchIssues: vi.fn() }));
const githubCommitSearch = vi.hoisted(() => ({ searchCommits: vi.fn() }));
const githubHistory = vi.hoisted(() => ({ fetchHistory: vi.fn() }));
const githubCompare = vi.hoisted(() => ({ compareRefs: vi.fn() }));
const githubCommit = vi.hoisted(() => ({ fetchCommit: vi.fn() }));

vi.mock('../../src/providers/github/githubSearch.js', () => githubSearch);
vi.mock('../../src/providers/github/githubContent.js', () => githubContent);
vi.mock(
  '../../src/providers/github/githubPullRequests.js',
  () => githubPullRequests
);
vi.mock('../../src/providers/github/githubStructure.js', () => githubStructure);
vi.mock('../../src/github/errors.js', () => githubErrors);
vi.mock('../../src/github/client.js', () => githubClient);
vi.mock('../../src/github/issues/orchestrator.js', () => githubIssues);
vi.mock('../../src/github/commitSearch.js', () => githubCommitSearch);
vi.mock('../../src/github/history.js', () => githubHistory);
vi.mock('../../src/github/compare.js', () => githubCompare);
vi.mock('../../src/github/commit.js', () => githubCommit);

import { GitHubProvider } from '../../src/providers/github/GitHubProvider.js';
import { requireGitHubHistoryProvider } from '../../src/providers/github/historyProvider.js';

const success = { status: 200, provider: 'github' as const, data: {} };

describe('GitHubProvider', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    githubSearch.searchCode.mockResolvedValue(success);
    githubSearch.searchRepos.mockResolvedValue(success);
    githubContent.getFileContent.mockResolvedValue(success);
    githubPullRequests.searchPullRequests.mockResolvedValue(success);
    githubStructure.getRepoStructure.mockResolvedValue(success);
    githubClient.resolveDefaultBranch.mockResolvedValue('main');
    githubIssues.fetchIssues.mockResolvedValue(success);
    githubCommitSearch.searchCommits.mockResolvedValue(success);
    githubHistory.fetchHistory.mockResolvedValue(success);
    githubCompare.compareRefs.mockResolvedValue(success);
    githubCommit.fetchCommit.mockResolvedValue(success);
    githubErrors.handleGitHubAPIError.mockReturnValue({
      error: 'normalized failure',
      status: 503,
    });
  });

  it('keeps GitHub-only history operations behind the provider gateway', async () => {
    const provider = new GitHubProvider({ type: 'github', token: 'secret' });
    const authInfo = { token: 'secret' };
    const issues = { owner: 'o', repo: 'r' };
    const search = {
      owner: 'o',
      repo: 'r',
      keywords: ['fix'],
      page: 1,
      perPage: 10,
    };
    const history = {
      type: 'repo' as const,
      owner: 'o',
      repo: 'r',
      page: 1,
      perPage: 10,
      includeDiff: false,
    };
    const compare = { owner: 'o', repo: 'r', base: 'main', head: 'next' };
    const commit = { owner: 'o', repo: 'r', ref: 'abc123' };
    const gateway = requireGitHubHistoryProvider(provider, [
      'fetchIssues',
      'searchCommits',
      'fetchHistory',
      'compareRefs',
      'fetchCommit',
    ]);

    await expect(gateway.fetchIssues(issues)).resolves.toBe(success);
    await expect(gateway.searchCommits(search)).resolves.toBe(success);
    await expect(gateway.fetchHistory(history)).resolves.toBe(success);
    await expect(gateway.compareRefs(compare)).resolves.toBe(success);
    await expect(gateway.fetchCommit(commit)).resolves.toBe(success);

    expect(githubIssues.fetchIssues).toHaveBeenCalledWith(issues, authInfo);
    expect(githubCommitSearch.searchCommits).toHaveBeenCalledWith(
      search,
      authInfo
    );
    expect(githubHistory.fetchHistory).toHaveBeenCalledWith(history, authInfo);
    expect(githubCompare.compareRefs).toHaveBeenCalledWith(compare, authInfo);
    expect(githubCommit.fetchCommit).toHaveBeenCalledWith(commit, authInfo);
  });

  it('rejects providers that do not expose the GitHub history gateway', () => {
    expect(() =>
      requireGitHubHistoryProvider({ type: 'github' } as never, ['fetchCommit'])
    ).toThrow('GitHub history operations are unavailable');
  });

  it('delegates every provider operation with the configured token', async () => {
    const provider = new GitHubProvider({ type: 'github', token: 'secret' });
    const authInfo = { token: 'secret' };
    const codeQuery = { keywords: ['needle'], projectId: 'owner/repo' };
    const contentQuery = { projectId: 'owner/repo', path: 'README.md' };
    const repoQuery = { keywords: ['octocode'] };
    const pullRequestQuery = { projectId: 'owner/repo' };
    const structureQuery = { projectId: 'owner/repo' };

    await expect(provider.searchCode(codeQuery)).resolves.toBe(success);
    await expect(provider.getFileContent(contentQuery)).resolves.toBe(success);
    await expect(provider.searchRepos(repoQuery)).resolves.toBe(success);
    await expect(provider.searchPullRequests(pullRequestQuery)).resolves.toBe(
      success
    );
    await expect(provider.getRepoStructure(structureQuery)).resolves.toBe(
      success
    );

    expect(githubSearch.searchCode).toHaveBeenCalledWith(
      codeQuery,
      authInfo,
      expect.any(Function)
    );
    expect(githubContent.getFileContent).toHaveBeenCalledWith(
      contentQuery,
      authInfo,
      expect.any(Function)
    );
    expect(githubSearch.searchRepos).toHaveBeenCalledWith(repoQuery, authInfo);
    expect(githubPullRequests.searchPullRequests).toHaveBeenCalledWith(
      pullRequestQuery,
      authInfo,
      expect.any(Function)
    );
    expect(githubStructure.getRepoStructure).toHaveBeenCalledWith(
      structureQuery,
      authInfo,
      expect.any(Function)
    );
  });

  it('normalizes delegated errors into provider responses', async () => {
    const failure = new Error('socket closed');
    githubSearch.searchCode.mockRejectedValue(failure);
    const provider = new GitHubProvider({ type: 'github' });

    await expect(provider.searchCode({ keywords: ['x'] })).resolves.toEqual({
      error: 'normalized failure',
      status: 503,
      provider: 'github',
      hints: undefined,
      rateLimit: undefined,
    });
    expect(githubErrors.handleGitHubAPIError).toHaveBeenCalledWith(failure);
  });

  it('validates project ids before resolving the default branch', async () => {
    const authInfo = { token: 'auth-token' };
    const provider = new GitHubProvider({ type: 'github', authInfo });

    await expect(provider.resolveDefaultBranch('invalid')).rejects.toThrow(
      "Invalid GitHub projectId format: 'invalid'"
    );
    await expect(provider.resolveDefaultBranch('owner/repo')).resolves.toBe(
      'main'
    );
    expect(githubClient.resolveDefaultBranch).toHaveBeenCalledWith(
      'owner',
      'repo',
      authInfo
    );
  });
});
