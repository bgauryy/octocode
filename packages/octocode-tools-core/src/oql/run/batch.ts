/** Bounded batch (1-5) execution: independent envelopes, or a merged one. */
import { diagnostic } from '../diagnostics.js';
import { buildEnvelope } from '../envelope.js';
import type {
  OqlBatch,
  OqlBatchResultEnvelope,
  OqlContinuationHint,
  OqlResultEnvelope,
} from '../types.js';
import { hintsEqual } from './continuations/registry.js';
import { runSingle, stripUniformSource, type RunOptions } from './single.js';

export async function runBatch(
  batch: OqlBatch,
  rawInput: unknown,
  options: RunOptions
): Promise<OqlBatchResultEnvelope> {
  const children = await Promise.all(
    batch.queries.map(async (q, i) => {
      const envelope = await runSingle(q, rawInput, options, i);
      return {
        queryId: q.id ?? `q${i}`,
        queryIndex: i,
        envelope,
      };
    })
  );

  const result: OqlBatchResultEnvelope = {
    ...(batch.id ? { batchId: batch.id } : {}),
    mode: batch.combine ?? 'independent',
    children,
    diagnostics: [],
  };

  if (batch.combine === 'merge') {
    const merged = mergeChildren(children);
    if (merged.error) {
      result.diagnostics.push(merged.error);
    } else if (merged.envelope) {
      result.merged = merged.envelope;
    }
    // mergeChildren reads source for rowKey dedup, then references the SAME row
    // objects in merged.results. Strip only the merged envelope (multi-source →
    // not uniform → source kept). Stripping children would mutate those shared
    // rows and wrongly drop source from a cross-source merge.
    if (result.merged) stripUniformSource(result.merged.results);
  } else {
    // Independent children: each is single-source; drop its redundant per-row
    // source (no shared refs, no merge dedup to preserve).
    for (const c of children) stripUniformSource(c.envelope.results);
  }

  return result;
}

function mergeChildren(children: OqlBatchResultEnvelope['children']): {
  envelope?: OqlResultEnvelope;
  error?: OqlResultEnvelope['diagnostics'][number];
} {
  // Rows are compatible only when every child shares the same row kind.
  const kinds = new Set<string>();
  for (const c of children) {
    for (const r of c.envelope.results) kinds.add(r.kind);
  }
  if (kinds.size > 1) {
    return {
      error: {
        code: 'invalidQuery',
        severity: 'error',
        message:
          'combine:"merge" requires compatible rows (same target/result kind); use combine:"independent".',
        blocksAnswer: true,
        repair: {
          message: 'Set combine:"independent" to keep per-query envelopes.',
        },
      },
    };
  }

  const seen = new Set<string>();
  const results = [];
  const diagnostics = [];
  const provenance = [];
  const nextHints: Record<string, OqlContinuationHint> = {};
  const shared: Record<string, unknown> = {};
  let approximate = false;
  let anyOpenPages = false;
  for (const c of children) {
    for (const r of c.envelope.results) {
      const key = rowKey(r);
      if (seen.has(key)) continue;
      seen.add(key);
      results.push(r);
    }
    diagnostics.push(...c.envelope.diagnostics);
    provenance.push(...c.envelope.provenance);
    mergeNextHints(nextHints, c.envelope.nextHints);
    mergeShared(shared, c.envelope.shared);
    if (c.envelope.evidence.kind === 'candidate') approximate = true;
    if (childHasOpenPages(c.envelope)) anyOpenPages = true;
  }

  // A merged batch carries no single continuation cursor, so child pagination
  // would otherwise be lost and the merged result could falsely read as
  // complete. Surface the open pages on the envelope (so hasOpenPages trips →
  // partial/not-complete) and point the agent at per-query paging.
  if (anyOpenPages) {
    diagnostics.push(
      diagnostic(
        'partialResult',
        'combine:"merge" has child queries with more pages remaining; a merged batch carries no single continuation cursor — page each query with combine:"independent" to reach completeness.',
        { severity: 'info', blocksAnswer: false }
      )
    );
  }

  return {
    envelope: buildEnvelope({
      results,
      ...(Object.keys(shared).length ? { shared } : {}),
      ...(anyOpenPages ? { pagination: { hasMore: true } } : {}),
      ...(Object.keys(nextHints).length ? { nextHints } : {}),
      diagnostics,
      provenance,
      executable: children.every(
        c => c.envelope.evidence.kind !== 'unsupported'
      ),
      approximate,
    }),
  };
}

function mergeNextHints(
  target: Record<string, OqlContinuationHint>,
  source: Record<string, OqlContinuationHint> | undefined
): void {
  if (!source) return;
  for (const [key, hint] of Object.entries(source)) {
    const existing = target[key];
    if (!existing || hintsEqual(existing, hint)) {
      target[key] = hint;
    }
  }
}

function mergeShared(
  target: Record<string, unknown>,
  source: Record<string, unknown> | undefined
): void {
  if (!source) return;
  for (const [key, value] of Object.entries(source)) {
    const existing = target[key];
    if (
      existing === undefined ||
      JSON.stringify(existing) === JSON.stringify(value)
    ) {
      target[key] = value;
    }
  }
}

/** Mirror of envelope.hasOpenPages for a child envelope. */
function childHasOpenPages(env: OqlResultEnvelope): boolean {
  if (env.pagination?.hasMore) return true;
  if (env.next && Object.keys(env.next).some(k => k.startsWith('next.page'))) {
    return true;
  }
  return false;
}

function rowKey(r: OqlResultEnvelope['results'][number]): string {
  const path = (r as { path?: string }).path ?? '';
  const line = (r as { line?: number }).line ?? '';
  const src = JSON.stringify((r as { source?: unknown }).source ?? {});
  return `${r.kind}:${src}:${path}:${line}`;
}
