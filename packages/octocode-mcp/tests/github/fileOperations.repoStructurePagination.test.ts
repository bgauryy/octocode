import { describe, it, expect, beforeEach, vi } from 'vitest';
import { viewGitHubRepositoryStructureAPI } from '../../src/github/repoStructure.js';
import { getOctokit } from '../../src/github/client.js';
import { clearAllCache } from '../../src/utils/http/cache.js';
import { GITHUB_STRUCTURE_DEFAULTS } from '../../src/tools/github_view_repo_structure/constants.js';

vi.mock('../../src/github/client.js');
vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('GitHub Repository Structure - Pagination', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearAllCache();
  });

  function createMockFiles(count: number, prefix: string = 'file') {
    return Array.from({ length: count }, (_, i) => ({
      name: `${prefix}${i}.ts`,
      path: `${prefix}${i}.ts`,
      type: 'file' as const,
      size: 100,
      url: `https://api.github.com/repos/test/repo/contents/${prefix}${i}.ts`,
      html_url: `https://github.com/test/repo/blob/main/${prefix}${i}.ts`,
      git_url: `https://api.github.com/repos/test/repo/git/blobs/${i}`,
      sha: `sha${i}`,
    }));
  }

  function createMockOctokit(items: unknown[]) {
    return {
      rest: {
        repos: {
          get: vi.fn().mockResolvedValue({
            data: { default_branch: 'main' },
          }),
          getContent: vi.fn().mockResolvedValue({
            data: items,
          }),
        },
      },
    };
  }

  describe('pagination info', () => {
    it('should include pagination info when results fit on one page', async () => {
      const files = createMockFiles(10);
      const mockOctokit = createMockOctokit(files);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.pagination).toBeDefined();
        expect(result.pagination?.currentPage).toBe(1);
        expect(result.pagination?.totalPages).toBe(1);
        expect(result.pagination?.hasMore).toBe(false);
        expect(result.pagination?.entriesPerPage).toBe(
          GITHUB_STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE
        );
        expect(result.pagination?.totalEntries).toBe(10);
      }
    });

    it('should paginate results when exceeding entriesPerPage', async () => {
      const files = createMockFiles(100);
      const mockOctokit = createMockOctokit(files);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        entriesPerPage: 20,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.pagination).toBeDefined();
        expect(result.pagination?.currentPage).toBe(1);
        expect(result.pagination?.totalPages).toBe(5);
        expect(result.pagination?.hasMore).toBe(true);
        expect(result.pagination?.entriesPerPage).toBe(20);
        expect(result.pagination?.totalEntries).toBe(100);

        expect(result.structure['.']!.files.length).toBe(20);
      }
    });

    it('should return correct page when entryPageNumber is specified', async () => {
      const files = createMockFiles(100);
      const mockOctokit = createMockOctokit(files);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        entriesPerPage: 20,
        entryPageNumber: 3,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.pagination?.currentPage).toBe(3);
        expect(result.pagination?.totalPages).toBe(5);
        expect(result.pagination?.hasMore).toBe(true);

        expect(result.structure['.']!.files.length).toBe(20);
      }
    });

    it('should handle last page correctly', async () => {
      const files = createMockFiles(95);
      const mockOctokit = createMockOctokit(files);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        entriesPerPage: 20,
        entryPageNumber: 5,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.pagination?.currentPage).toBe(5);
        expect(result.pagination?.totalPages).toBe(5);
        expect(result.pagination?.hasMore).toBe(false);

        expect(result.structure['.']!.files.length).toBe(15);
      }
    });

    it('should use default entriesPerPage when not specified', async () => {
      const files = createMockFiles(100);
      const mockOctokit = createMockOctokit(files);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.pagination?.entriesPerPage).toBe(
          GITHUB_STRUCTURE_DEFAULTS.ENTRIES_PER_PAGE
        );
      }
    });
  });

  describe('summary truncation info', () => {
    it('should set truncated=true when more pages exist', async () => {
      const files = createMockFiles(100);
      const mockOctokit = createMockOctokit(files);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        entriesPerPage: 20,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.summary.truncated).toBe(true);
        expect(result.summary.originalCount).toBe(100);
      }
    });

    it('should set truncated=false when all results are shown', async () => {
      const files = createMockFiles(10);
      const mockOctokit = createMockOctokit(files);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.summary.truncated).toBe(false);
      }
    });
  });

  describe('edge cases', () => {
    it('should handle page number beyond total pages', async () => {
      const files = createMockFiles(50);
      const mockOctokit = createMockOctokit(files);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        entriesPerPage: 20,
        entryPageNumber: 10,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.structure['.']?.files?.length ?? 0).toBe(0);
        expect(result.pagination?.hasMore).toBe(false);
      }
    });

    it('should handle empty directory', async () => {
      const mockOctokit = createMockOctokit([]);

      vi.mocked(getOctokit).mockResolvedValue(
        mockOctokit as unknown as ReturnType<typeof getOctokit>
      );

      const result = await viewGitHubRepositoryStructureAPI({
        owner: 'test',
        repo: 'repo',
        branch: 'main',
        entriesPerPage: 20,
      });

      expect('structure' in result).toBe(true);
      if ('structure' in result) {
        expect(result.pagination?.totalEntries).toBe(0);
        expect(result.pagination?.totalPages).toBe(1);
        expect(result.pagination?.hasMore).toBe(false);
      }
    });
  });
});
