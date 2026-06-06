import { describe, it, expect } from 'vitest';
import {
  isProviderSuccess,
  isProviderError,
  type ProviderResponse,
  type ProviderType,
  type CodeSearchResult,
  type FileContentResult,
} from '../../src/providers/types.js';

describe('Provider Types', () => {
  describe('isProviderSuccess', () => {
    it('should return true for a successful response with data', () => {
      const response: ProviderResponse<string> = {
        data: 'test data',
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return true for successful response with complex data', () => {
      const codeSearchResult: CodeSearchResult = {
        items: [
          {
            path: 'src/index.ts',
            matches: [{ context: 'test context', positions: [[0, 5]] }],
            url: 'https://github.com/owner/repo/blob/main/src/index.ts',
            repository: {
              id: '123',
              name: 'repo',
              url: 'https://github.com/owner/repo',
            },
          },
        ],
        totalCount: 1,
        pagination: {
          currentPage: 1,
          totalPages: 1,
          hasMore: false,
        },
      };

      const response: ProviderResponse<CodeSearchResult> = {
        data: codeSearchResult,
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return false for a response with error', () => {
      const response: ProviderResponse<string> = {
        error: 'Something went wrong',
        status: 500,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(false);
    });

    it('should return false for a response with both data and error', () => {
      const response: ProviderResponse<string> = {
        data: 'partial data',
        error: 'Something went wrong',
        status: 500,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(false);
    });

    it('should return false for a response with undefined data', () => {
      const response: ProviderResponse<string> = {
        data: undefined,
        status: 404,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(false);
    });

    it('should return false for a response without data property', () => {
      const response: ProviderResponse<string> = {
        status: 404,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(false);
    });

    it('should return true for response with data as empty string', () => {
      const response: ProviderResponse<string> = {
        data: '',
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return true for response with data as 0', () => {
      const response: ProviderResponse<number> = {
        data: 0,
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return true for response with data as false', () => {
      const response: ProviderResponse<boolean> = {
        data: false,
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return true for response with data as null', () => {
      const response: ProviderResponse<null> = {
        data: null,
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return true for response with data as empty array', () => {
      const response: ProviderResponse<string[]> = {
        data: [],
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return true for response with data as empty object', () => {
      const response: ProviderResponse<object> = {
        data: {},
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return true for response with additional hints', () => {
      const response: ProviderResponse<string> = {
        data: 'test data',
        status: 200,
        provider: 'github',
        hints: ['Hint 1', 'Hint 2'],
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return true for response with rateLimit info', () => {
      const response: ProviderResponse<string> = {
        data: 'test data',
        status: 200,
        provider: 'github',
        rateLimit: {
          remaining: 100,
          reset: 1704067200,
          retryAfter: 60,
        },
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should return false for response with error and rateLimit', () => {
      const response: ProviderResponse<string> = {
        error: 'Rate limit exceeded',
        status: 429,
        provider: 'github',
        rateLimit: {
          remaining: 0,
          reset: 1704067200,
          retryAfter: 3600,
        },
      };

      expect(isProviderSuccess(response)).toBe(false);
    });

    it('should handle FileContentResult data type', () => {
      const fileContent: FileContentResult = {
        path: 'src/index.ts',
        content: 'console.log("hello");',
        encoding: 'utf-8',
        size: 21,
        ref: 'main',
        lastModified: '2024-01-01T00:00:00Z',
      };

      const response: ProviderResponse<FileContentResult> = {
        data: fileContent,
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });
  });

  describe('isProviderError', () => {
    it('should return true for a response with error', () => {
      const response: ProviderResponse<string> = {
        error: 'Something went wrong',
        status: 500,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return true for response with both data and error', () => {
      const response: ProviderResponse<string> = {
        data: 'partial data',
        error: 'Something went wrong',
        status: 500,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return false for a successful response', () => {
      const response: ProviderResponse<string> = {
        data: 'test data',
        status: 200,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(false);
    });

    it('should return false for response without error property', () => {
      const response: ProviderResponse<string> = {
        status: 200,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(false);
    });

    it('should return false for response with undefined error', () => {
      const response: ProviderResponse<string> = {
        error: undefined,
        status: 200,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(false);
    });

    it('should return true for response with empty string error', () => {
      const response: ProviderResponse<string> = {
        error: '',
        status: 400,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return true for 4xx client errors', () => {
      const response: ProviderResponse<string> = {
        error: 'Not found',
        status: 404,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return true for 401 unauthorized', () => {
      const response: ProviderResponse<string> = {
        error: 'Unauthorized',
        status: 401,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return true for 403 forbidden', () => {
      const response: ProviderResponse<string> = {
        error: 'Forbidden',
        status: 403,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return true for 429 rate limit', () => {
      const response: ProviderResponse<string> = {
        error: 'Rate limit exceeded',
        status: 429,
        provider: 'github',
        rateLimit: {
          remaining: 0,
          reset: 1704067200,
        },
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return true for 500 internal server error', () => {
      const response: ProviderResponse<string> = {
        error: 'Internal server error',
        status: 500,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return true for 502 bad gateway', () => {
      const response: ProviderResponse<string> = {
        error: 'Bad gateway',
        status: 502,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return true for 503 service unavailable', () => {
      const response: ProviderResponse<string> = {
        error: 'Service unavailable',
        status: 503,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should return true for error with hints', () => {
      const response: ProviderResponse<string> = {
        error: 'Repository not found',
        status: 404,
        provider: 'github',
        hints: ['Check if the repository exists', 'Verify you have access'],
      };

      expect(isProviderError(response)).toBe(true);
    });
  });

  describe('isProviderSuccess and isProviderError interaction', () => {
    it('should be mutually exclusive for clean success response', () => {
      const response: ProviderResponse<string> = {
        data: 'test data',
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
      expect(isProviderError(response)).toBe(false);
    });

    it('should be mutually exclusive for clean error response', () => {
      const response: ProviderResponse<string> = {
        error: 'Something went wrong',
        status: 500,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(false);
      expect(isProviderError(response)).toBe(true);
    });

    it('should handle ambiguous response with both data and error', () => {
      const response: ProviderResponse<string> = {
        data: 'partial data',
        error: 'Partial failure',
        status: 206,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(false);
      expect(isProviderError(response)).toBe(true);
    });

    it('should handle response with neither data nor error', () => {
      const response: ProviderResponse<string> = {
        status: 204,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(false);
      expect(isProviderError(response)).toBe(false);
    });
  });

  describe('ProviderType type validation', () => {
    it('should accept github as valid provider', () => {
      const provider: ProviderType = 'github';
      const response: ProviderResponse<string> = {
        data: 'test',
        status: 200,
        provider,
      };

      expect(isProviderSuccess(response)).toBe(true);
    });
  });

  describe('ProviderResponse with various data types', () => {
    it('should work with string data', () => {
      const response: ProviderResponse<string> = {
        data: 'hello world',
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
      expect(isProviderError(response)).toBe(false);
    });

    it('should work with number data', () => {
      const response: ProviderResponse<number> = {
        data: 42,
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
      expect(isProviderError(response)).toBe(false);
    });

    it('should work with boolean data', () => {
      const response: ProviderResponse<boolean> = {
        data: true,
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
      expect(isProviderError(response)).toBe(false);
    });

    it('should work with array data', () => {
      const response: ProviderResponse<number[]> = {
        data: [1, 2, 3],
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
      expect(isProviderError(response)).toBe(false);
    });

    it('should work with nested object data', () => {
      interface NestedData {
        level1: {
          level2: {
            value: string;
          };
        };
      }

      const response: ProviderResponse<NestedData> = {
        data: {
          level1: {
            level2: {
              value: 'deep value',
            },
          },
        },
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
      expect(isProviderError(response)).toBe(false);
    });

    it('should work with union type data', () => {
      const response: ProviderResponse<string | number> = {
        data: 'string value',
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should work with optional properties in data', () => {
      interface DataWithOptionals {
        required: string;
        optional?: number;
      }

      const response: ProviderResponse<DataWithOptionals> = {
        data: {
          required: 'value',
        },
        status: 200,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });
  });

  describe('Edge cases for type guard functions', () => {
    it('should handle response with all optional fields populated', () => {
      const response: ProviderResponse<string> = {
        data: 'test data',
        status: 200,
        provider: 'github',
        hints: ['hint1', 'hint2'],
        rateLimit: {
          remaining: 4999,
          reset: 1704067200,
          retryAfter: undefined,
        },
      };

      expect(isProviderSuccess(response)).toBe(true);
      expect(isProviderError(response)).toBe(false);
    });

    it('should handle response with empty hints array', () => {
      const response: ProviderResponse<string> = {
        data: 'test data',
        status: 200,
        provider: 'github',
        hints: [],
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should handle response with zero rate limit remaining', () => {
      const response: ProviderResponse<string> = {
        data: 'test data',
        status: 200,
        provider: 'github',
        rateLimit: {
          remaining: 0,
          reset: 1704067200,
        },
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should handle error response with zero rate limit', () => {
      const response: ProviderResponse<string> = {
        error: 'Rate limited',
        status: 429,
        provider: 'github',
        rateLimit: {
          remaining: 0,
          reset: 1704067200,
          retryAfter: 3600,
        },
      };

      expect(isProviderError(response)).toBe(true);
      expect(isProviderSuccess(response)).toBe(false);
    });

    it('should handle status code 0', () => {
      const response: ProviderResponse<string> = {
        error: 'Network error',
        status: 0,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should handle negative status code', () => {
      const response: ProviderResponse<string> = {
        error: 'Unknown error',
        status: -1,
        provider: 'github',
      };

      expect(isProviderError(response)).toBe(true);
    });

    it('should handle 3xx redirect status with data', () => {
      const response: ProviderResponse<string> = {
        data: 'redirect info',
        status: 301,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should handle 1xx informational status with data', () => {
      const response: ProviderResponse<string> = {
        data: 'continuing',
        status: 100,
        provider: 'github',
      };

      expect(isProviderSuccess(response)).toBe(true);
    });

    it('should correctly type narrow on success', () => {
      const response: ProviderResponse<string> = {
        data: 'test data',
        status: 200,
        provider: 'github',
      };

      if (isProviderSuccess(response)) {
        const data: string = response.data;
        expect(data).toBe('test data');
      }
    });

    it('should correctly type narrow on error', () => {
      const response: ProviderResponse<string> = {
        error: 'test error',
        status: 500,
        provider: 'github',
      };

      if (isProviderError(response)) {
        const error: string = response.error;
        expect(error).toBe('test error');
      }
    });
  });
});
