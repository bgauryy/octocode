/**
 * P5 (Option A) — `target:"research"` stays candidate-grade but must emit a
 * one-call `next.graph` upgrade: a pre-filled `proof:"lsp"` graph query, bounded
 * by proofLimit and page-aligned, so an agent can escalate to LSP proof in a
 * single follow-up run without research itself running LSP.
 */
import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { runOqlSearch } from '../../src/oql/run.js';
import { isBatchEnvelope, type OqlInputQuery } from '../../src/oql/types.js';

const OQL_SRC = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../src/oql'
);

function single(r: Awaited<ReturnType<typeof runOqlSearch>>) {
  if (isBatchEnvelope(r)) throw new Error('expected single');
  return r;
}

describe('research mode:"prove" one-call LSP upgrade (P5)', () => {
  it('emits an executable next.graph proof:"lsp" upgrade on the research row', async () => {
    const env = single(
      await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: OQL_SRC },
        params: { goal: 'dead code', intent: 'reachability', mode: 'prove' },
        itemsPerPage: 4,
      })
    );
    // research never claims proof — it is candidate-grade.
    expect(env.evidence.kind).not.toBe('proof');

    const row = env.results[0] as {
      next?: Record<string, { query: OqlInputQuery; confidence: string }>;
    };
    const upgrade = row.next?.['next.graph'];
    expect(upgrade).toBeDefined();
    const q = upgrade!.query as OqlInputQuery;
    expect(q.target).toBe('graph');
    expect(q.params).toMatchObject({
      mode: 'prove',
      proof: 'lsp',
      intent: 'reachability',
    });
    // proofLimit is bounded (graphParams caps at 25) and page-aligned.
    expect((q.params as { proofLimit: number }).proofLimit).toBeLessThanOrEqual(
      25
    );
    expect((q.params as { proofLimit: number }).proofLimit).toBe(4);
    expect(q.itemsPerPage).toBe(4);
    expect(upgrade!.confidence).toBe('exact');
  });

  it('emits an executable next.graph proof:"lsp" upgrade on candidate graph rows', async () => {
    const env = single(
      await runOqlSearch({
        target: 'graph',
        from: { kind: 'local', path: OQL_SRC },
        params: { goal: 'dead-looking symbols', intent: 'reachability' },
        itemsPerPage: 3,
      })
    );

    const row = env.results[0] as {
      proofGrade?: string;
      next?: Record<string, { query: OqlInputQuery; confidence: string }>;
    };
    expect(row.proofGrade).toBe('missing');
    const upgrade = row.next?.['next.graph'];
    expect(upgrade).toBeDefined();
    const q = upgrade!.query as OqlInputQuery;
    expect(q.target).toBe('graph');
    expect(q.params).toMatchObject({
      mode: 'prove',
      proof: 'lsp',
      intent: 'reachability',
    });
    expect((q.params as { proofLimit: number }).proofLimit).toBe(3);
    expect(q.itemsPerPage).toBe(3);
    expect(upgrade!.confidence).toBe('exact');
  });
});
