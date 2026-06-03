import { describe, expect, it } from 'vitest';
import { applyGithubViewRepoStructureVerbosity } from '../../src/tools/github_view_repo_structure/execution.js';

// #B1: in concise mode the `top:` sampler and the `Next paths:` truncation
// cursor were both emitted from the same entry sample — identical content.
// Concise must emit only `top:`.
describe('githubViewRepoStructure concise hints (#B1)', () => {
  const input = {
    data: {
      path: 'src',
      structure: {
        '.': { folders: ['a', 'b'], files: ['x.ts'] },
      },
    },
    entryCount: 20,
    summary: { truncated: true },
    extraHints: [],
  };

  it('emits a single top: hint and no duplicate Next paths: hint when concise', () => {
    const out = applyGithubViewRepoStructureVerbosity(input, {
      verbosity: 'concise',
    } as never);
    const top = out.extraHints.filter(h => h.startsWith('top: '));
    const next = out.extraHints.filter(h => h.startsWith('Next paths: '));
    expect(top).toHaveLength(1);
    expect(next).toHaveLength(0);
  });

  it('still emits Next paths: in basic mode (no top: there, so no duplication)', () => {
    const out = applyGithubViewRepoStructureVerbosity(input, {
      verbosity: 'basic',
    } as never);
    const top = out.extraHints.filter(h => h.startsWith('top: '));
    const next = out.extraHints.filter(h => h.startsWith('Next paths: '));
    expect(top).toHaveLength(0);
    expect(next).toHaveLength(1);
  });
});

// #B2: concise must return ALL entry names as a flat `entries` array
// (folders suffixed `/`, then files) — not just the top-5 hint sample.
// Before the fix, concise dropped the structure entirely; agents could not
// navigate the repo without upgrading to basic.
describe('githubViewRepoStructure concise entries field (#B2)', () => {
  const structure = {
    '.': {
      folders: ['src', 'tests', 'docs', 'scripts', 'dist', 'coverage'],
      files: ['package.json', 'tsconfig.json', 'README.md'],
    },
  };
  const input = {
    data: { path: '.', structure },
    entryCount: 9,
    summary: { truncated: false },
    extraHints: [],
  };

  it('concise response includes entries[] with all 9 names (folders first, no metadata)', () => {
    const out = applyGithubViewRepoStructureVerbosity(input, {
      verbosity: 'concise',
    } as never);
    const entries = (out.data as { entries?: string[] }).entries;
    expect(entries).toBeDefined();
    expect(entries).toHaveLength(9);
    // Folders come first, suffixed with /
    expect(entries![0]).toBe('src/');
    expect(entries![5]).toBe('coverage/');
    // Then files
    expect(entries![6]).toBe('package.json');
  });

  it('concise response does NOT include the raw structure map', () => {
    const out = applyGithubViewRepoStructureVerbosity(input, {
      verbosity: 'concise',
    } as never);
    expect((out.data as Record<string, unknown>).structure).toBeUndefined();
  });

  it('basic response still includes raw structure (unchanged)', () => {
    const out = applyGithubViewRepoStructureVerbosity(input, {
      verbosity: 'basic',
    } as never);
    expect((out.data as Record<string, unknown>).structure).toBeDefined();
    expect((out.data as { entries?: string[] }).entries).toBeUndefined();
  });
});
