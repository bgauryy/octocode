import { describe, expect, it } from 'vitest';
import {
  buildStructureTree,
  buildFileSizeMap,
  buildStructureResult,
} from '../../src/github/repoStructure/resultBuilder.js';
import type { GitHubApiFileItem } from '../../src/tools/github_view_repo_structure/types.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function file(path: string, size = 100): GitHubApiFileItem {
  return {
    path,
    name: path.split('/').pop()!,
    type: 'file',
    size,
    sha: 'abc',
    url: '',
    html_url: '',
    git_url: '',
  };
}

function dir(path: string): GitHubApiFileItem {
  return {
    path,
    name: path.split('/').pop()!,
    type: 'dir',
    sha: 'abc',
    url: '',
    html_url: '',
    git_url: '',
  };
}

// ---------------------------------------------------------------------------
// buildStructureTree
// ---------------------------------------------------------------------------

describe('buildStructureTree', () => {
  it('places root files under the "." key', () => {
    const tree = buildStructureTree(
      [file('README.md'), file('package.json')],
      ''
    );
    expect(tree['.']).toBeDefined();
    expect(tree['.']!.files).toContain('README.md');
    expect(tree['.']!.files).toContain('package.json');
    expect(tree['.']!.folders).toHaveLength(0);
  });

  it('separates files from directories', () => {
    const items = [file('src/index.ts'), dir('src/lib')];
    const tree = buildStructureTree(items, '');
    expect(tree['src']!.files).toContain('index.ts');
    expect(tree['src']!.folders).toContain('lib');
  });

  it('strips the basePath prefix', () => {
    const items = [
      file('packages/core/src/index.ts'),
      dir('packages/core/src'),
    ];
    const tree = buildStructureTree(items, 'packages/core');
    expect(tree['src']!.files).toContain('index.ts');
  });

  it('handles basePath with trailing slash', () => {
    const items = [file('packages/core/index.ts')];
    const tree = buildStructureTree(items, 'packages/core/');
    // basePath has trailing slash but code checks startsWith then slices
    expect(Object.keys(tree).length).toBeGreaterThan(0);
  });

  it('places items with no slash parent under "."', () => {
    const tree = buildStructureTree([file('main.ts')], '');
    expect(tree['.']!.files).toContain('main.ts');
  });

  it('sorts files and folders alphabetically', () => {
    const items = [file('src/z.ts'), file('src/a.ts'), dir('src/m')];
    const tree = buildStructureTree(items, '');
    expect(tree['src']!.files).toEqual(['a.ts', 'z.ts']);
  });

  it('sorts "." key first in returned object', () => {
    const items = [file('src/a.ts'), file('README.md')];
    const tree = buildStructureTree(items, '');
    const keys = Object.keys(tree);
    expect(keys[0]).toBe('.');
  });

  it('returns an empty structure for empty input', () => {
    const tree = buildStructureTree([], '');
    expect(Object.keys(tree)).toHaveLength(0);
  });

  it('nests deeply', () => {
    const items = [file('a/b/c/deep.ts')];
    const tree = buildStructureTree(items, '');
    expect(tree['a/b/c']!.files).toContain('deep.ts');
  });
});

// ---------------------------------------------------------------------------
// buildFileSizeMap
// ---------------------------------------------------------------------------

describe('buildFileSizeMap', () => {
  it('maps file names to their sizes by directory', () => {
    const items = [file('src/index.ts', 500), file('src/utils.ts', 200)];
    const sizeMap = buildFileSizeMap(items, '');
    expect(sizeMap['src']!['index.ts']).toBe(500);
    expect(sizeMap['src']!['utils.ts']).toBe(200);
  });

  it('skips directories (type=dir)', () => {
    const items = [dir('src'), file('src/a.ts', 100)];
    const sizeMap = buildFileSizeMap(items, '');
    // dir entries are skipped
    expect(Object.keys(sizeMap)).toHaveLength(1);
  });

  it('skips items without a size', () => {
    const item: GitHubApiFileItem = {
      path: 'src/nosize.ts',
      name: 'nosize.ts',
      type: 'file',
      sha: 'abc',
      url: '',
      html_url: '',
      git_url: '',
    };
    const sizeMap = buildFileSizeMap([item], '');
    expect(Object.keys(sizeMap)).toHaveLength(0);
  });

  it('strips basePath prefix from file paths', () => {
    const items = [file('packages/core/index.ts', 300)];
    const sizeMap = buildFileSizeMap(items, 'packages/core');
    expect(sizeMap['.']!['index.ts']).toBe(300);
  });

  it('returns empty map for empty input', () => {
    expect(Object.keys(buildFileSizeMap([], ''))).toHaveLength(0);
  });

  it('handles root-level files (no slash in path)', () => {
    const items = [file('README.md', 42)];
    const sizeMap = buildFileSizeMap(items, '');
    expect(sizeMap['.']!['README.md']).toBe(42);
  });
});

// ---------------------------------------------------------------------------
// buildStructureResult
// ---------------------------------------------------------------------------

const BASE_ARGS = {
  owner: 'facebook',
  repo: 'react',
  workingBranch: 'main',
  cleanPath: '',
  depth: 2,
  allItems: [
    file('src/index.ts', 100),
    file('src/utils.ts', 200),
    dir('src'),
    file('README.md', 50),
  ],
  partialTreeFailures: 0,
  incompleteTree: false,
  rawResponseChars: 5000,
  includeSizes: false,
} as const;

