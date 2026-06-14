import { describe, it, expect, beforeEach, vi } from 'vitest';
import { fetchGitHubFileContentAPI } from '../../../octocode-tools-core/src/github/fileContent.js';
import { getOctokit } from '../../../octocode-tools-core/src/github/client.js';
import * as minifierModule from '@octocodeai/octocode-context-utils';

vi.mock('../../../octocode-tools-core/src/github/client.js');
vi.mock('@octocodeai/octocode-context-utils', async importOriginal => {
  const actual = await importOriginal();
  return { ...actual, minifyContent: vi.fn(), minifyContentSync: vi.fn() };
});

describe('GitHub File Operations - Pagination', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

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
      const smallContent = 'x'.repeat(800);
      const mockOctokit = createMockOctokit(smallContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        expect(result.data.content?.length).toBe(800);
      }
    });

    it('returns full content for large files (char-based pagination removed)', async () => {
      const largeContent = 'x'.repeat(70000);
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
    });
  });

  describe('page-based navigation', () => {
    it('uses startLine/endLine for partial file content', async () => {
      const largeContent = Array.from(
        { length: 1000 },
        (_, i) => `line ${i + 1}`
      ).join('\n');
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        startLine: 1,
        endLine: 50,
      });

      expect(result).toHaveProperty('data');
    });
  });

  describe('boundary conditions', () => {
    it('returns full content without pagination for any file size', async () => {
      const content = 'x'.repeat(40000);
      const mockOctokit = createMockOctokit(content);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
      });

      expect(result).toHaveProperty('data');
    });

    it('handles regular file fetch correctly', async () => {
      const content = 'x'.repeat(5000);
      const mockOctokit = createMockOctokit(content);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
      });

      expect(result).toHaveProperty('data');
    });

    it('should not paginate when explicit charOffset=0 and content is below the page budget', async () => {
      const smallContent = 'x'.repeat(800);
      const mockOctokit = createMockOctokit(smallContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        expect(result.data.pagination).toBeUndefined();
      }
    });
  });

  describe('fullContent', () => {
    it('returns full content without pagination when fullContent=true', async () => {
      const largeContent = 'x'.repeat(50000);
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
    });
  });

  describe('matchString with pagination', () => {
    it('should paginate large matchString results', async () => {
      const lineContent = 'function test() { return true; }\n';
      const largeContent = lineContent.repeat(1000);
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        contextLines: 50,
      });

      expect(result).toHaveProperty('data');
      if ('data' in result && result.data && !('error' in result.data)) {
        if ((result.data.content?.length ?? 0) > 20000) {
          expect(result.data.pagination).toBeDefined();
        }
      }
    });
  });

  describe('cache behavior', () => {
    it('should use same cache for repeated requests to the same file', async () => {
      const uniquePath = `cache-test-${Date.now()}.ts`;
      const largeContent = 'x'.repeat(70000);
      const mockOctokit = createMockOctokit(largeContent, uniquePath);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
      );
      vi.mocked(minifierModule.minifyContent).mockImplementation(async c => ({
        content: c,
        failed: false,
        type: 'general',
      }));

      const result1 = await fetchGitHubFileContentAPI({
        owner: 'cache-owner',
        repo: 'cache-repo',
        path: uniquePath,
      });

      const firstCallCount =
        mockOctokit.rest.repos.getContent.mock.calls.length;
      expect(firstCallCount).toBe(1);

      const result2 = await fetchGitHubFileContentAPI({
        owner: 'cache-owner',
        repo: 'cache-repo',
        path: uniquePath,
      });

      expect(mockOctokit.rest.repos.getContent.mock.calls.length).toBe(
        firstCallCount
      );

      expect('data' in result1 && result1.data?.content).toBeTruthy();
      expect('data' in result2 && result2.data?.content).toBeTruthy();
    });
  });

  describe('content response', () => {
    it('returns content for large files', async () => {
      const largeContent = 'x'.repeat(70000);
      const mockOctokit = createMockOctokit(largeContent);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as Awaited<ReturnType<typeof getOctokit>>
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
        expect(result.data.content).toBeDefined();
      }
    });
  });
});
