import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  getOctokit,
  OctokitWithThrottling,
  clearOctokitInstances,
  resolveDefaultBranch,
} from '../../src/github/client.js';

vi.mock('../../src/serverConfig.js', () => ({
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

import { getGitHubToken, getServerConfig } from '../../src/serverConfig.js';
import { Octokit } from 'octokit';

const mockGetGitHubToken = vi.mocked(getGitHubToken);
const mockGetServerConfig = vi.mocked(getServerConfig);
const mockOctokit = vi.mocked(Octokit);

describe('GitHub Client', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearOctokitInstances();

    mockGetServerConfig.mockReturnValue({
      version: '1.0.0',
      githubApiUrl: 'https://api.github.com',
      timeout: 30000,
      maxRetries: 3,
      loggingEnabled: true,
      enableLocal: true,
      enableClone: false,
      outputFormat: 'yaml',
      tokenSource: 'env:GH_TOKEN',
    });
  });

  afterEach(() => {
    clearOctokitInstances();
  });

  describe('getOctokit', () => {
    it('should create Octokit instance with token', async () => {
      const testToken = 'test-token';
      mockGetGitHubToken.mockResolvedValue(testToken);

      await getOctokit();

      expect(mockOctokit).toHaveBeenCalledWith({
        userAgent: expect.stringMatching(/^octocode-mcp\//),
        baseUrl: 'https://api.github.com',
        request: { timeout: 30000 },
        throttle: {
          onRateLimit: expect.any(Function),
          onSecondaryRateLimit: expect.any(Function),
        },
        auth: testToken,
      });
    });

    it('should create Octokit instance without token if none provided', async () => {
      mockGetGitHubToken.mockResolvedValue(null);

      await getOctokit();

      expect(mockOctokit).toHaveBeenCalledWith({
        userAgent: expect.stringMatching(/^octocode-mcp\//),
        baseUrl: 'https://api.github.com',
        request: { timeout: 30000 },
        throttle: {
          onRateLimit: expect.any(Function),
          onSecondaryRateLimit: expect.any(Function),
        },
      });
    });

    it('should use provided auth token over config token', async () => {
      mockGetGitHubToken.mockResolvedValue('config-token');
      const authInfo = {
        token: 'auth-token',
        clientId: 'test-client',
        scopes: [],
      };

      await getOctokit(authInfo);

      expect(mockOctokit).toHaveBeenCalledWith({
        userAgent: expect.stringMatching(/^octocode-mcp\//),
        baseUrl: 'https://api.github.com',
        request: { timeout: 30000 },
        throttle: {
          onRateLimit: expect.any(Function),
          onSecondaryRateLimit: expect.any(Function),
        },
        auth: 'auth-token',
      });
    });

    it('should reuse cached instance when no authInfo provided', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');

      const instance1 = await getOctokit();
      const instance2 = await getOctokit();

      expect(instance1).toBe(instance2);
      expect(mockOctokit).toHaveBeenCalledTimes(1);
    });

    it('should create new instance when authInfo is provided', async () => {
      mockGetGitHubToken.mockResolvedValue('config-token');

      await getOctokit();
      await getOctokit({
        token: 'new-token',
        clientId: 'test-client',
        scopes: [],
      });

      expect(mockOctokit).toHaveBeenCalledTimes(2);
    });

    it('should use server config timeout', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://api.github.com',
        timeout: 60000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: true,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GH_TOKEN',
      });

      await getOctokit();

      expect(mockOctokit).toHaveBeenCalledWith(
        expect.objectContaining({
          request: { timeout: 60000 },
        })
      );
    });

    it('should use custom GitHub API URL from config', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');
      mockGetServerConfig.mockReturnValue({
        version: '1.0.0',
        githubApiUrl: 'https://github.enterprise.com/api/v3',
        timeout: 30000,
        maxRetries: 3,
        loggingEnabled: true,
        enableLocal: true,
        enableClone: false,
        outputFormat: 'yaml',
        tokenSource: 'env:GH_TOKEN',
      });

      await getOctokit();

      expect(mockOctokit).toHaveBeenCalledWith(
        expect.objectContaining({
          baseUrl: 'https://github.enterprise.com/api/v3',
        })
      );
    });
  });

  describe('clearOctokitInstances', () => {
    it('should clear cached Octokit instance', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');

      const mockInstance1 = {
        rest: { repos: { get: vi.fn(function () {}) } },
      };
      const mockInstance2 = {
        rest: { repos: { get: vi.fn(function () {}) } },
      };
      mockOctokit
        .mockImplementationOnce(function () {
          return mockInstance1;
        })
        .mockImplementationOnce(function () {
          return mockInstance2;
        });

      const instance1 = await getOctokit();

      clearOctokitInstances();

      const instance2 = await getOctokit();

      expect(instance1).not.toBe(instance2);
      expect(mockOctokit).toHaveBeenCalledTimes(2);
    });
  });

  describe('OctokitWithThrottling', () => {
    it('should export OctokitWithThrottling class', () => {
      expect(typeof OctokitWithThrottling).toEqual('function');
      expect(OctokitWithThrottling.name.length > 0).toEqual(true);
    });
  });

  describe('resolveDefaultBranch', () => {
    it('should return default branch from GitHub API', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');
      const mockReposGet = vi.fn().mockResolvedValue({
        data: { default_branch: 'develop' },
      });
      mockOctokit.mockImplementation(function () {
        return { rest: { repos: { get: mockReposGet } } };
      });

      const branch = await resolveDefaultBranch('org', 'repo');

      expect(branch).toBe('develop');
      expect(mockReposGet).toHaveBeenCalledWith({ owner: 'org', repo: 'repo' });
    });

    it('should fall back to "main" via getBranch when repos.get fails', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');
      const mockReposGet = vi.fn().mockRejectedValue(new Error('Not found'));
      const mockGetBranch = vi
        .fn()
        .mockResolvedValueOnce({ data: { name: 'main' } });
      mockOctokit.mockImplementation(function () {
        return {
          rest: { repos: { get: mockReposGet, getBranch: mockGetBranch } },
        };
      });

      const branch = await resolveDefaultBranch('org', 'repo');

      expect(branch).toBe('main');
      expect(mockGetBranch).toHaveBeenCalledWith({
        owner: 'org',
        repo: 'repo',
        branch: 'main',
      });
    });

    it('should fall back to "master" when repos.get and "main" both fail', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');
      const mockReposGet = vi.fn().mockRejectedValue(new Error('Not found'));
      const mockGetBranch = vi
        .fn()
        .mockRejectedValueOnce(new Error('main not found'))
        .mockResolvedValueOnce({ data: { name: 'master' } });
      mockOctokit.mockImplementation(function () {
        return {
          rest: { repos: { get: mockReposGet, getBranch: mockGetBranch } },
        };
      });

      const branch = await resolveDefaultBranch('org', 'repo');

      expect(branch).toBe('master');
      expect(mockGetBranch).toHaveBeenCalledTimes(2);
    });

    it('should throw when all resolution attempts fail', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');
      const mockReposGet = vi.fn().mockRejectedValue(new Error('Not found'));
      const mockGetBranch = vi.fn().mockRejectedValue(new Error('Not found'));
      mockOctokit.mockImplementation(function () {
        return {
          rest: { repos: { get: mockReposGet, getBranch: mockGetBranch } },
        };
      });

      await expect(resolveDefaultBranch('org', 'repo')).rejects.toThrow(
        'Could not determine default branch'
      );
    });

    it('should throw when getOctokit fails', async () => {
      mockGetGitHubToken.mockRejectedValue(new Error('No token'));

      await expect(resolveDefaultBranch('org', 'repo')).rejects.toThrow();
    });

    it('should forward authInfo to getOctokit', async () => {
      const authInfo = {
        token: 'oauth-token',
        clientId: 'test-client',
        scopes: [],
      };
      const mockReposGet = vi.fn().mockResolvedValue({
        data: { default_branch: 'main' },
      });
      mockOctokit.mockImplementation(function () {
        return { rest: { repos: { get: mockReposGet } } };
      });

      await resolveDefaultBranch('org', 'repo', authInfo);

      expect(mockOctokit).toHaveBeenCalledWith(
        expect.objectContaining({ auth: 'oauth-token' })
      );
    });

    it('should handle repos with "master" as default branch', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');
      const mockReposGet = vi.fn().mockResolvedValue({
        data: { default_branch: 'master' },
      });
      mockOctokit.mockImplementation(function () {
        return { rest: { repos: { get: mockReposGet } } };
      });

      const branch = await resolveDefaultBranch('legacy-org', 'old-repo');

      expect(branch).toBe('master');
    });
  });

  describe('throttle configuration', () => {
    it('should configure throttling options correctly', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');

      await getOctokit();

      const callArgs = mockOctokit.mock.calls[0][0];
      expect(typeof callArgs.throttle).toEqual('object');
      expect(typeof callArgs.throttle.onRateLimit).toEqual('function');
      expect(typeof callArgs.throttle.onSecondaryRateLimit).toEqual('function');
    });

    it('should never retry on rate limit - fail immediately', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');

      await getOctokit();

      const callArgs = mockOctokit.mock.calls[0][0];
      const { onRateLimit } = callArgs.throttle;

      expect(onRateLimit(3600, {}, {}, 0)).toBe(false);
      expect(onRateLimit(3600, {}, {}, 1)).toBe(false);
      expect(onRateLimit(3600, {}, {}, 5)).toBe(false);
    });

    it('should never retry on secondary rate limit - fail immediately', async () => {
      mockGetGitHubToken.mockResolvedValue('test-token');

      await getOctokit();

      const callArgs = mockOctokit.mock.calls[0][0];
      const { onSecondaryRateLimit } = callArgs.throttle;

      expect(onSecondaryRateLimit(60, {}, {}, 0)).toBe(false);
      expect(onSecondaryRateLimit(60, {}, {}, 1)).toBe(false);
      expect(onSecondaryRateLimit(60, {}, {}, 5)).toBe(false);
    });
  });
});
