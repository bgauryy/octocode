import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  createSuccessResult,
  createErrorResult,
  handleProviderError,
  handleCatchError,
  invokeCallbackSafely,
} from '../../../octocode-tools-core/src/tools/utils.js';
import type { ToolInvocationCallback } from '../../../octocode-tools-core/src/types/toolResults.js';

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

    it('should accept toolName without changing the returned error', () => {
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

  describe('invokeCallbackSafely', () => {
    beforeEach(() => {
      vi.clearAllMocks();
    });

    it('should do nothing when callback is undefined', async () => {
      await expect(
        invokeCallbackSafely(undefined, 'TEST_TOOL', [{ query: 'test' }])
      ).resolves.toBeUndefined();
    });

    it('should invoke callback with correct arguments', async () => {
      const mockCallback: ToolInvocationCallback = vi
        .fn()
        .mockResolvedValue(undefined);
      const queries = [{ query: 'test1' }, { query: 'test2' }];

      await invokeCallbackSafely(mockCallback, 'GITHUB_SEARCH_CODE', queries);

      expect(mockCallback).toHaveBeenCalledTimes(1);
      expect(mockCallback).toHaveBeenCalledWith('GITHUB_SEARCH_CODE', queries);
    });

    it('should catch synchronous errors from callback', async () => {
      const mockCallback: ToolInvocationCallback = vi
        .fn()
        .mockImplementation(() => {
          throw new Error('Sync callback error');
        });

      await expect(
        invokeCallbackSafely(mockCallback, 'GITHUB_FETCH_CONTENT', [])
      ).resolves.toBeUndefined();

      expect(mockCallback).toHaveBeenCalledTimes(1);
    });

    it('should catch async rejected promises from callback', async () => {
      const mockCallback: ToolInvocationCallback = vi
        .fn()
        .mockRejectedValue(new Error('Async callback error'));

      await expect(
        invokeCallbackSafely(mockCallback, 'GITHUB_SEARCH_REPOSITORIES', [
          { name: 'test' },
        ])
      ).resolves.toBeUndefined();

      expect(mockCallback).toHaveBeenCalledTimes(1);
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
      }
    });
  });
});
