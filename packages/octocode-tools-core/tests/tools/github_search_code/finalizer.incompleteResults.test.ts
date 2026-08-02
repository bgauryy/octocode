import { describe, expect, it } from 'vitest';

import { buildGhSearchCodeFinalizer } from '../../../src/tools/github_search_code/finalizer.js';

type AnyRec = Record<string, unknown>;

function runFinalizer(results: AnyRec[]) {
  const finalize = buildGhSearchCodeFinalizer();
  const out = finalize({
    queries: results.map(r => ({ id: r.id })) as never,
    results: results as never,
    config: {} as never,
  });
  return out.structuredContent as AnyRec;
}

function runFinalizerWithQueries(queries: AnyRec[], results: AnyRec[]) {
  const finalize = buildGhSearchCodeFinalizer();
  const out = finalize({
    queries: queries as never,
    results: results as never,
    config: {} as never,
  });
  return out.structuredContent as AnyRec;
}

describe('ghSearchCode finalizer — incomplete_results (GitHub index degradation)', () => {
  it('flags an empty query as incompleteResults (typed flag; warnings are stripped from responses)', () => {
    const sc = runFinalizer([
      {
        id: 'q1',
        data: { results: [], incompleteResults: true },
      },
    ]);

    const empty = sc.emptyQueries as Array<AnyRec>;
    expect(empty).toHaveLength(1);
    expect(empty[0].id).toBe('q1');
    // Distinguishes "GitHub index did not complete" from a true no-match.
    expect(empty[0].incompleteResults).toBe(true);
    expect(empty[0].nonExistentScope).toBeUndefined();

    // Responses carry no warnings channel — the typed flag above is the signal.
    expect(sc.warnings).toBeUndefined();
  });

  it('a genuine no-match (complete search) carries no incompleteResults and no warning', () => {
    const sc = runFinalizer([{ id: 'q1', data: { results: [] } }]);

    const empty = sc.emptyQueries as Array<AnyRec>;
    expect(empty).toHaveLength(1);
    expect(empty[0].incompleteResults).toBeUndefined();
    expect(sc.warnings).toBeUndefined();
  });

  it('a scoped repo no-match yields an empty query row with no warnings channel', () => {
    const sc = runFinalizerWithQueries(
      [{ id: 'q1', owner: 'facebook', repo: 'react' }],
      [{ id: 'q1', data: { results: [] } }]
    );

    const empty = sc.emptyQueries as Array<AnyRec>;
    expect(empty).toHaveLength(1);
    expect(sc.warnings).toBeUndefined();
  });
});
