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
  it('flags an empty query as incompleteResults and surfaces a visible warning', () => {
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

    const warnings = sc.warnings as string[];
    expect(Array.isArray(warnings)).toBe(true);
    expect(warnings.join(' ')).toContain('incomplete_results');
  });

  it('a genuine no-match (complete search) carries no incompleteResults and no warning', () => {
    const sc = runFinalizer([{ id: 'q1', data: { results: [] } }]);

    const empty = sc.emptyQueries as Array<AnyRec>;
    expect(empty).toHaveLength(1);
    expect(empty[0].incompleteResults).toBeUndefined();
    expect(sc.warnings).toBeUndefined();
  });

  it('warns that a scoped repo no-match is not proof of absence', () => {
    const sc = runFinalizerWithQueries(
      [{ id: 'q1', owner: 'facebook', repo: 'react' }],
      [{ id: 'q1', data: { results: [] } }]
    );

    const warnings = sc.warnings as string[];
    expect(warnings.join(' ')).toContain('unproven absence');
    expect(warnings.join(' ')).toContain('materialize or clone');
  });
});
