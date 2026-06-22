import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  resolveDefaultBranch,
  clearOctokitInstances,
  MAX_BRANCH_CACHE_SIZE,
} from '../../../octocode-tools-core/src/github/client.js';

vi.mock('../../../octocode-tools-core/src/serverConfig.js', () => ({
  getGitHubToken: vi.fn(function () {}),
  getServerConfig: vi.fn(function () {
    return {
      timeout: 30000,
      version: '1.0.0',
    };
  }),
}));

vi.mock('octokit', () => {
  const mockOctokitInstance = {
    rest: {
      repos: {
        get: vi.fn(function () {}),
        getBranch: vi.fn(function () {}),
      },
    },
  };

  const mockOctokitClass = vi.fn(function () {
    return mockOctokitInstance;
  });

  Object.assign(mockOctokitClass, {
    plugin: vi.fn(function () {
      return mockOctokitClass;
    }),
  });

  return {
    Octokit: mockOctokitClass,
  };
});

vi.mock('@octokit/plugin-throttling', () => ({
  throttling: {},
}));

import {
  getGitHubToken,
  getServerConfig,
} from '../../../octocode-tools-core/src/serverConfig.js';
import { Octokit } from 'octokit';

const mockGetGitHubToken = vi.mocked(getGitHubToken);
const mockGetServerConfig = vi.mocked(getServerConfig);
const mockOctokit = vi.mocked(Octokit);

describe('resolveDefaultBranch - caching', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearOctokitInstances();

    mockGetServerConfig.mockReturnValue({
      version: '1.0.0',
      githubApiUrl: 'https://api.github.com',
      timeout: 30000,
      maxRetries: 3,
      enableLocal: true,
      enableClone: false,
      outputFormat: 'yaml',
      tokenSource: 'env:GH_TOKEN',
    });
  });

  afterEach(() => {
    clearOctokitInstances();
  });

  it('should NOT call GitHub API twice for the same owner/repo', async () => {
    mockGetGitHubToken.mockResolvedValue('test-token');
    const mockReposGet = vi.fn().mockResolvedValue({
      data: { default_branch: 'develop' },
    });
    mockOctokit.mockImplementation(function () {
      return { rest: { repos: { get: mockReposGet } } };
    });

    const branch1 = await resolveDefaultBranch('org', 'repo');
    const branch2 = await resolveDefaultBranch('org', 'repo');

    expect(branch1).toBe('develop');
    expect(branch2).toBe('develop');

    expect(mockReposGet).toHaveBeenCalledTimes(1);
  });

  it('should call GitHub API separately for different repos', async () => {
    mockGetGitHubToken.mockResolvedValue('test-token');
    const mockReposGet = vi
      .fn()
      .mockResolvedValueOnce({ data: { default_branch: 'main' } })
      .mockResolvedValueOnce({ data: { default_branch: 'master' } });
    mockOctokit.mockImplementation(function () {
      return { rest: { repos: { get: mockReposGet } } };
    });

    const branch1 = await resolveDefaultBranch('org', 'repo-a');
    const branch2 = await resolveDefaultBranch('org', 'repo-b');

    expect(branch1).toBe('main');
    expect(branch2).toBe('master');
    expect(mockReposGet).toHaveBeenCalledTimes(2);
  });

  it('should cache smart fallback result when repos.get fails but getBranch succeeds', async () => {
    mockGetGitHubToken.mockResolvedValue('test-token');
    const mockReposGet = vi.fn().mockRejectedValue(new Error('Not found'));
    const mockGetBranch = vi.fn().mockResolvedValue({ data: { name: 'main' } });
    mockOctokit.mockImplementation(function () {
      return {
        rest: { repos: { get: mockReposGet, getBranch: mockGetBranch } },
      };
    });

    const branch1 = await resolveDefaultBranch('org', 'fallback-repo');
    const branch2 = await resolveDefaultBranch('org', 'fallback-repo');

    expect(branch1).toBe('main');
    expect(branch2).toBe('main');

    expect(mockReposGet).toHaveBeenCalledTimes(1);
    expect(mockGetBranch).toHaveBeenCalledTimes(1);
  });

  it('should throw when all resolution attempts fail (not cached)', async () => {
    mockGetGitHubToken.mockResolvedValue('test-token');
    const mockReposGet = vi.fn().mockRejectedValue(new Error('Not found'));
    const mockGetBranch = vi.fn().mockRejectedValue(new Error('Not found'));
    mockOctokit.mockImplementation(function () {
      return {
        rest: { repos: { get: mockReposGet, getBranch: mockGetBranch } },
      };
    });

    await expect(resolveDefaultBranch('org', 'missing-repo')).rejects.toThrow(
      'Could not determine default branch'
    );

    expect(mockGetBranch).toHaveBeenCalledTimes(2);
  });

  it('should not exceed MAX_BRANCH_CACHE_SIZE', async () => {
    mockGetGitHubToken.mockResolvedValue('test-token');
    const mockReposGet = vi
      .fn()
      .mockImplementation(({ repo }: { repo: string }) => ({
        data: { default_branch: `branch-${repo}` },
      }));
    mockOctokit.mockImplementation(function () {
      return { rest: { repos: { get: mockReposGet } } };
    });

    for (let i = 0; i < MAX_BRANCH_CACHE_SIZE + 10; i++) {
      await resolveDefaultBranch('org', `repo-${i}`);
    }

    mockReposGet.mockClear();
    await resolveDefaultBranch('org', 'repo-0');
    expect(mockReposGet).toHaveBeenCalledTimes(1);
  });

  it('should retain recent entries when cache is full', async () => {
    mockGetGitHubToken.mockResolvedValue('test-token');
    const mockReposGet = vi
      .fn()
      .mockImplementation(({ repo }: { repo: string }) => ({
        data: { default_branch: `branch-${repo}` },
      }));
    mockOctokit.mockImplementation(function () {
      return { rest: { repos: { get: mockReposGet } } };
    });

    for (let i = 0; i < MAX_BRANCH_CACHE_SIZE + 5; i++) {
      await resolveDefaultBranch('org', `repo-${i}`);
    }

    mockReposGet.mockClear();
    const lastRepo = `repo-${MAX_BRANCH_CACHE_SIZE + 4}`;
    const branch = await resolveDefaultBranch('org', lastRepo);
    expect(branch).toBe(`branch-${lastRepo}`);
    expect(mockReposGet).not.toHaveBeenCalled();
  });

  it('should clear branch cache when clearOctokitInstances is called', async () => {
    mockGetGitHubToken.mockResolvedValue('test-token');
    const mockReposGet = vi
      .fn()
      .mockResolvedValueOnce({ data: { default_branch: 'develop' } })
      .mockResolvedValueOnce({ data: { default_branch: 'main' } });
    mockOctokit.mockImplementation(function () {
      return { rest: { repos: { get: mockReposGet } } };
    });

    const branch1 = await resolveDefaultBranch('org', 'repo');
    expect(branch1).toBe('develop');

    clearOctokitInstances();

    const branch2 = await resolveDefaultBranch('org', 'repo');
    expect(branch2).toBe('main');

    expect(mockReposGet).toHaveBeenCalledTimes(2);
  });
});
