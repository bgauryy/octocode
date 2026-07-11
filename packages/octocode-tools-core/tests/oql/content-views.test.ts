import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { runOqlSearch } from '../../src/oql/run.js';
import {
  isBatchEnvelope,
  type OqlContentResultRow,
  type OqlContinuation,
} from '../../src/oql/types.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const OQL_SRC = path.resolve(here, '../../src/oql');
const TYPES = path.join(OQL_SRC, 'types.ts');

function single(r: Awaited<ReturnType<typeof runOqlSearch>>) {
  if (isBatchEnvelope(r)) throw new Error('expected single');
  return r;
}
async function readContent(contentView: 'none' | 'standard' | 'symbols') {
  return single(
    await runOqlSearch({
      target: 'content',
      from: { kind: 'local', path: TYPES },
      fetch: { content: { contentView } },
    })
  ).results[0] as OqlContentResultRow;
}

describe('OQL content view labelling (contract: report the view used)', () => {
  it('symbols request -> row.contentView === "symbols"', async () => {
    const row = await readContent('symbols');
    expect(row.contentView).toBe('symbols');
  });
  it('none request -> row.contentView === "none"', async () => {
    const row = await readContent('none');
    expect(row.contentView).toBe('none');
  });
  it('standard request -> row.contentView === "standard"', async () => {
    const row = await readContent('standard');
    expect(row.contentView).toBe('standard');
  });
});

describe('OQL content fetch capabilities: match string + context lines', () => {
  it('match string anchors the read to the matched region', async () => {
    const env = single(
      await runOqlSearch({
        target: 'content',
        from: { kind: 'local', path: TYPES },
        fetch: {
          content: {
            match: { text: 'OqlContinuation' },
            range: { contextLines: 1 },
            contentView: 'none',
          },
        },
      })
    );
    const row = env.results[0] as OqlContentResultRow;
    expect(row.kind).toBe('content');
    // The anchored window must actually contain the matched token.
    expect(row.content).toContain('OqlContinuation');
  });

  it('context lines widen a match-anchored read', async () => {
    // contextLines applies around a match anchor (not an explicit line range,
    // where the range itself is the exact window).
    const tight = single(
      await runOqlSearch({
        target: 'content',
        from: { kind: 'local', path: TYPES },
        fetch: {
          content: {
            match: { text: 'OqlContinuation' },
            range: { contextLines: 0 },
            contentView: 'none',
          },
        },
      })
    ).results[0] as OqlContentResultRow;
    const wide = single(
      await runOqlSearch({
        target: 'content',
        from: { kind: 'local', path: TYPES },
        fetch: {
          content: {
            match: { text: 'OqlContinuation' },
            range: { contextLines: 4 },
            contentView: 'none',
          },
        },
      })
    ).results[0] as OqlContentResultRow;
    expect(wide.content.length).toBeGreaterThan(tight.content.length);
  });
});

describe('OQL content char-window pagination -> next.charRange (not next.page)', () => {
  it('windowed read carries the char range and a next.charRange continuation', async () => {
    const env = single(
      await runOqlSearch({
        target: 'content',
        from: { kind: 'local', path: TYPES },
        fetch: {
          content: { contentView: 'none', charOffset: 0, charLength: 200 },
        },
      })
    );
    const row = env.results[0] as OqlContentResultRow & {
      next?: Record<string, OqlContinuation>;
    };
    // The row must report the char window it read.
    expect(row.range?.charOffset).toBe(0);
    expect(typeof row.range?.charLength).toBe('number');
    expect(row.range!.charLength!).toBeGreaterThan(0);

    // Content pagination is the char domain, never next.page.
    expect(env.next?.['next.page']).toBeUndefined();

    // A next.charRange continuation must advance the offset.
    const cr = row.next?.['next.charRange'];
    expect(cr).toBeDefined();
    expect(cr!.query.target).toBe('content');
    const nextOffset = cr!.query.fetch?.content?.charOffset;
    expect(typeof nextOffset).toBe('number');
    expect(nextOffset!).toBeGreaterThan(0);
  });
});
