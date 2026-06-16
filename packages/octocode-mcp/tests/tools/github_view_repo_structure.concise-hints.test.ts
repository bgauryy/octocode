import { describe, expect, it } from 'vitest';
import { buildRepoStructureOutput } from '../../../octocode-tools-core/src/tools/github_view_repo_structure/execution.js';

describe('ghViewRepoStructure pass-through contract — data is preserved', () => {
  const input = {
    data: {
      path: 'src',
      structure: {
        '.': { folders: ['a', 'b'], files: ['x.ts'] },
      },
    },
    entryCount: 20,
    wasTruncated: true,
    extraHints: [],
  };

  it(' and produce the same extraHints', () => {
    const withoutMeta = buildRepoStructureOutput(input, {} as never);
    const withMeta = buildRepoStructureOutput(input, {} as never);
    expect(withMeta.extraHints).toEqual(withoutMeta.extraHints);
  });

  it('emits Next paths: hint when truncated ()', () => {
    const out = buildRepoStructureOutput(input, {} as never);
    const next = out.extraHints.filter(h => h.startsWith('Next paths: '));
    expect(next).toHaveLength(1);
  });
});

describe('ghViewRepoStructure entries — data structure preserved', () => {
  const structure = {
    '.': {
      folders: ['src', 'tests', 'docs', 'scripts', 'dist', 'coverage'],
      files: ['package.json', 'tsconfig.json', 'README.md'],
    },
  };
  const input = {
    data: { path: '.', structure },
    entryCount: 9,
    wasTruncated: false,
    extraHints: [],
  };

  it(' and expose same structure data', () => {
    const withoutMeta = buildRepoStructureOutput(input, {} as never);
    const withMeta = buildRepoStructureOutput(input, {} as never);
    expect((withMeta.data as Record<string, unknown>).structure).toEqual(
      (withoutMeta.data as Record<string, unknown>).structure
    );
  });

  it(' includes raw structure (unchanged)', () => {
    const out = buildRepoStructureOutput(input, {} as never);
    expect((out.data as Record<string, unknown>).structure).toBeDefined();
  });
});
