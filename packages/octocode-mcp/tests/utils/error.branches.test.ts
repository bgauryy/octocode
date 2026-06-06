import { describe, it, expect, vi } from 'vitest';

vi.mock('../../src/hints/index.js', () => ({
  getHints: vi.fn(() => ['mock-hint']),
}));

import { createErrorResult } from '../../src/utils/response/error.js';

const baseQuery = {
  researchGoal: 'test',
  reasoning: 'test',
};

describe('createErrorResult - branch coverage', () => {
  describe('isGitHubApiError detection', () => {
    it('should detect GitHubAPIError with "type" field', () => {
      const apiError = { error: 'Not Found', type: 'NOT_FOUND' };
      const result = createErrorResult(apiError, baseQuery);
      expect(result.error).toBe(apiError);
      expect(result.hints).toBeDefined();
      expect(result.hints!.some(h => h.includes('API Error'))).toBe(true);
    });

    it('should detect GitHubAPIError with "status" field', () => {
      const apiError = { error: 'Rate limited', status: 429 };
      const result = createErrorResult(apiError, baseQuery);
      expect(result.error).toBe(apiError);
    });

    it('should detect GitHubAPIError with "scopesSuggestion" field', () => {
      const apiError = {
        error: 'Forbidden',
        scopesSuggestion: 'Add repo scope',
      };
      const result = createErrorResult(apiError, baseQuery);
      expect(result.error).toBe(apiError);
      expect(result.hints!.some(h => h.includes('Add repo scope'))).toBe(true);
    });

    it('should skip GitHub hints extraction when hintSourceError is provided', () => {
      const apiError = { error: 'Forbidden', type: 'FORBIDDEN' };
      const hintSourceError = {
        error: 'Rate limit exceeded',
        type: 'http' as const,
        rateLimitRemaining: 0,
        rateLimitReset: Date.now() + 60000,
      };
      const result = createErrorResult(apiError, baseQuery, {
        hintSourceError,
      });
      expect(result.error).toBe(apiError);
      expect(result.hints!.some(h => h.includes('Rate limit:'))).toBe(true);
      const githubErrors = result.hints!.filter(h => h.includes('API Error'));
      expect(githubErrors).toHaveLength(1);
    });
  });

  describe('unknown error type (else branch)', () => {
    it('should handle non-Error non-string non-object error', () => {
      const result = createErrorResult(42, baseQuery);
      expect(result.error).toBe('Unknown error occurred');
    });

    it('should handle null error', () => {
      const result = createErrorResult(null, baseQuery);
      expect(result.error).toBe('Unknown error occurred');
    });

    it('should handle boolean error', () => {
      const result = createErrorResult(false, baseQuery);
      expect(result.error).toBe('Unknown error occurred');
    });
  });

  describe('extra.hints merging', () => {
    it('should merge hints from extra.hints array', () => {
      const result = createErrorResult('some error', baseQuery, {
        extra: { hints: ['extra-hint-1', 'extra-hint-2'] },
      });
      expect(result.hints).toBeDefined();
      expect(result.hints!.includes('extra-hint-1')).toBe(true);
      expect(result.hints!.includes('extra-hint-2')).toBe(true);
    });

    it('should ignore extra.hints when not an array', () => {
      const result = createErrorResult('some error', baseQuery, {
        extra: { hints: 'not-an-array' as any },
      });
      expect(result.hints).toBeUndefined();
    });

    it('should spread extra fields into result (excluding hints)', () => {
      const result = createErrorResult('some error', baseQuery, {
        extra: { cwd: '/test', resolvedPath: '/test/file.ts', hints: [] },
      });
      expect(result.cwd).toBe('/test');
      expect(result.resolvedPath).toBe('/test/file.ts');
    });
  });

  describe('hintSourceError path', () => {
    it('should extract hints from hintSourceError with retryAfter', () => {
      const hintSourceError = {
        error: 'Secondary Rate limit',
        type: 'http' as const,
        retryAfter: 60,
      };
      const result = createErrorResult('main error', baseQuery, {
        hintSourceError,
      });
      expect(
        result.hints!.some(h => h.includes('Retry after 60 seconds'))
      ).toBe(true);
    });
  });

  describe('Error instance handling', () => {
    it('should convert plain Error to ToolError with hints', () => {
      const error = new Error('Something failed');
      const result = createErrorResult(error, baseQuery, {
        toolName: 'LOCAL_FETCH_CONTENT',
      });
      expect(result.error).toBe('Something failed');
      expect(result.errorCode).toBeDefined();
    });
  });
});
