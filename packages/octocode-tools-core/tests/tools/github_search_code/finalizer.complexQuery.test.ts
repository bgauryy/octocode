import { describe, expect, it } from 'vitest';

import { buildGhSearchCodeFinalizer } from '../../../src/tools/github_search_code/finalizer.js';

type AnyRec = Record<string, unknown>;

function runFinalizerWithQueries(queries: AnyRec[], results: AnyRec[]) {
  const finalize = buildGhSearchCodeFinalizer();
  const out = finalize({
    queries: queries as never,
    results: results as never,
    config: {} as never,
  });
  return out.structuredContent as AnyRec;
}

const MANY_KEYWORDS = Array.from({ length: 12 }, (_, i) => `term${i}`);

describe('ghSearchCode finalizer — overly-long query zero-result honesty', () => {
  it('warns when a >8-keyword query returns zero unexplained results', () => {
    const sc = runFinalizerWithQueries(
      [{ id: 'q1', keywords: MANY_KEYWORDS }],
      [{ id: 'q1', data: { results: [] } }]
    );

    const warnings = sc.warnings as string[];
    expect(Array.isArray(warnings)).toBe(true);
    expect(warnings.join(' ')).toContain('silently under-match');
  });

  it('does not warn for a short keyword query with zero results', () => {
    const sc = runFinalizerWithQueries(
      [{ id: 'q1', keywords: ['useState'] }],
      [{ id: 'q1', data: { results: [] } }]
    );
    expect(sc.warnings).toBeUndefined();
  });

  it('does not double-warn when the zero result is already explained (renamed/archived/not-found)', () => {
    const sc = runFinalizerWithQueries(
      [{ id: 'q1', owner: 'facebook', repo: 'react', keywords: MANY_KEYWORDS }],
      [{ id: 'q1', data: { results: [], nonExistentScope: true } }]
    );
    const warnings = (sc.warnings as string[] | undefined) ?? [];
    expect(warnings.join(' ')).not.toContain('silently under-match');
  });
});
