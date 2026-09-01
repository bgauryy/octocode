import { describe, expect, it } from 'vitest';

import { buildGhSearchCodeFinalizer } from '../../../src/tools/github_search_code/finalizer.js';

type AnyRec = Record<string, unknown>;

function runFinalizer(results: AnyRec[]) {
  const finalize = buildGhSearchCodeFinalizer();
  const out = finalize({
    queries: results.map(() => ({})) as never,
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

describe('github.code finalizer — incomplete_results (GitHub index degradation)', () => {
  it('flags an empty query with typed partial diagnostics', () => {
    const sc = runFinalizerWithQueries(
      [{ keywords: ['react'] }],
      [{ index: 0, data: { results: [], incompleteResults: true } }]
    );

    const empty = sc.results as Array<AnyRec>;
    expect(empty).toHaveLength(1);
    expect(empty[0]).toMatchObject({ index: 0, status: 'empty' });
    // Distinguishes "GitHub index did not complete" from a true no-match.
    expect((empty[0].data as AnyRec).incompleteResults).toBe(true);
    expect((empty[0].data as AnyRec).nonExistentScope).toBeUndefined();
    expect(empty[0]).toMatchObject({
      meta: {
        diagnostics: {
          codes: ['ghIncompleteResults'],
          hints: [expect.stringContaining('retry')],
          partial: true,
        },
      },
      data: {
        next: {
          retry: {
            tool: 'github.code',
            query: { keywords: ['react'] },
          },
        },
      },
    });
    expect(sc.emptyQueries).toBeUndefined();

    // Responses carry no warnings channel — the typed flag above is the signal.
    expect(sc.warnings).toBeUndefined();
  });

  it('a genuine no-match (complete search) carries no incompleteResults and no warning', () => {
    const sc = runFinalizer([{ index: 0, data: { results: [] } }]);

    const empty = sc.results as Array<AnyRec>;
    expect(empty).toHaveLength(1);
    expect(empty[0]).toMatchObject({ index: 0, status: 'empty' });
    expect(
      (empty[0].data as AnyRec | undefined)?.incompleteResults
    ).toBeUndefined();
    expect(sc.warnings).toBeUndefined();
  });

  it('a scoped repo no-match carries typed proof limits and recovery', () => {
    const sc = runFinalizerWithQueries(
      [{ owner: 'facebook', repo: 'react' }],
      [{ index: 0, data: { results: [] } }]
    );

    const empty = sc.results as Array<AnyRec>;
    expect(empty).toHaveLength(1);
    expect(empty[0]).toMatchObject({
      index: 0,
      status: 'empty',
      meta: {
        diagnostics: {
          codes: ['ghScopedZeroUnproven'],
          hints: [expect.stringContaining('verify')],
        },
      },
      data: {
        next: {
          viewStructure: {
            tool: 'github.tree',
            query: { owner: 'facebook', repo: 'react', path: '' },
          },
        },
      },
    });
    expect(sc.warnings).toBeUndefined();
  });
});
