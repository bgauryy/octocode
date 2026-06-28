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

describe('ghSearchCode finalizer — file row shape (no redundant fields)', () => {
  it('does not repeat queryId on each file row — it always equals the parent result id', () => {
    const sc = runFinalizer(
      [{ id: 'q1' }],
      [
        {
          id: 'q1',
          data: { results: [groupResult('octo', 'a', 'src/a.ts', 'foo')] },
        },
      ]
    );

    const results = sc.results as Array<AnyRec>;
    expect(results).toHaveLength(1);
    expect(results[0].id).toBe('q1');

    const files = (results[0].data as AnyRec).files as Array<AnyRec>;
    expect(files.length).toBeGreaterThan(0);
    for (const file of files) {
      // queryId is redundant with results[].id and must not be emitted per row
      expect(file).not.toHaveProperty('queryId');
      // owner/repo ARE retained: a global (un-scoped) code search returns files
      // from many repos, so per-row owner/repo is meaningful, not redundant.
      expect(file.owner).toBe('octo');
      expect(file.repo).toBe('a');
      expect(file.path).toBe('src/a.ts');
    }
  });
});
