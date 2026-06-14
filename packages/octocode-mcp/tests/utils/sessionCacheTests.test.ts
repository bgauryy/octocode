import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { generateCacheKey } from '../../../octocode-tools-core/src/utils/http/cache.js';

describe('Session-scoped Caching', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('generateCacheKey with session ID', () => {
    it('should generate different cache keys for different session IDs', () => {
      const params = { owner: 'test', repo: 'repo', query: 'test' };

      const key1 = generateCacheKey('gh-api-code', params, 'session1');
      const key2 = generateCacheKey('gh-api-code', params, 'session2');
      const key3 = generateCacheKey('gh-api-code', params);

      expect(key1).not.toBe(key2);
      expect(key1).not.toBe(key3);
      expect(key2).not.toBe(key3);
    });

    it('should generate same cache key for same session ID and params', () => {
      const params = { owner: 'test', repo: 'repo', query: 'test' };

      const key1 = generateCacheKey('gh-api-code', params, 'session1');
      const key2 = generateCacheKey('gh-api-code', params, 'session1');

      expect(key1).toBe(key2);
    });

    it('should handle undefined session ID gracefully', () => {
      const params = { owner: 'test', repo: 'repo', query: 'test' };

      const key1 = generateCacheKey('gh-api-code', params, undefined);
      const key2 = generateCacheKey('gh-api-code', params);

      expect(key1).toBe(key2);
    });

    it('should work with different prefixes', () => {
      const params = { owner: 'test', repo: 'repo' };
      const sessionId = 'test-session';

      const codeKey = generateCacheKey('gh-api-code', params, sessionId);
      const repoKey = generateCacheKey('gh-api-repos', params, sessionId);
      const fileKey = generateCacheKey(
        'gh-api-file-content',
        params,
        sessionId
      );

      expect(codeKey).not.toBe(repoKey);
      expect(codeKey).not.toBe(fileKey);
      expect(repoKey).not.toBe(fileKey);
    });

    it('should include session ID in hash generation', () => {
      const params = { simple: 'test' };

      const sessionKeys = [
        generateCacheKey('test', params, 'session-1'),
        generateCacheKey('test', params, 'session-2'),
        generateCacheKey('test', params, 'session-3'),
      ];

      const uniqueKeys = new Set(sessionKeys);
      expect(uniqueKeys.size).toBe(3);
    });
  });
});
