import { describe, expect, it, vi } from 'vitest';
import { RequestError } from 'octokit';
import { GitHubSearchQuerySchema } from '@octocodeai/octocode-core/schema';
import { exploreRepositoryStructure } from '../../src/tools/github_view_repo_structure/execution.js';
import { buildGitHubSearchFinalizer } from '../../src/tools/github_search/finalizer.js';

async function execute(provider: object, branch?: string) {
  const query = GitHubSearchQuerySchema.parse({
    operation: 'tree',
    owner: 'fixture',
    repo: 'rate-errors',
    branch,
  });
  const data = await exploreRepositoryStructure(
    query as never,
    {} as never,
    () => ({ provider }) as never
  );
  const finalized = buildGitHubSearchFinalizer()({
    queries: [query],
    results: [{ index: 0, data }],
    config: { toolName: 'ghSearch' },
  } as never);
  return (
    finalized.structuredContent as {
      results: Array<{ data: Record<string, unknown> }>;
    }
  ).results[0]!.data;
}

describe('public tree rate-limit diagnostics', () => {
  it.each([403, 429])(
    'preserves a thrown metadata HTTP %i without more discovery',
    async status => {
      const getRepoStructure = vi.fn();
      const result = await execute({
        getRepoStructure,
        resolveDefaultBranch: async () => {
          throw new RequestError('Secondary rate limit exceeded', status, {
            request: {
              method: 'GET',
              url: 'https://api.github.com/fixture',
              headers: {},
            },
            response: {
              status,
              url: 'https://api.github.com/fixture',
              headers: {
                'retry-after': '90',
                'x-ratelimit-remaining': '7',
                'x-ratelimit-reset': '2000000000',
              },
              data: {},
            },
          });
        },
      });
      expect(result).toMatchObject({
        statusCode: status,
        retryAfter: 90,
        rateLimitRemaining: 7,
        rateLimitReset: 2000000000000,
      });
      expect(result.error).toContain('secondary rate limit');
      expect(result.next).toBeUndefined();
      expect(getRepoStructure).not.toHaveBeenCalled();
    }
  );

  it('retains rate guidance from an ordinary provider error response', async () => {
    const resolveDefaultBranch = vi.fn();
    const result = await execute(
      {
        resolveDefaultBranch,
        getRepoStructure: async () => ({
          provider: 'github',
          status: 429,
          error: 'Secondary rate limit exceeded',
          rateLimit: { remaining: 7, reset: 2000000000, retryAfter: 90 },
        }),
      },
      'a'.repeat(40)
    );
    expect(result).toMatchObject({
      statusCode: 429,
      retryAfter: 90,
      rateLimitRemaining: 7,
      rateLimitReset: 2000000000000,
    });
    expect(resolveDefaultBranch).not.toHaveBeenCalled();
  });

  it('keeps unexpected local failures separate from provider limits', async () => {
    const result = await execute({
      resolveDefaultBranch: async () => {
        throw new Error('local initialization failed');
      },
    });
    expect(result.errorCode).toBe('toolExecutionFailed');
    expect(result.statusCode).toBeUndefined();
    expect(result.retryAfter).toBeUndefined();
  });
});
