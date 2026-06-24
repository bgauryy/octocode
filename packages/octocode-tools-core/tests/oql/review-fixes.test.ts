import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { normalizeQuery, normalizeInput } from '../../src/oql/normalize.js';
import { planQuery } from '../../src/oql/planner.js';
import { runOqlSearch } from '../../src/oql/run.js';
import { OqlValidationError } from '../../src/oql/diagnostics.js';
import { isBatchEnvelope, type OqlQuery } from '../../src/oql/types.js';

const OQL_SRC = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../src/oql'
);
function single(r: Awaited<ReturnType<typeof runOqlSearch>>) {
  if (isBatchEnvelope(r)) throw new Error('expected single');
  return r;
}

describe('review fix #1: executable next.* continuations', () => {
  it('code rows carry next.fetch pointing at an exact content read', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'runOqlSearch' },
      })
    );
    const code = env.results.find(r => r.kind === 'code') as {
      next?: Record<string, { query: OqlQuery }>;
    };
    const fetch = code.next?.['next.fetch'];
    expect(fetch).toBeDefined();
    expect(fetch!.query.target).toBe('content');
    expect(fetch!.query.from?.kind).toBe('local');
  });

  it('matchTruncated emits next.matchPage', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'diagnostic' },
      })
    );
    expect(env.next?.['next.matchPage']).toBeDefined();
  });

  it('local code pagination never emits stale row pages for file-paged search', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: { kind: 'text', value: 'normalizeQuery' },
        itemsPerPage: 3,
      })
    );
    expect(env.results.length).toBe(4);
    expect(env.pagination?.itemUnit).toBe('files');
    expect(env.pagination?.rowCount).toBe(4);
    expect(env.pagination?.totalItemsKind).toBe('files');
    expect(env.pagination?.totalItems).toBe(2);
    expect(env.pagination?.reportedTotalItems).toBe(4);
    expect(env.pagination?.reportedRowCount).toBe(4);
    expect(env.pagination?.hasMore).toBe(false);
    expect(env.next?.['next.page']).toBeUndefined();
  });

});

describe('boolean predicate over target:"code" executes (set algebra)', () => {
  it('all(text,text) -> match rows from files matching both, not unsupported', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'all',
          of: [
            { kind: 'text', value: 'diagnostic' },
            { kind: 'text', value: 'export' },
          ],
        },
      })
    );
    // Boolean over code is now implemented via file-set algebra; it must NOT
    // report unsupported, and rows are code occurrences.
    expect(env.evidence.kind).not.toBe('unsupported');
    expect(env.diagnostics.some(d => d.code === 'unsupportedBoolean')).toBe(
      false
    );
    expect(env.results.every(r => r.kind === 'code')).toBe(true);
    expect(env.results.length).toBeGreaterThan(0);
  });

  it('any(text,text) -> union of occurrences', async () => {
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'any',
          of: [
            { kind: 'text', value: 'requiresMaterialization' },
            { kind: 'text', value: 'negativeUniverseRequired' },
          ],
        },
      })
    );
    expect(env.evidence.kind).not.toBe('unsupported');
    expect(env.results.length).toBeGreaterThan(0);
    expect(env.results.every(r => r.kind === 'code')).toBe(true);
  });
});

describe('review fix #4: field modified absolute compare is unsupported', () => {
  it('modified > date -> unsupportedPredicate (no bogus modifiedWithin)', async () => {
    const env = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'local', path: OQL_SRC },
        where: {
          kind: 'field',
          field: 'modified',
          op: '>',
          value: '2024-01-01',
        },
      })
    );
    expect(env.diagnostics.some(d => d.code === 'unsupportedPredicate')).toBe(
      true
    );
  });
});

describe('review fix #5: unbounded materialization blocks', () => {
  it('mode:required + subtree + no scope.path -> not executable', () => {
    const q = normalizeQuery({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react' },
      where: { kind: 'structural', lang: 'js', pattern: 'x($A)' },
      materialize: { mode: 'required', strategy: 'subtree' },
    } as never) as OqlQuery;
    const { executable, plan } = planQuery(q, {});
    expect(executable).toBe(false);
    expect(
      plan.diagnostics.some(d => d.code === 'materializationNotAllowed')
    ).toBe(true);
  });
});

describe('review fix #7: where rejected on content/structure', () => {
  for (const target of ['content', 'structure']) {
    it(`${target} + where -> invalidQuery`, () => {
      expect(() =>
        normalizeQuery({
          target,
          from: { kind: 'local', path: '.' },
          where: { kind: 'text', value: 'x' },
        } as never)
      ).toThrowError(OqlValidationError);
    });
  }
});

describe('review fix #12: structural sugar requires canonical lang', () => {
  it('lang works for a structural pattern', () => {
    const n = normalizeQuery({
      path: './src',
      pattern: 'eval($X)',
      lang: 'ts',
    } as never);
    expect(n.where).toMatchObject({
      kind: 'structural',
      lang: 'ts',
      pattern: 'eval($X)',
    });
  });
});

describe('review fix #16: batch-level unknown fields rejected', () => {
  it('batchId (not a field) -> unknownField', () => {
    expect(() =>
      normalizeInput({
        batchId: 'x',
        queries: [
          {
            target: 'code',
            from: { kind: 'local', path: '.' },
            where: { kind: 'text', value: 'x' },
          },
        ],
      } as never)
    ).toThrowError(OqlValidationError);
  });
});

describe('review fix #9: github files -> requiresMaterialization', () => {
  it('files over github (no materialize) reports requiresMaterialization', async () => {
    // A non-path field op (size comparison) genuinely cannot be enumerated by
    // the provider — path-like field equality (basename/extension/path "=") now
    // pushes down to provider path search instead, like target:"files".
    const env = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'github', repo: 'facebook/react' },
        where: { kind: 'field', field: 'size', op: '>', value: 100 },
        materialize: { mode: 'never' },
      })
    );
    expect(
      env.diagnostics.some(d => d.code === 'requiresMaterialization')
    ).toBe(true);
    expect(env.diagnostics.some(d => d.code === 'unsupportedTarget')).toBe(
      false
    );
  });
});
