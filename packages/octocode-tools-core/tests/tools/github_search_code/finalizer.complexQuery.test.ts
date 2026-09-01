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

describe('github.code finalizer — overly-long query zero-result honesty', () => {
  it('surfaces a typed diagnostic and bounded retry for a complex empty query', () => {
    const sc = runFinalizerWithQueries(
      [{ keywords: MANY_KEYWORDS }],
      [{ index: 0, data: { results: [] } }]
    );

    expect(sc.warnings).toBeUndefined();
    expect(sc.results).toMatchObject([
      {
        index: 0,
        status: 'empty',
        meta: {
          diagnostics: {
            codes: ['ghQueryPossiblyTooComplex'],
            hints: [expect.stringContaining('narrow')],
          },
        },
        data: {
          next: {
            retryNarrow: {
              tool: 'github.code',
              query: { keywords: MANY_KEYWORDS.slice(0, 8) },
            },
          },
        },
      },
    ]);
    expect(sc.emptyQueries).toBeUndefined();
  });

  it('does not warn for a short keyword query with zero results', () => {
    const sc = runFinalizerWithQueries(
      [{ keywords: ['useState'] }],
      [{ index: 0, data: { results: [] } }]
    );
    expect(sc.warnings).toBeUndefined();
    expect((sc.results as AnyRec[])[0]?.meta).toBeUndefined();
  });

  it('does not double-warn when the zero result is already explained (renamed/archived/not-found)', () => {
    const sc = runFinalizerWithQueries(
      [{ owner: 'facebook', repo: 'react', keywords: MANY_KEYWORDS }],
      [{ index: 0, data: { results: [], nonExistentScope: true } }]
    );
    const warnings = (sc.warnings as string[] | undefined) ?? [];
    expect(warnings.join(' ')).not.toContain('silently under-match');
  });
});
