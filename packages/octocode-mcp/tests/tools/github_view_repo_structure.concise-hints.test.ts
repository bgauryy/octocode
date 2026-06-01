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
