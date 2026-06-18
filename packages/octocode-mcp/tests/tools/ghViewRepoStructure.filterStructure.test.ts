/**
 * Tests for Bug 2: ghViewRepoStructure filterStructure orphan dir entries.
 *
 * Before the fix, `filterStructure` removed ignored dirs from each entry's
 * `folders` list but left TOP-LEVEL structure keys whose basename was an
 * ignored dir (e.g., '.github', '.yarn'). These became orphan entries: present
 * as top-level keys but unreachable from any parent.
 *
 * After the fix, top-level keys whose last path component is in
 * IGNORED_FOLDER_NAMES are skipped entirely.
 */
import { describe, it, expect } from 'vitest';
import { filterStructure } from '../../../octocode-tools-core/src/tools/github_view_repo_structure/execution.js';
import { hints as repoStructureHints } from '../../../octocode-tools-core/src/tools/github_view_repo_structure/hints.js';

describe('filterStructure — orphan top-level dir entries', () => {
  it('removes .github as a top-level key', () => {
    const structure = {
      '.': { files: ['README.md'], folders: ['.github', 'src'] },
      '.github': { files: ['CODEOWNERS'], folders: ['workflows'] },
      '.github/workflows': { files: ['ci.yml'], folders: [] },
      src: { files: ['index.ts'], folders: [] },
    };

    const result = filterStructure(structure);

    // .github itself (basename='.github') is in IGNORED_FOLDER_NAMES → removed
    expect(Object.keys(result)).not.toContain('.github');
    // .github/workflows has basename='workflows' which is NOT ignored → kept
    // (it's now an orphan, but the filter only acts on the immediate dirname)
    expect(Object.keys(result)).toContain('.github/workflows');
  });

  it('removes .github from the parent folders list', () => {
    const structure = {
      '.': { files: ['README.md'], folders: ['.github', 'src'] },
      '.github': { files: ['CODEOWNERS'], folders: [] },
      src: { files: ['index.ts'], folders: [] },
    };

    const result = filterStructure(structure);

    expect(result['.']?.folders).not.toContain('.github');
    expect(result['.']?.folders).toContain('src');
  });

  it('removes .yarn as a top-level key', () => {
    const structure = {
      '.': { files: ['package.json'], folders: ['.yarn', 'src'] },
      '.yarn': { files: [], folders: ['cache'] },
      src: { files: ['app.ts'], folders: [] },
    };

    const result = filterStructure(structure);

    expect(Object.keys(result)).not.toContain('.yarn');
  });

  it('keeps root entry "." even though it starts with a dot', () => {
    const structure = {
      '.': { files: ['README.md'], folders: ['src'] },
      src: { files: ['index.ts'], folders: [] },
    };

    const result = filterStructure(structure);

    expect(Object.keys(result)).toContain('.');
  });

  it('keeps non-ignored top-level dirs', () => {
    const structure = {
      '.': { files: ['package.json'], folders: ['src', 'tests'] },
      src: { files: ['index.ts'], folders: [] },
      tests: { files: ['index.test.ts'], folders: [] },
    };

    const result = filterStructure(structure);

    expect(Object.keys(result)).toContain('src');
    expect(Object.keys(result)).toContain('tests');
  });

  it('removes node_modules as a top-level key', () => {
    const structure = {
      '.': { files: ['package.json'], folders: ['node_modules', 'src'] },
      node_modules: { files: [], folders: ['lodash'] },
      src: { files: ['app.ts'], folders: [] },
    };

    const result = filterStructure(structure);

    expect(Object.keys(result)).not.toContain('node_modules');
    expect(result['.']?.folders).not.toContain('node_modules');
  });

  it('removes dist as a top-level key', () => {
    const structure = {
      '.': { files: ['package.json'], folders: ['dist', 'src'] },
      dist: { files: ['index.js'], folders: [] },
      src: { files: ['index.ts'], folders: [] },
    };

    const result = filterStructure(structure);

    expect(Object.keys(result)).not.toContain('dist');
  });

  it('handles nested ignored dir paths correctly (removes top-level key if basename is ignored)', () => {
    const structure = {
      '.': { files: ['README.md'], folders: ['packages'] },
      packages: { files: [], folders: ['core', '.git'] },
      'packages/core': { files: ['index.ts'], folders: [] },
      'packages/.git': { files: ['HEAD'], folders: [] },
    };

    const result = filterStructure(structure);

    // Top-level 'packages' is not ignored (basename='packages')
    expect(Object.keys(result)).toContain('packages');
    // 'packages/.git' should be removed because basename='.git' is ignored
    expect(Object.keys(result)).not.toContain('packages/.git');
    // The parent should not list '.git' in folders
    expect(result['packages']?.folders).not.toContain('.git');
  });
});

describe('ghViewRepoStructure hints.empty', () => {
  it('emits filter-specific hint when wasFilteredToEmpty is true', () => {
    const result = repoStructureHints.empty({
      wasFilteredToEmpty: true,
      path: 'packages',
    } as unknown as Parameters<typeof repoStructureHints.empty>[0]);
    expect(result.length).toBeGreaterThan(0);
    expect(result.some(h => h.includes('filtered'))).toBe(true);
  });

  it('emits parent-path hint for valid path without wasFilteredToEmpty', () => {
    const result = repoStructureHints.empty({ path: 'unknown-path' });
    expect(result.some(h => h.includes('parent'))).toBe(true);
  });

  it('returns empty array when no path and wasFilteredToEmpty is false', () => {
    const result = repoStructureHints.empty({});
    expect(result).toEqual([]);
  });
});
