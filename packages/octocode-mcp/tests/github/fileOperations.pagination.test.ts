import { describe, it, expect, beforeEach, vi } from 'vitest';
import { fetchGitHubFileContentAPI } from '../../src/github/fileContent.js';
import { getOctokit } from '../../src/github/client.js';
import * as minifierModule from '../../src/utils/minifier/minifier.js';

// Mock dependencies
vi.mock('../../src/github/client.js');
vi.mock('../../src/utils/minifier/minifier.js');

const DEFAULT_OUTPUT_CHAR_LENGTH = 8000;

describe('GitHub File Operations - Pagination', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  /**
   * Helper to create mock Octokit with file content
   */
  function createMockOctokit(content: string, filename = 'test.ts') {
    return {
      rest: {
        repos: {
          getContent: vi.fn().mockResolvedValue({
            data: {
              type: 'file',
              content: Buffer.from(content).toString('base64'),
              size: content.length,
              sha: 'abc123',
              name: filename,
              path: filename,
            },
          }),
          listCommits: vi.fn().mockResolvedValue({ data: [] }),
        },
      },
    };
  }

  describe('auto-pagination', () => {
    it('should NOT paginate small files below the shared output budget', async () => {
      const smallContent = 'x'.repeat(5000); // 5K chars
      const mockOctokit = createMockOctokit(smallContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'small.ts',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        expect(result.data.pagination).toBeUndefined();
        expect(result.data.content?.length).toBe(5000);
      }
    });

    it('should auto-paginate large files exceeding the shared output budget', async () => {
      const largeContent = 'x'.repeat(70000); // 70K chars
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'large.ts',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        expect(result.data.pagination).toBeDefined();
        expect(result.data.pagination?.currentPage).toBe(1);
        expect(result.data.pagination?.hasMore).toBe(true);
        expect(result.data.pagination?.charOffset).toBe(0);
        expect(result.data.pagination?.charLength).toBe(
          DEFAULT_OUTPUT_CHAR_LENGTH
        );
        expect(result.data.pagination?.totalChars).toBe(70000);
        expect(result.data.pagination?.totalPages).toBe(9);
      }
    });

    it('should return page 2 with charOffset=8000', async () => {
      const largeContent = 'x'.repeat(70000);
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'large.ts',
        charOffset: DEFAULT_OUTPUT_CHAR_LENGTH,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        expect(result.data.pagination?.currentPage).toBe(2);
        expect(result.data.pagination?.charOffset).toBe(
          DEFAULT_OUTPUT_CHAR_LENGTH
        );
        expect(result.data.pagination?.hasMore).toBe(true);
      }
    });

    it('should indicate last page correctly (charOffset=64000)', async () => {
      const largeContent = 'x'.repeat(70000);
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'large.ts',
        charOffset: DEFAULT_OUTPUT_CHAR_LENGTH * 8,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        expect(result.data.pagination?.currentPage).toBe(9);
        expect(result.data.pagination?.hasMore).toBe(false);
        expect(result.data.pagination?.charLength).toBe(6000); // Remaining
      }
    });
  });

  describe('custom page size', () => {
    it('should respect charLength parameter', async () => {
      const largeContent = 'x'.repeat(70000);
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'large.ts',
        charLength: 10000,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        expect(result.data.pagination?.totalPages).toBe(7); // 70K / 10K
      }
    });
  });

  describe('boundary conditions', () => {
    it('should handle charOffset at exactly content boundary', async () => {
      const content = 'x'.repeat(40000); // Exactly 5 pages at the shared default
      const mockOctokit = createMockOctokit(content);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(async c => ({
        content: c,
        failed: false,
        type: 'general',
      }));

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'boundary.ts',
        charOffset: DEFAULT_OUTPUT_CHAR_LENGTH * 4,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        expect(result.data.pagination?.currentPage).toBe(5);
        expect(result.data.pagination?.hasMore).toBe(false);
      }
    });

    it('should handle charOffset beyond content length', async () => {
      const content = 'x'.repeat(5000);
      const mockOctokit = createMockOctokit(content);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(async c => ({
        content: c,
        failed: false,
        type: 'general',
      }));

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'small.ts',
        charOffset: 999999,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        // Content should be empty or at end
        expect(result.data.pagination?.hasMore).toBe(false);
      }
    });

    it('should paginate when explicit charOffset=0 but content < threshold', async () => {
      // When charOffset is explicitly 0 but content is small, no pagination needed
      const smallContent = 'x'.repeat(5000);
      const mockOctokit = createMockOctokit(smallContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(async c => ({
        content: c,
        failed: false,
        type: 'general',
      }));

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'small.ts',
        charOffset: 0,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        // Should NOT paginate small content even with explicit charOffset=0
        expect(result.data.pagination).toBeUndefined();
      }
    });
  });

  describe('fullContent with pagination', () => {
    it('should auto-paginate fullContent=true on large files', async () => {
      const largeContent = 'x'.repeat(50000);
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(async c => ({
        content: c,
        failed: false,
        type: 'general',
      }));

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'large.ts',
        fullContent: true,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        expect(result.data.pagination).toBeDefined();
        expect(result.data.pagination?.currentPage).toBe(1);
        expect(result.data.pagination?.hasMore).toBe(true);
      }
    });
  });

  describe('matchString with pagination', () => {
    it('should paginate large matchString results', async () => {
      // Create content with many matches that results in > 20K chars
      const lineContent = 'function test() { return true; }\n';
      const largeContent = lineContent.repeat(1000); // ~33K chars
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(async c => ({
        content: c,
        failed: false,
        type: 'general',
      }));

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'functions.ts',
        matchString: 'function',
        matchStringContextLines: 50, // Large context to exceed threshold
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        // If the matched content exceeds threshold, it should be paginated
        if ((result.data.content?.length ?? 0) > 20000) {
          expect(result.data.pagination).toBeDefined();
        }
      }
    });
  });

  describe('cache behavior', () => {
    it('should use same cache for different pagination offsets', async () => {
      // Use unique path to avoid cache collision with other tests
      const uniquePath = `cache-test-${Date.now()}.ts`;
      const largeContent = 'x'.repeat(70000);
      const mockOctokit = createMockOctokit(largeContent, uniquePath);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(async c => ({
        content: c,
        failed: false,
        type: 'general',
      }));

      // First request - page 1
      const result1 = await fetchGitHubFileContentAPI({
        owner: 'cache-owner',
        repo: 'cache-repo',
        path: uniquePath,
      });

      const firstCallCount =
        mockOctokit.rest.repos.getContent.mock.calls.length;
      expect(firstCallCount).toBe(1); // First call should hit the API

      // Second request - page 2 (should hit cache)
      const result2 = await fetchGitHubFileContentAPI({
        owner: 'cache-owner',
        repo: 'cache-repo',
        path: uniquePath,
        charOffset: 20000,
      });

      // The second call should hit cache since charOffset is excluded from cache key
      expect(mockOctokit.rest.repos.getContent.mock.calls.length).toBe(
        firstCallCount
      );

      // Verify both results have content from the same source
      expect('data' in result1 && result1.data?.content).toBeTruthy();
      expect('data' in result2 && result2.data?.content).toBeTruthy();

      // Page 1 should start at offset 0, page 2 at offset 20000
      if ('data' in result1 && result1.data) {
        expect(result1.data.pagination?.charOffset).toBe(0);
      }
      if ('data' in result2 && result2.data) {
        expect(result2.data.pagination?.charOffset).toBe(20000);
      }
    });
  });

  describe('byte/character offset separation', () => {
    it('should return char offsets in pagination', async () => {
      const largeContent = 'x'.repeat(70000);
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(
        async content => ({
          content,
          failed: false,
          type: 'general',
        })
      );

      const result = await fetchGitHubFileContentAPI({
        owner: 'test',
        repo: 'repo',
        path: 'large.ts',
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        // Should have char fields
        expect(result.data.pagination?.charOffset).toBeDefined();
        expect(result.data.pagination?.charLength).toBeDefined();
        expect(result.data.pagination?.totalChars).toBeDefined();
      }
    });
  });
});
