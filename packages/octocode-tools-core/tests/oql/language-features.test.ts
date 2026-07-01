/**
 * Coverage for language features that were previously declared-but-inert:
 *  - `select` projection (filters row fields + continuations)
 *  - `limit` cap on the primary result-row domain
 *  - `controls.budget.maxBooleanExpansion` enforcement on boolean sugar
 *  - `scope.exclude` forwarding to the local backend
 */
import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { runOqlSearch } from '../../src/oql/run.js';
import { normalizeQuery } from '../../src/oql/normalize.js';
import { OqlValidationError } from '../../src/oql/diagnostics.js';
import { isBatchEnvelope } from '../../src/oql/types.js';

const OQL_SRC = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../src/oql'
);
function single(r: Awaited<ReturnType<typeof runOqlSearch>>) {
  if (isBatchEnvelope(r)) throw new Error('expected single');
  return r;
}

describe('select projection', () => {
  it('keeps only selected row fields + identity, drops the rest', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'runOqlSearch' },
        select: ['path', 'next.fetch'],
      })
    );
    const row = env.results.find(r => r.kind === 'code') as Record<
      string,
      unknown
    >;
    expect(row).toBeDefined();
    expect(row.path).toBeDefined();
    // identity is always kept
    expect(row.kind).toBe('code');
    expect(row.proofGrade).toBe('text');
    // snippet/line were not selected -> projected out
    expect('snippet' in row).toBe(false);
    // only next.fetch survives in continuations
    const next = row.next as Record<string, unknown> | undefined;
    if (next) {
      expect(Object.keys(next).every(k => k === 'next.fetch')).toBe(true);
    }
  });

  it('unknown select field -> non-blocking unknownField diagnostic', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'diagnostic' },
        select: ['path', 'notARealField'],
      })
    );
    const diag = env.diagnostics.find(d => d.code === 'unknownField');
    expect(diag).toBeDefined();
    expect(diag?.blocksAnswer).toBe(false);
  });
});

describe('limit', () => {
  it('caps the result-row count', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'diagnostic' },
        limit: 2,
      })
    );
    expect(env.results.length).toBeLessThanOrEqual(2);
  });
});

describe('controls.budget.maxBooleanExpansion', () => {
  it('rejects an oneOf expansion that exceeds the budget', () => {
    expect(() =>
      normalizeQuery({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        oneOf: [
          { kind: 'text', value: 'a' },
          { kind: 'text', value: 'b' },
          { kind: 'text', value: 'c' },
          { kind: 'text', value: 'd' },
        ],
        controls: { budget: { maxBooleanExpansion: 4 } },
      } as never)
    ).toThrowError(OqlValidationError);
  });

  it('allows an expansion within the budget', () => {
    const q = normalizeQuery({
      target: 'code',
      from: { kind: 'local', path: OQL_SRC },
      oneOf: [
        { kind: 'text', value: 'a' },
        { kind: 'text', value: 'b' },
      ],
    } as never);
    expect(q).toBeDefined();
  });
});

describe('predicate id uniqueness', () => {
  it('rejects duplicate user-supplied predicate ids', () => {
    expect(() =>
      normalizeQuery({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'all',
          of: [
            { kind: 'text', value: 'a', id: 'p1' },
            { kind: 'text', value: 'b', id: 'p1' },
          ],
        },
      } as never)
    ).toThrowError(/Duplicate predicate id "p1"/);
  });

  it('rejects a duplicate id between a boolean node and a nested leaf', () => {
    expect(() =>
      normalizeQuery({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'not',
          id: 'p1',
          predicate: { kind: 'text', value: 'a', id: 'p1' },
        },
      } as never)
    ).toThrowError(OqlValidationError);
  });

  it('allows unique and absent ids', () => {
    const q = normalizeQuery({
      target: 'code',
      from: { kind: 'local', path: OQL_SRC },
      where: {
        kind: 'all',
        of: [
          { kind: 'text', value: 'a', id: 'p1' },
          { kind: 'text', value: 'b', id: 'p2' },
          { kind: 'text', value: 'c' },
        ],
      },
    } as never);
    expect(q).toBeDefined();
  });

  // oneOf/xor expansion re-places the SAME predicate object at multiple tree
  // paths; that must NOT read as a duplicate id (regression: unique ids in
  // oneOf/xor were spuriously rejected).
  it('allows unique ids through oneOf expansion', () => {
    for (const sugar of ['oneOf', 'xor'] as const) {
      const q = normalizeQuery({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        [sugar]: [
          { kind: 'text', value: 'a', id: 'p1' },
          { kind: 'text', value: 'b', id: 'p2' },
        ],
      } as never);
      expect(q).toBeDefined();
    }
  });

  it('still rejects genuinely duplicate ids inside oneOf', () => {
    expect(() =>
      normalizeQuery({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        oneOf: [
          { kind: 'text', value: 'a', id: 'p1' },
          { kind: 'text', value: 'b', id: 'p1' },
        ],
      } as never)
    ).toThrowError(/Duplicate predicate id "p1"/);
  });
});

describe('scope.exclude forwarding', () => {
  it('runs with an exclude glob without error', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        scope: { exclude: ['*.test.ts'] },
        where: { kind: 'text', value: 'diagnostic' },
      })
    );
    expect(env.evidence.kind).not.toBe('unsupported');
  });
});
