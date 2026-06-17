import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  createSuccessResult,
  createErrorResult,
  handleProviderError,
  handleCatchError,
  invokeCallbackSafely,
} from '../../../octocode-tools-core/src/tools/utils.js';
import type { GitHubAPIError } from '../../../octocode-tools-core/src/github/githubAPI.js';
import type { ToolInvocationCallback } from '../../../octocode-tools-core/src/types/toolResults.js';
import { logSessionError } from '../../../octocode-tools-core/src/session.js';

vi.mock('../../../octocode-tools-core/src/session.js', () => ({
  logSessionError: vi.fn().mockResolvedValue(undefined),
}));

function expectNoResearchContext(result: Record<string, unknown>): void {
  expect(result).not.toHaveProperty('mainResearchGoal');
  expect(result).not.toHaveProperty('researchGoal');
  expect(result).not.toHaveProperty('reasoning');
}

describe('Tools Utils', () => {
  describe('createSuccessResult', () => {
    it('should create success result with hasResults status', () => {
      const query = {
        researchGoal: 'Find test files',
        reasoning: 'Looking for tests',
      };
      const data = { files: ['test1.ts', 'test2.ts'] };

      const result = createSuccessResult(
        query,
        data,
        true,
        'GITHUB_SEARCH_CODE'
      );

      expect(result.status).toBeUndefined();
      expectNoResearchContext(result);
      expect(result.files).toEqual(['test1.ts', 'test2.ts']);
    });

    it('should create success result with empty status', () => {
      const query = {
        researchGoal: 'Find test files',
        reasoning: 'Looking for tests',
      };
      const data = { files: [] };

      const result = createSuccessResult(
        query,
        data,
        false,
        'GITHUB_SEARCH_CODE'
      );

      expect(result.status).toBe('empty');
      expectNoResearchContext(result);
      expect(result.files).toEqual([]);
    });

    it('should propagate extra hints to the final result', () => {
      const query = {
        researchGoal: 'Find repositories',
        reasoning: 'Searching for repos',
      };
      const data = { repositories: ['repo1', 'repo2'] };
      const extraHints = [
        'Try narrowing your search with topics',
        'Consider using stars filter',
      ];

      const result = createSuccessResult(
        query,
        data,
        true,
        'GITHUB_SEARCH_REPOSITORIES',
        { extraHints }
      );

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('Try narrowing your search with topics');
      expect(result.hints).toContain('Consider using stars filter');
    });

    it('should prepend prefixHints before other hints', () => {
      const query = { researchGoal: 'Test', reasoning: 'Testing' };
      const data = { files: ['file1.ts'] };
      const prefixHints = ['Critical: Check path first'];
      const extraHints = ['Regular hint'];

      const result = createSuccessResult(query, data, true, 'localSearchCode', {
        prefixHints,
        extraHints,
      });

      expect(result.hints).toBeDefined();
      expect(result.hints![0]).toBe('Critical: Check path first');
      expect(result.hints).toContain('Regular hint');
    });

    it('should NOT include hints property when extraHints is empty array', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const data = { files: ['file1.ts'] };

      const result = createSuccessResult(
        query,
        data,
        true,
        'GITHUB_SEARCH_CODE',
        { extraHints: [] }
      );

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeUndefined();
    });

    it('should NOT include hints property when customHints is undefined', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const data = { files: ['file1.ts'] };

      const result = createSuccessResult(
        query,
        data,
        true,
        'GITHUB_SEARCH_CODE'
      );

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeUndefined();
    });

    it('should propagate hints with empty status', () => {
      const query = {
        researchGoal: 'Find repositories',
        reasoning: 'Searching for repos',
      };
      const data = { repositories: [] };
      const extraHints = [
        'Try broader search terms',
        'Remove filters to expand results',
      ];

      const result = createSuccessResult(
        query,
        data,
        false,
        'GITHUB_SEARCH_REPOSITORIES',
        { extraHints }
      );

      expect(result.status).toBe('empty');
      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('Try broader search terms');
      expect(result.hints).toContain('Remove filters to expand results');
    });

    it('should propagate single hint', () => {
      const query = {
        researchGoal: 'Find content',
        reasoning: 'Fetching file',
      };
      const data = { content: 'file content' };

      const result = createSuccessResult(
        query,
        data,
        true,
        'GITHUB_FETCH_CONTENT',
        { extraHints: ['File found successfully'] }
      );

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('File found successfully');
    });

    it('should propagate multiple hints', () => {
      const query = {
        researchGoal: 'Find PRs',
        reasoning: 'Searching pull requests',
      };
      const data = { pull_requests: [{ number: 1 }] };
      const extraHints = [
        'First hint',
        'Second hint',
        'Third hint',
        'Fourth hint',
      ];

      const result = createSuccessResult(
        query,
        data,
        true,
        'GITHUB_SEARCH_PULL_REQUESTS',
        { extraHints }
      );

      expect(result.status).toBeUndefined();
      expect(result.hints).toBeDefined();
      extraHints.forEach(hint => {
        expect(result.hints).toContain(hint);
      });
    });

    it('should merge data properties correctly with hints', () => {
      const query = {
        researchGoal: 'Complex search',
        reasoning: 'Testing',
      };
      const data = {
        repositories: ['repo1'],
        total: 1,
        metadata: { page: 1 },
      };

      const result = createSuccessResult(
        query,
        data,
        true,
        'GITHUB_SEARCH_REPOSITORIES',
        { extraHints: ['Custom hint'] }
      );

      expect(result.status).toBeUndefined();
      expectNoResearchContext(result);
      expect(result.repositories).toEqual(['repo1']);
      expect(result.total).toBe(1);
      expect(result.metadata).toEqual({ page: 1 });
      expect(result.hints).toContain('Custom hint');
    });

    it('should deduplicate hints from extraHints', () => {
      const query = { researchGoal: 'Test', reasoning: 'Testing' };
      const data = { files: [] };
      const extraHints = [
        'Duplicate hint',
        'Unique hint',
        'Duplicate hint',
        'Another unique',
        'Duplicate hint',
      ];

      const result = createSuccessResult(query, data, true, 'localSearchCode', {
        extraHints,
      });

      expect(result.hints).toBeDefined();
      const duplicateCount = result.hints!.filter(
        h => h === 'Duplicate hint'
      ).length;
      expect(duplicateCount).toBe(1);
      expect(result.hints).toContain('Unique hint');
      expect(result.hints).toContain('Another unique');
    });

    it('should filter out empty string hints', () => {
      const query = { researchGoal: 'Test', reasoning: 'Testing' };
      const data = { files: [] };
      const extraHints = ['Valid hint', '', 'Another valid', ''];

      const result = createSuccessResult(query, data, true, 'localSearchCode', {
        extraHints,
      });

      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('Valid hint');
      expect(result.hints).toContain('Another valid');
      expect(result.hints).not.toContain('');
    });

    it('should filter out whitespace-only hints', () => {
      const query = { researchGoal: 'Test', reasoning: 'Testing' };
      const data = { files: [] };
      const extraHints = [
        'Valid hint',
        '   ',
        '\t\n',
        '  \n  ',
        'Another valid',
      ];

      const result = createSuccessResult(query, data, true, 'localSearchCode', {
        extraHints,
      });

      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('Valid hint');
      expect(result.hints).toContain('Another valid');
      expect(result.hints!.some(h => h.trim() === '')).toBe(false);
    });

    it('should handle all empty/whitespace extraHints', () => {
      const query = { researchGoal: 'Test', reasoning: 'Testing' };
      const data = { files: [] };
      const extraHints = ['', '   ', '\t'];

      const result = createSuccessResult(query, data, true, 'localSearchCode', {
        extraHints,
      });

      if (result.hints) {
        expect(result.hints.every(h => h.trim().length > 0)).toBe(true);
      }
    });

    it('should filter out non-string hints without crashing', () => {
      const query = { researchGoal: 'Test', reasoning: 'Testing' };
      const data = { files: ['file1.ts'] };
      const extraHints = [
        'Valid hint',
        42 as unknown as string,
        null as unknown as string,
        undefined as unknown as string,
        { obj: true } as unknown as string,
        'Another valid',
      ];

      const result = createSuccessResult(query, data, true, 'localSearchCode', {
        extraHints,
      });

      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('Valid hint');
      expect(result.hints).toContain('Another valid');
      result.hints!.forEach(h => {
        expect(typeof h).toBe('string');
        expect(h.trim().length).toBeGreaterThan(0);
      });
    });
  });

  describe('createErrorResult', () => {
    it('should create error result without hints', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const error = 'API error occurred';

      const result = createErrorResult(error, query);

      expect(result.status).toBe('error');
      expect(result.error).toBe('API error occurred');
      expectNoResearchContext(result);
      expect(result.hints).toBeUndefined();
    });

    it('should include hints from API error', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const apiError: GitHubAPIError = {
        error: 'Rate limit exceeded',
        type: 'http',
        status: 429,
        rateLimitRemaining: 0,
        rateLimitReset: Date.now() + 3600000,
      };

      const result = createErrorResult('Rate limit error', query, {
        hintSourceError: apiError,
      });

      expect(result.status).toBe('error');
      expect(result.error).toBe('Rate limit error');
      expect(result.hints).toBeDefined();
      expect(result.hints!.length).toBeGreaterThan(0);
      expect(result.hints!.some(h => h.includes('Rate limit'))).toBe(true);
    });

    it('should include retry after hint', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const apiError: GitHubAPIError = {
        error: 'Too many requests',
        type: 'http',
        status: 429,
        retryAfter: 60,
      };

      const result = createErrorResult('Too many requests', query, {
        hintSourceError: apiError,
      });

      expect(result.status).toBe('error');
      expect(result.hints).toBeDefined();
      expect(result.hints!.some(h => h.includes('Retry after 60'))).toBe(true);
    });

    it('should include scopes suggestion hint', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const apiError: GitHubAPIError = {
        error: 'Insufficient permissions',
        type: 'http',
        status: 403,
        scopesSuggestion: 'Required scopes: repo, read:org',
      };

      const result = createErrorResult('Permission denied', query, {
        hintSourceError: apiError,
      });

      expect(result.status).toBe('error');
      expect(result.hints).toBeDefined();
      expect(
        result.hints!.some(h => h.includes('Required scopes: repo, read:org'))
      ).toBe(true);
    });
  });

  describe('handleProviderError', () => {
    it('should return error result for provider error responses', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const apiResult = {
        error: 'API error',
        status: 500,
        provider: 'github' as const,
      };

      const result = handleProviderError(apiResult, query);

      expect(result.status).toBe('error');
      expect(result.error).toEqual(
        expect.objectContaining({
          error: 'API error',
          type: 'http',
          status: 500,
        })
      );
    });

    it('should propagate hints from provider error response', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const apiResult = {
        error: 'Authentication failed',
        status: 401,
        provider: 'github' as const,
        hints: ['Check your GitHub token', 'Verify token permissions'],
      };

      const result = handleProviderError(apiResult, query);

      expect(result.status).toBe('error');
      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('Check your GitHub token');
      expect(result.hints).toContain('Verify token permissions');
    });

    it('should handle non-array hints without crashing', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const apiResult = {
        error: 'Bad response',
        status: 500,
        provider: 'github' as const,
        hints: 'not-an-array' as unknown as string[],
      };

      const result = handleProviderError(apiResult, query);

      expect(result.status).toBe('error');
      expect(!result.hints || Array.isArray(result.hints)).toBe(true);
    });

    it('should preserve rate limit data from provider response', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const resetSeconds = Math.floor(Date.now() / 1000) + 3600;
      const apiResult = {
        error: 'Rate limit exceeded',
        status: 429,
        provider: 'github' as const,
        hints: ['Custom API hint'],
        rateLimit: {
          remaining: 0,
          reset: resetSeconds,
          retryAfter: 60,
        },
      };

      const result = handleProviderError(apiResult, query);

      expect(result.status).toBe('error');
      expect(result.hints).toBeDefined();
      expect(result.hints).toContain('Custom API hint');
      expect(result.hints!.some(h => h.includes('Rate limit'))).toBe(true);
      expect(result.hints!.some(h => h.includes('Retry after 60'))).toBe(true);
    });

    it('should NOT produce duplicate hints for rate limit errors', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const resetSeconds = Math.floor(Date.now() / 1000) + 3600;
      const apiResult = {
        error: 'Rate limit exceeded',
        status: 429,
        provider: 'github' as const,
        rateLimit: {
          remaining: 0,
          reset: resetSeconds,
        },
      };

      const result = handleProviderError(apiResult, query);

      expect(result.hints).toBeDefined();

      const hintCounts = new Map<string, number>();
      for (const hint of result.hints || []) {
        hintCounts.set(hint, (hintCounts.get(hint) || 0) + 1);
      }

      for (const [hint, count] of hintCounts) {
        expect(count).toBe(1);
        if (count > 1) {
          throw new Error(
            `Duplicate hint found: "${hint}" appears ${count} times`
          );
        }
      }

      // 'API Error' echo removed — raw error string is no longer emitted as a hint
      expect(
        result.hints!.some(
          h => h.startsWith('Rate limit:') && h.includes('remaining')
        )
      ).toBe(true);
    });

    it('should NOT produce duplicate hints for retry after errors', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const apiResult = {
        error: 'Too many requests',
        status: 429,
        provider: 'github' as const,
        rateLimit: {
          remaining: 0,
          reset: Math.floor(Date.now() / 1000) + 3600,
          retryAfter: 60,
        },
      };

      const result = handleProviderError(apiResult, query);

      expect(result.hints).toBeDefined();

      const hintCounts = new Map<string, number>();
      for (const hint of result.hints || []) {
        hintCounts.set(hint, (hintCounts.get(hint) || 0) + 1);
      }

      for (const [hint, count] of hintCounts) {
        expect(count).toBe(1);
        if (count > 1) {
          throw new Error(
            `Duplicate hint found: "${hint}" appears ${count} times`
          );
        }
      }

      expect(result.hints!.some(h => h.includes('Retry after 60'))).toBe(true);
    });

    it('should use default error message when error is undefined', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const apiResult = {
        error: undefined as unknown as string,
        status: 500,
        provider: 'github' as const,
      };

      const result = handleProviderError(apiResult, query);

      expect(result.status).toBe('error');
      expect(result.error).toEqual(
        expect.objectContaining({ error: 'Provider error' })
      );
    });
  });

  describe('handleCatchError', () => {
    it('should handle Error objects', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const error = new Error('Something went wrong');

      const result = handleCatchError(error, query);

      expect(result.status).toBe('error');
      expect(result.error).toBe('Something went wrong');
      expectNoResearchContext(result);
    });

    it('should handle Error objects with context message', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const error = new Error('Network failure');
      const contextMessage = 'Failed to fetch data';

      const result = handleCatchError(error, query, contextMessage);

      expect(result.status).toBe('error');
      expect(result.error).toBe('Failed to fetch data: Network failure');
    });

    it('should use toolName for logging when provided', () => {
      const query = { researchGoal: 'Test', reasoning: 'Testing' };
      const error = new Error('Tool execution failed');

      const result = handleCatchError(
        error,
        query,
        'Package search failed',
        'npmSearch'
      );

      expect(result.status).toBe('error');
      expect(result.error).toBe('Package search failed: Tool execution failed');
      expect(logSessionError).toHaveBeenCalledWith(
        'npmSearch',
        expect.any(String)
      );
    });

    it('should handle unknown error types', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const error = 'String error';

      const result = handleCatchError(error, query);

      expect(result.status).toBe('error');
      expect(result.error).toBe('Unknown error occurred');
    });

    it('should not include hints in catch errors', () => {
      const query = {
        researchGoal: 'Find files',
        reasoning: 'Searching',
      };
      const error = new Error('Test error');

      const result = handleCatchError(error, query);

      expect(result.status).toBe('error');
      expect(result.hints).toBeUndefined();
    });
  });

  describe('Integration - Hints propagation in full flow', () => {
    it('should propagate extra hints through createSuccessResult for all tool types', () => {
      const toolNames = [
        'GITHUB_SEARCH_CODE',
        'GITHUB_SEARCH_REPOSITORIES',
        'GITHUB_FETCH_CONTENT',
        'GITHUB_SEARCH_PULL_REQUESTS',
        'GITHUB_VIEW_REPO_STRUCTURE',
      ] as const;

      toolNames.forEach(toolName => {
        const query = {
          researchGoal: `Test ${toolName}`,
          reasoning: 'Testing hints',
        };
        const data = { testData: 'value' };
        const extraHints = [`Hint for ${toolName}`];

        const result = createSuccessResult(query, data, true, toolName, {
          extraHints,
        });

        expect(result.status).toBeUndefined();
        expect(result.hints).toBeDefined();
        expect(result.hints).toContain(`Hint for ${toolName}`);
      });
    });

    it('should include extra hints in the result', () => {
      const query = {
        researchGoal: 'Test',
        reasoning: 'Testing',
      };
      const data = { items: [] };
      const extraHints = ['Hint 1', 'Hint 2'];

      const result = createSuccessResult(
        query,
        data,
        true,
        'GITHUB_SEARCH_CODE',
        {
          extraHints,
        }
      );

      expect(result.hints).toContain('Hint 1');
      expect(result.hints).toContain('Hint 2');
    });
  });

  describe('invokeCallbackSafely', () => {
    beforeEach(() => {
      vi.clearAllMocks();
    });

    it('should do nothing when callback is undefined', async () => {
      await expect(
        invokeCallbackSafely(undefined, 'TEST_TOOL', [{ query: 'test' }])
      ).resolves.toBeUndefined();

      expect(logSessionError).not.toHaveBeenCalled();
    });

    it('should invoke callback with correct arguments', async () => {
      const mockCallback: ToolInvocationCallback = vi
        .fn()
        .mockResolvedValue(undefined);
      const queries = [{ query: 'test1' }, { query: 'test2' }];

      await invokeCallbackSafely(mockCallback, 'GITHUB_SEARCH_CODE', queries);

      expect(mockCallback).toHaveBeenCalledTimes(1);
      expect(mockCallback).toHaveBeenCalledWith('GITHUB_SEARCH_CODE', queries);
      expect(logSessionError).not.toHaveBeenCalled();
    });

    it('should catch and log synchronous errors from callback', async () => {
      const mockCallback: ToolInvocationCallback = vi
        .fn()
        .mockImplementation(() => {
          throw new Error('Sync callback error');
        });

      await expect(
        invokeCallbackSafely(mockCallback, 'GITHUB_FETCH_CONTENT', [])
      ).resolves.toBeUndefined();

      expect(mockCallback).toHaveBeenCalledTimes(1);
      expect(logSessionError).toHaveBeenCalledTimes(1);
      expect(logSessionError).toHaveBeenCalledWith(
        'GITHUB_FETCH_CONTENT',
        'TOOL_EXECUTION_FAILED'
      );
    });

    it('should catch and log async rejected promises from callback', async () => {
      const mockCallback: ToolInvocationCallback = vi
        .fn()
        .mockRejectedValue(new Error('Async callback error'));

      await expect(
        invokeCallbackSafely(mockCallback, 'GITHUB_SEARCH_REPOSITORIES', [
          { name: 'test' },
        ])
      ).resolves.toBeUndefined();

      expect(mockCallback).toHaveBeenCalledTimes(1);
      expect(logSessionError).toHaveBeenCalledTimes(1);
      expect(logSessionError).toHaveBeenCalledWith(
        'GITHUB_SEARCH_REPOSITORIES',
        'TOOL_EXECUTION_FAILED'
      );
    });

    it('should not propagate errors to caller', async () => {
      const mockCallback: ToolInvocationCallback = vi
        .fn()
        .mockRejectedValue(new Error('Should not propagate'));

      await invokeCallbackSafely(
        mockCallback,
        'GITHUB_VIEW_REPO_STRUCTURE',
        []
      );

      expect(logSessionError).toHaveBeenCalled();
    });

    it('should handle callback that returns rejected promise', async () => {
      const mockCallback: ToolInvocationCallback = vi
        .fn()
        .mockImplementation(async () => {
          return Promise.reject(new Error('Promise rejection'));
        });

      await expect(
        invokeCallbackSafely(mockCallback, 'GITHUB_SEARCH_PULL_REQUESTS', [])
      ).resolves.toBeUndefined();

      expect(logSessionError).toHaveBeenCalledWith(
        'GITHUB_SEARCH_PULL_REQUESTS',
        'TOOL_EXECUTION_FAILED'
      );
    });

    it('should handle non-Error thrown values', async () => {
      const mockCallback: ToolInvocationCallback = vi
        .fn()
        .mockImplementation(() => {
          throw 'string error';
        });

      await expect(
        invokeCallbackSafely(mockCallback, 'PACKAGE_SEARCH', [])
      ).resolves.toBeUndefined();

      expect(logSessionError).toHaveBeenCalledWith(
        'PACKAGE_SEARCH',
        'TOOL_EXECUTION_FAILED'
      );
    });

    it('should work with all tool types', async () => {
      const toolNames = [
        'GITHUB_SEARCH_CODE',
        'GITHUB_FETCH_CONTENT',
        'GITHUB_SEARCH_REPOSITORIES',
        'GITHUB_SEARCH_PULL_REQUESTS',
        'GITHUB_VIEW_REPO_STRUCTURE',
        'PACKAGE_SEARCH',
      ];

      for (const toolName of toolNames) {
        vi.clearAllMocks();
        const mockCallback: ToolInvocationCallback = vi
          .fn()
          .mockResolvedValue(undefined);

        await invokeCallbackSafely(mockCallback, toolName, [{ test: true }]);

        expect(mockCallback).toHaveBeenCalledWith(toolName, [{ test: true }]);
        expect(logSessionError).not.toHaveBeenCalled();
      }
    });
  });
});
