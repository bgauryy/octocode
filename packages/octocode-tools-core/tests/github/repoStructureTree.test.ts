import { describe, expect, it } from 'vitest';
import {
  filterGitTreeEntries,
  isGitStructureTreesEnabled,
  type GitTreeEntry,
} from '../../src/github/repoStructureTree.js';

describe('filterGitTreeEntries', () => {
  const tree: GitTreeEntry[] = [
    { path: 'README.md', type: 'blob', size: 10, sha: 'a' },
    { path: 'src', type: 'tree', sha: 'b' },
    { path: 'src/index.ts', type: 'blob', size: 20, sha: 'c' },
    { path: 'src/lib', type: 'tree', sha: 'd' },
    { path: 'src/lib/util.ts', type: 'blob', size: 30, sha: 'e' },
    { path: 'packages/octocode', type: 'tree', sha: 'f' },
    { path: 'packages/octocode/package.json', type: 'blob', size: 40, sha: 'g' },
  ];

  it('caps at maxDepth from repo root', () => {
    const depth1 = filterGitTreeEntries(tree, { maxDepth: 1 });
    expect(depth1.map(i => i.path).sort()).toEqual(['README.md', 'src']);
  });

  it('includes depth-2 entries when maxDepth=2', () => {
    const depth2 = filterGitTreeEntries(tree, { maxDepth: 2 });
    expect(depth2.some(i => i.path === 'src/index.ts')).toBe(true);
    expect(depth2.some(i => i.path === 'src/lib')).toBe(true);
    expect(depth2.some(i => i.path === 'src/lib/util.ts')).toBe(false);
  });

  it('scopes to pathPrefix with relative depth', () => {
    const scoped = filterGitTreeEntries(tree, {
      pathPrefix: 'src',
      maxDepth: 1,
    });
    expect(scoped.map(i => i.path).sort()).toEqual(
      ['src/index.ts', 'src/lib'].sort()
    );
    expect(scoped.every(i => i.type === 'file' || i.type === 'dir')).toBe(true);
  });

  it('maps blob→file and tree→dir', () => {
    const items = filterGitTreeEntries(tree, { maxDepth: 1 });
    expect(items.find(i => i.path === 'README.md')?.type).toBe('file');
    expect(items.find(i => i.path === 'src')?.type).toBe('dir');
  });
});

describe('isGitStructureTreesEnabled', () => {
  it('defaults to enabled', () => {
    expect(isGitStructureTreesEnabled({})).toBe(true);
  });

  it('disables when OCTOCODE_GH_STRUCTURE_TREES=0', () => {
    expect(
      isGitStructureTreesEnabled({ OCTOCODE_GH_STRUCTURE_TREES: '0' })
    ).toBe(false);
  });
});