describe('buildStructureResult', () => {
  it('turns provider tree truncation into an explicit terminal partial result', () => {
    const result = buildStructureResult({
      ...BASE_ARGS,
      incompleteTree: true,
    });
    expect(result).toMatchObject({
      isPartial: true,
      terminalLimit: true,
      partialReasons: ['providerTreeTruncated'],
    });
  });

  it('marks recursive subtree failures as retryable partial results', () => {
    const result = buildStructureResult({
      ...BASE_ARGS,
      partialTreeFailures: 2,
    });
    expect(result).toMatchObject({
      isPartial: true,
      partialReasons: ['partialTreeFailures'],
    });
    expect(result.terminalLimit).toBeUndefined();
  });
  it('returns owner/repo/branch in the result', () => {
    const result = buildStructureResult(BASE_ARGS);
    expect(result.owner).toBe('facebook');
    expect(result.repo).toBe('react');
    expect(result.branch).toBe('main');
  });

  it('sets path to "/" when cleanPath is empty', () => {
    const result = buildStructureResult(BASE_ARGS);
    expect(result.path).toBe('/');
  });

  it('uses non-empty cleanPath as-is', () => {
    const result = buildStructureResult({ ...BASE_ARGS, cleanPath: 'src' });
    expect(result.path).toBe('src');
  });

  it('includes defaultBranch when provided', () => {
    const result = buildStructureResult({
      ...BASE_ARGS,
      repoDefaultBranch: 'main',
    });
    expect(result.defaultBranch).toBe('main');
  });

  it('omits defaultBranch when not provided', () => {
    const result = buildStructureResult(BASE_ARGS);
    expect(result).not.toHaveProperty('defaultBranch');
  });

  it('filters ignored directories and files', () => {
    const items = [
      ...BASE_ARGS.allItems,
      dir('node_modules'),
      file('node_modules/pkg/index.js', 10),
    ];
    const result = buildStructureResult({ ...BASE_ARGS, allItems: items });
    // node_modules should be filtered out
    const allPaths = Object.values(result.structure).flatMap(d => [
      ...d.files,
      ...d.folders,
    ]);
    expect(allPaths.some(p => p.includes('node_modules'))).toBe(false);
  });

  it('paginates items correctly', () => {
    const manyItems = Array.from({ length: 50 }, (_, i) =>
      file(`src/f${i}.ts`)
    );
    const result = buildStructureResult({
      ...BASE_ARGS,
      allItems: manyItems,
      itemsPerPage: 10,
      page: 1,
    });
    expect(result.pagination.currentPage).toBe(1);
    expect(result.pagination.hasMore).toBe(true);
    expect(result.pagination.nextPage).toBe(2);
    expect(result.pagination.totalPages).toBeGreaterThan(1);
  });

  it('page 2 returns the second slice', () => {
    const manyItems = Array.from({ length: 25 }, (_, i) =>
      file(`src/f${i}.ts`)
    );
    const p2 = buildStructureResult({
      ...BASE_ARGS,
      allItems: manyItems,
      itemsPerPage: 10,
      page: 2,
    });
    expect(p2.pagination.currentPage).toBe(2);
  });

  it('reports hasMore=false on last page', () => {
    const result = buildStructureResult({
      ...BASE_ARGS,
      allItems: [file('src/a.ts')],
      itemsPerPage: 100,
      page: 1,
    });
    expect(result.pagination.hasMore).toBe(false);
    expect(result).not.toHaveProperty('pagination.nextPage');
  });

  it('includes fileSizeMap when includeSizes is true', () => {
    const result = buildStructureResult({ ...BASE_ARGS, includeSizes: true });
    expect(result.fileSizeMap).toBeDefined();
    expect(result._cachedFileSizeMap).toBeDefined();
  });

  it('omits fileSizeMap when includeSizes is false', () => {
    const result = buildStructureResult({ ...BASE_ARGS, includeSizes: false });
    expect(result.fileSizeMap).toBeUndefined();
  });

  it('adds partial-tree failure hint when partialTreeFailures > 0', () => {
    const result = buildStructureResult({
      ...BASE_ARGS,
      partialTreeFailures: 2,
    });
    expect(result.hints.some(h => h.includes('Partial tree'))).toBe(true);
  });

  it('prepends extraHints to the hints array', () => {
    const result = buildStructureResult({
      ...BASE_ARGS,
      extraHints: ['Custom hint'],
    });
    expect(result.hints.some(h => h.includes('Custom hint'))).toBe(true);
  });

  it('sets incompleteTree in summary when true', () => {
    const result = buildStructureResult({ ...BASE_ARGS, incompleteTree: true });
    expect(result.summary.incompleteTree).toBe(true);
  });

  it('omits incompleteTree from summary when false', () => {
    const result = buildStructureResult({
      ...BASE_ARGS,
      incompleteTree: false,
    });
    expect(result.summary).not.toHaveProperty('incompleteTree');
  });

  it('sorts dirs before files in _cachedItems', () => {
    const result = buildStructureResult(BASE_ARGS);
    const dirs = result._cachedItems.filter(i => i.type === 'dir');
    const files = result._cachedItems.filter(i => i.type === 'file');
    // dirs should come first in the sorted structure
    expect(dirs.length).toBeGreaterThan(0);
    expect(files.length).toBeGreaterThan(0);
    // First item should be a dir
    expect(result._cachedItems[0]?.type).toBe('dir');
  });

  it('sets apiSource to true', () => {
    const result = buildStructureResult(BASE_ARGS);
    expect(result.apiSource).toBe(true);
  });

  it('includes rawResponseChars', () => {
    const result = buildStructureResult(BASE_ARGS);
    expect(result.rawResponseChars).toBe(5000);
  });
});
