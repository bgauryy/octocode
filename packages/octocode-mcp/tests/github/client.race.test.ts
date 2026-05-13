import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { getOctokit, clearOctokitInstances } from '../../src/github/client.js';

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
  const mockOctokitClass = vi.fn(function (options) {
    return {
      options,
      rest: {
        repos: {
          get: vi.fn(),
        },
      },
    };
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

import { getGitHubToken } from '../../src/serverConfig.js';
import { Octokit } from 'octokit';

const mockGetGitHubToken = vi.mocked(getGitHubToken);
const mockOctokit = vi.mocked(Octokit);

describe('GitHub Client Race Conditions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearOctokitInstances();

    // Simulate async token retrieval without a real timer.
    mockGetGitHubToken.mockImplementation(async () => {
      await Promise.resolve();
      return 'default-token';
    });
  });

  afterEach(() => {
    clearOctokitInstances();
  });

  it('should handle concurrent default instance creation without race condition', async () => {
    // Start multiple requests for the default client simultaneously
    const promise1 = getOctokit();
    const promise2 = getOctokit();
    const promise3 = getOctokit();

    const [instance1, instance2, instance3] = await Promise.all([
      promise1,
      promise2,
      promise3,
    ]);

    // All should be the same instance
    expect(instance1).toBe(instance2);
    expect(instance2).toBe(instance3);

    // Constructor should be called exactly ONCE despite 3 concurrent requests
    expect(mockOctokit).toHaveBeenCalledTimes(1);
  });

  it('should maintain separate instances for different auth tokens concurrently', async () => {
    const auth1 = { token: 'token-1', clientId: 'c1', scopes: [] };
    const auth2 = { token: 'token-2', clientId: 'c2', scopes: [] };

    const [instance1, instance2] = await Promise.all([
      getOctokit(auth1),
      getOctokit(auth2),
    ]);

    expect(instance1).not.toBe(instance2);
    expect(mockOctokit).toHaveBeenCalledTimes(2);

    // Verify correct tokens used

    expect((instance1 as any).options.auth).toBe('token-1');

    expect((instance2 as any).options.auth).toBe('token-2');
  });

  it('should not overwrite default instance when requesting specific auth', async () => {
    // 1. Initialize default
    const defaultInstance = await getOctokit();

    // 2. Request specific auth
    const authInstance = await getOctokit({
      token: 'specific',
      clientId: 'c',
      scopes: [],
    });

    // 3. Request default again
    const defaultInstanceAgain = await getOctokit();

    expect(defaultInstance).toBe(defaultInstanceAgain);
    expect(defaultInstance).not.toBe(authInstance);
  });
});
