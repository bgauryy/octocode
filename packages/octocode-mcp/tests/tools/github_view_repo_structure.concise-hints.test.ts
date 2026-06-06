import { describe, expect, it } from 'vitest';
import { applyGithubViewRepoStructureVerbosity } from '../../src/tools/github_view_repo_structure/execution.js';

describe('githubViewRepoStructure verbose contract — data is preserved', () => {
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

  it('verbose:false and verbose:true produce the same extraHints', () => {
    const withoutMeta = applyGithubViewRepoStructureVerbosity(input, {
      verbose: false,
    } as never);
    const withMeta = applyGithubViewRepoStructureVerbosity(input, {
      verbose: true,
    } as never);
    expect(withMeta.extraHints).toEqual(withoutMeta.extraHints);
  });

  it('emits Next paths: hint when truncated (verbose:false)', () => {
    const out = applyGithubViewRepoStructureVerbosity(input, {
      verbose: false,
    } as never);
    const next = out.extraHints.filter(h => h.startsWith('Next paths: '));
    expect(next).toHaveLength(1);
  });
});

describe('githubViewRepoStructure verbose entries — data structure preserved', () => {
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

  it('verbose:false and verbose:true expose same structure data', () => {
    const withoutMeta = applyGithubViewRepoStructureVerbosity(input, {
      verbose: false,
    } as never);
    const withMeta = applyGithubViewRepoStructureVerbosity(input, {
      verbose: true,
    } as never);
    expect((withMeta.data as Record<string, unknown>).structure).toEqual(
      (withoutMeta.data as Record<string, unknown>).structure
    );
  });

  it('verbose:false includes raw structure (unchanged)', () => {
    const out = applyGithubViewRepoStructureVerbosity(input, {
      verbose: false,
    } as never);
    expect((out.data as Record<string, unknown>).structure).toBeDefined();
  });
});
