import { describe, expect, it } from 'vitest';

import { buildGhSearchCodeFinalizer } from '../../../src/tools/github_search_code/finalizer.js';

type AnyRec = Record<string, unknown>;

function runFinalizer(queries: AnyRec[], results: AnyRec[]) {
  const finalize = buildGhSearchCodeFinalizer();
  const out = finalize({
    queries: queries as never,
    results: results as never,
    config: {} as never,
  });
  return out.structuredContent as AnyRec;
}

function groupResult(owner: string, repo: string, path: string, value: string) {
  return { id: `${owner}/${repo}`, owner, repo, matches: [{ path, value }] };
}

describe('github.code finalizer — file row shape (no redundant fields)', () => {
  it('does not repeat queryIndex on each file row — it equals the parent result index', () => {
    const sc = runFinalizer(
      [{}],
      [
        {
          index: 0,
          data: { results: [groupResult('octo', 'a', 'src/a.ts', 'foo')] },
        },
      ]
    );

    const results = sc.results as Array<AnyRec>;
    expect(results).toHaveLength(1);
    expect(results[0].index).toBe(0);

    const files = (results[0].data as AnyRec).files as Array<AnyRec>;
    expect(files.length).toBeGreaterThan(0);
    for (const file of files) {
      // queryIndex is redundant with results[].index and must not be emitted per row
      expect(file).not.toHaveProperty('queryIndex');
      // owner/repo ARE retained: a global (un-scoped) code search returns files
      // from many repos, so per-row owner/repo is meaningful, not redundant.
      expect(file.owner).toBe('octo');
      expect(file.repo).toBe('a');
      expect(file.path).toBe('src/a.ts');
    }
  });
});
