/**
 * P1 — `target:"research" view:"detailed"` must page each domain instead of
 * inlining whole arrays. Each requested domain returns a window (≤ itemsPerPage)
 * plus a typed `<domain>Page`; a narrow `select` drops the other domains; and a
 * single `next.page` advances all detailed domains together.
 */
import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { runOqlSearch } from '../../src/oql/run.js';
import { isBatchEnvelope } from '../../src/oql/types.js';

const OQL_SRC = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../src/oql'
);

function single(r: Awaited<ReturnType<typeof runOqlSearch>>) {
  if (isBatchEnvelope(r)) throw new Error('expected single');
  return r;
}

const DOMAINS = ['manifests', 'files', 'dependencies', 'symbols', 'graphFacts'];

describe('research view:"detailed" per-domain paging (P1)', () => {
  it('windows every domain to itemsPerPage and carries a per-domain page', async () => {
    const itemsPerPage = 3;
    const env = single(
      await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: OQL_SRC },
        view: 'detailed',
        params: {
          goal: 'what is here',
          mode: 'analyze',
          facets: ['symbols', 'files', 'dependencies', 'relations'],
        },
        itemsPerPage,
      })
    );
    const data = (env.results[0] as { data: Record<string, unknown> }).data;

    for (const domain of DOMAINS) {
      const window = data[domain] as unknown[] | undefined;
      const page = data[`${domain}Page`] as
        | { itemsPerPage?: number; totalPages?: number; hasMore?: boolean; totalItems?: number }
        | undefined;
      expect(Array.isArray(window)).toBe(true);
      expect(window!.length).toBeLessThanOrEqual(itemsPerPage);
      expect(page).toBeDefined();
      expect(page!.itemsPerPage).toBe(itemsPerPage);
      // totalPages must be consistent with totalItems / itemsPerPage.
      expect(page!.totalPages).toBe(
        Math.max(1, Math.ceil((page!.totalItems ?? 0) / itemsPerPage))
      );
    }
  });

  it('emits next.page when any detailed domain has more pages', async () => {
    const env = single(
      await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: OQL_SRC },
        view: 'detailed',
        params: {
          goal: 'what is here',
          mode: 'analyze',
          facets: ['symbols', 'files', 'dependencies', 'relations'],
        },
        itemsPerPage: 1,
      })
    );
    const data = (env.results[0] as { data: Record<string, unknown> }).data;
    const anyMore = DOMAINS.some(
      d =>
        (data[`${d}Page`] as { hasMore?: boolean } | undefined)?.hasMore === true
    );
    if (anyMore) {
      expect(env.pagination?.hasMore).toBe(true);
      expect(env.next?.['next.page']).toBeDefined();
    }
  });

  it('honors a narrow select — drops unrequested domains', async () => {
    const env = single(
      await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: OQL_SRC },
        view: 'detailed',
        select: ['symbols'],
        params: {
          goal: 'what is here',
          mode: 'analyze',
          facets: ['symbols', 'files', 'dependencies', 'relations'],
        },
        itemsPerPage: 5,
      })
    );
    const data = (env.results[0] as { data: Record<string, unknown> }).data;
    // requested domain survives...
    expect(data.symbols).toBeDefined();
    expect(data.symbolsPage).toBeDefined();
    // ...unrequested detailed domains are dropped.
    expect(data.files).toBeUndefined();
    expect(data.dependencies).toBeUndefined();
    expect(data.graphFacts).toBeUndefined();
    // bare domain selector must NOT raise an unknownField diagnostic.
    expect(env.diagnostics.some(d => d.code === 'unknownField')).toBe(false);
  });
});
