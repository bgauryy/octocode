import { describe, it, expect } from 'vitest';
import { applyStructurePagination } from '../../../octocode-tools-core/src/github/repoStructurePagination.js';
import type { GitHubRepositoryStructureResult } from '../../../octocode-tools-core/src/tools/github_view_repo_structure/types.js';
import type { GitHubViewRepoStructureQuery } from '../../src/public.js';

function makeQuery(
  overrides: Partial<GitHubViewRepoStructureQuery> = {}
): GitHubViewRepoStructureQuery {
  return {
    id: 'q1',
    mainResearchGoal: 'unit test',
    researchGoal: 'verify pagination',
    reasoning: 'cover branches',
    owner: 'octo',
    repo: 'repo',
    path: '',
    maxDepth: 1,
    itemsPerPage: 50,
    page: 1,
    ...overrides,
  };
}

function makeCached(
  overrides: Partial<GitHubRepositoryStructureResult>
): GitHubRepositoryStructureResult {
  return {
    owner: 'octo',
    repo: 'repo',
    branch: 'main',
    path: '/',
    apiSource: true,
    summary: {
      totalFiles: 0,
      totalFolders: 0,
      truncated: false,
      filtered: false,
      originalCount: 0,
    },
    structure: {},
    ...overrides,
  };
}

describe('applyStructurePagination — direct unit tests', () => {
  describe('empty cache', () => {
    it('strips _cachedItems and returns the original result when no items are cached', () => {
      const cached = makeCached({
        _cachedItems: undefined,
        structure: { '.': { files: ['a.ts'], folders: [] } },
        summary: {
          totalFiles: 1,
          totalFolders: 0,
          truncated: false,
          filtered: false,
          originalCount: 1,
        },
      });
      const result = applyStructurePagination(cached, makeQuery());
      expect('_cachedItems' in result).toBe(false);
      expect(result.structure).toEqual({
        '.': { files: ['a.ts'], folders: [] },
      });
    });

    it('strips _cachedItems when the cached array is empty', () => {
      const cached = makeCached({ _cachedItems: [] });
      const result = applyStructurePagination(cached, makeQuery());
      expect('_cachedItems' in result).toBe(false);
    });
  });

  describe('root path (path === "/")', () => {
    it('places root-level files under "." and sorts them lexicographically', () => {
      const cached = makeCached({
        _cachedItems: [
          { path: 'b.ts', type: 'file' },
          { path: 'a.ts', type: 'file' },
          { path: 'docs', type: 'dir' },
        ],
      });
      const result = applyStructurePagination(
        cached,
        makeQuery({ itemsPerPage: 10 })
      );
      expect(result.structure['.']).toEqual({
        files: ['a.ts', 'b.ts'],
        folders: ['docs'],
      });
      expect(result.pagination).toEqual({
        currentPage: 1,
        totalPages: 1,
        hasMore: false,
        entriesPerPage: 10,
        totalEntries: 3,
      });
      expect(result.summary).toEqual({
        totalFiles: 2,
        totalFolders: 1,
        truncated: false,
        filtered: true,
        originalCount: 3,
      });
    });
  });

  describe('nested basePath', () => {
    it('strips basePath prefix and groups items by their relative parent dir', () => {
      const cached = makeCached({
        path: 'src',
        _cachedItems: [
          { path: 'src/index.ts', type: 'file' },
          { path: 'src/utils/helpers.ts', type: 'file' },
          { path: 'src/utils', type: 'dir' },
          { path: 'src/components/Button.tsx', type: 'file' },
        ],
      });
      const result = applyStructurePagination(
        cached,
        makeQuery({ itemsPerPage: 10 })
      );
      expect(result.structure['.']).toEqual({
        files: ['index.ts'],
        folders: ['utils'],
      });
      expect(result.structure['utils']).toEqual({
        files: ['helpers.ts'],
        folders: [],
      });
      expect(result.structure['components']).toEqual({
        files: ['Button.tsx'],
        folders: [],
      });
    });

    it('does not strip basePath when the item path does not start with it', () => {
      const cached = makeCached({
        path: 'src',
        _cachedItems: [
          { path: 'README.md', type: 'file' },
          { path: 'src/index.ts', type: 'file' },
        ],
      });
      const result = applyStructurePagination(
        cached,
        makeQuery({ itemsPerPage: 10 })
      );
      expect(result.structure['.']?.files).toContain('README.md');
      expect(result.structure['.']?.files).toContain('index.ts');
    });
  });

  describe('sort order', () => {
    it('places "." first regardless of other directory names', () => {
      const cached = makeCached({
        _cachedItems: [
          { path: 'zeta/x.ts', type: 'file' },
          { path: 'alpha/y.ts', type: 'file' },
          { path: 'README.md', type: 'file' },
        ],
      });
      const result = applyStructurePagination(
        cached,
        makeQuery({ itemsPerPage: 10 })
      );
      const keys = Object.keys(result.structure);
      expect(keys[0]).toBe('.');
      expect(keys.slice(1)).toEqual(['alpha', 'zeta']);
    });
  });

  describe('multi-page slicing', () => {
    const items = Array.from({ length: 25 }, (_, i) => ({
      path: `f${String(i).padStart(2, '0')}.ts`,
      type: 'file' as const,
    }));

    it('returns the first slice on page 1 with hasMore=true', () => {
      const cached = makeCached({ _cachedItems: items });
      const result = applyStructurePagination(
        cached,
        makeQuery({ itemsPerPage: 10, page: 1 })
      );
      expect(result.structure['.']?.files.length).toBe(10);
      expect(result.pagination?.hasMore).toBe(true);
      expect(result.summary.truncated).toBe(true);
    });

    it('returns the middle slice on a middle page', () => {
      const cached = makeCached({ _cachedItems: items });
      const result = applyStructurePagination(
        cached,
        makeQuery({ itemsPerPage: 10, page: 2 })
      );
      expect(result.structure['.']?.files.length).toBe(10);
      expect(result.pagination?.hasMore).toBe(true);
    });

    it('returns the partial last slice and hasMore=false', () => {
      const cached = makeCached({ _cachedItems: items });
      const result = applyStructurePagination(
        cached,
        makeQuery({ itemsPerPage: 10, page: 3 })
      );
      expect(result.structure['.']?.files.length).toBe(5);
      expect(result.pagination?.hasMore).toBe(false);
      expect(result.summary.truncated).toBe(false);
    });

    it('returns an empty page when page is beyond totalPages', () => {
      const cached = makeCached({ _cachedItems: items });
      const result = applyStructurePagination(
        cached,
        makeQuery({ itemsPerPage: 10, page: 99 })
      );
      expect(Object.keys(result.structure).length).toBe(0);
      expect(result.pagination?.hasMore).toBe(false);
    });
  });
});
