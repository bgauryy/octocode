/**
 * Regression tests for the OQL self-audit findings (2026-06-24):
 *  - #6  GitHub target:"files" with no `where` must not be a dead end — it must
 *        emit a runnable next.materialize continuation.
 *  - #7  `--scheme` (OQL_SCHEMA_DOC) must document every live, validated feature.
 *  - #12 An explicit `limit` is a hard cap on the primary result domain, even
 *        for paged targets.
 */
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { runOqlSearch } from '../../src/oql/run.js';
import { isBatchEnvelope, type OqlResultEnvelope } from '../../src/oql/types.js';
import { OQL_SCHEMA_DOC } from '../../src/oql/schemeText.js';

function single(r: Awaited<ReturnType<typeof runOqlSearch>>): OqlResultEnvelope {
  if (isBatchEnvelope(r)) throw new Error('expected single envelope');
  return r;
}

describe('audit #6: GitHub target:"files" without `where` is not a dead end', () => {
  it('emits a runnable next.materialize continuation instead of unsupported-with-no-path', async () => {
    const env = single(
      await runOqlSearch({ target: 'files', repo: 'facebook/react' })
    );
    const cont = env.next?.['next.materialize'];
    expect(cont, 'expected a next.materialize continuation').toBeTruthy();
    expect(cont?.query.target).toBe('materialize');
    expect(
      (cont?.query as { materialize?: { mode?: string } }).materialize?.mode
    ).toBe('required');
    expect((cont?.query as { from?: unknown }).from).toEqual({
      kind: 'github',
      repo: 'facebook/react',
    });
  });
});

describe('audit #7: --scheme documents every live feature', () => {
  const doc = JSON.stringify(OQL_SCHEMA_DOC);

  it('documents fetch.content.fullContent', () => {
    expect(OQL_SCHEMA_DOC.query.fetch).toContain('fullContent');
  });

  it('documents all live where.field names', () => {
    for (const f of [
      'accessed',
      'empty',
      'permissions',
      'executable',
      'readable',
      'writable',
    ]) {
      expect(OQL_SCHEMA_DOC.predicates.field, `field "${f}"`).toContain(f);
    }
  });

  it('documents the before op', () => {
    expect(OQL_SCHEMA_DOC.predicates.field).toContain('before');
  });

  it('documents fetch.tree options', () => {
    for (const o of [
      'includeSizes',
      'filesOnly',
      'directoriesOnly',
      'sortBy',
    ]) {
      expect(doc, `tree option "${o}"`).toContain(o);
    }
  });

  it('documents scope.minDepth', () => {
    expect(OQL_SCHEMA_DOC.query.scope).toContain('minDepth');
  });

  it('documents materialize.strategy values', () => {
    for (const s of ['subtree', 'repo']) {
      expect(OQL_SCHEMA_DOC.query.materialize, `strategy "${s}"`).toContain(s);
    }
  });
});

describe('audit #12: explicit limit is a hard cap on the primary result domain', () => {
  it('caps a paged file listing to `limit` rows and flags more remain', async () => {
    const dir = path.resolve(__dirname, '../../src/oql');
    const env = single(
      await runOqlSearch({
        target: 'files',
        from: { kind: 'local', path: dir },
        limit: 2,
      })
    );
    expect(env.results.length).toBe(2);
    expect(env.pagination?.hasMore).toBe(true);
  });

  it('does not invent next.page for local code row limits', async () => {
    const dir = path.resolve(__dirname, '../../src/oql');
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: dir },
        where: { kind: 'text', value: 'OqlQuery' },
        limit: 2,
      })
    );
    expect(env.results.length).toBe(2);
    expect(env.next?.['next.page']).toBeUndefined();
  });

  it('does not keep hasMore true on an exhausted local code page with a row limit', async () => {
    const dir = path.resolve(__dirname, '../../src/oql');
    const first = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: dir },
        where: { kind: 'text', value: 'OqlQuery' },
        view: 'discovery',
        limit: 2,
        page: 1,
      })
    );
    const lastPage = first.pagination?.totalPages ?? 1;
    const env = single(
      await runOqlSearch({
        target: 'code',
        from: { kind: 'local', path: dir },
        where: { kind: 'text', value: 'OqlQuery' },
        view: 'discovery',
        limit: 2,
        page: lastPage,
      })
    );
    expect(env.pagination?.currentPage).toBe(lastPage);
    expect(env.pagination?.hasMore).toBe(false);
    expect(env.next?.['next.page']).toBeUndefined();
  });
});
