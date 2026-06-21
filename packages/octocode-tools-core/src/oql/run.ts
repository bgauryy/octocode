/**
 * OQL runner — the single entry point behind `octocode search`.
 *
 *   normalize -> plan -> (explain) -> execute via adapter -> envelope
 *
 * Handles single queries and bounded batches (1-5). `--explain` includes the
 * plan; `--dry-run` returns the plan without executing.
 */
import path from 'node:path';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { normalizeInput } from './normalize.js';
import { planQuery, type PlanQueryResult } from './planner.js';
import { OqlValidationError } from './diagnostics.js';
import {
  backendsApproximate,
  buildEnvelope,
  unsupportedEnvelope,
} from './envelope.js';
import { executeLocal, type AdapterResult } from './adapters/local.js';
import { executeGithub } from './adapters/github.js';
import { executeMaterialize } from './adapters/materialize.js';
import { V2_ADAPTERS } from './adapters/v2.js';
import {
  isCanonicalBatch,
  type OqlBatchResultEnvelope,
  type OqlBatchV1,
  type OqlCodeResultRow,
  type OqlContinuation,
  type OqlContentResultRow,
  type OqlQueryV1,
  type OqlResultEnvelope,
  type OqlRunResult,
  type OqlSearchInputV1,
} from './types.js';

export interface RunOptions {
  authInfo?: AuthInfo;
  /** Plan only; do not execute. Maps to `octocode search --dry-run`. */
  dryRun?: boolean;
}

export async function runOqlSearch(
  input: OqlSearchInputV1,
  options: RunOptions = {}
): Promise<OqlRunResult> {
  let canonical;
  try {
    canonical = normalizeInput(input);
  } catch (err) {
    if (err instanceof OqlValidationError) {
      return unsupportedEnvelope(err.diagnostics);
    }
    throw err;
  }

  if (isCanonicalBatch(canonical)) {
    return runBatch(canonical, input, options);
  }
  return runSingle(canonical, input, options);
}

async function runSingle(
  query: OqlQueryV1,
  rawInput: unknown,
  options: RunOptions,
  queryIndex?: number
): Promise<OqlResultEnvelope> {
  const planned = planQuery(query, rawInput);
  const includePlan = Boolean(query.explain) || Boolean(options.dryRun);
  const plan = includePlan ? planned.plan : undefined;

  // Not executable, or explicitly a dry run: return without executing.
  if (!planned.executable || options.dryRun) {
    return unsupportedEnvelopeFromPlan(planned, plan, query.id, queryIndex);
  }

  const exec = await dispatch(query, planned);
  relativizeResultPaths(query, exec.results);
  const next = attachContinuations(query, exec);

  return buildEnvelope({
    queryId: query.id,
    queryIndex,
    results: exec.results,
    ...(exec.pagination ? { pagination: exec.pagination } : {}),
    ...(Object.keys(next).length ? { next } : {}),
    diagnostics: [...planned.plan.diagnostics, ...exec.diagnostics],
    provenance: exec.provenance,
    executable: true,
    approximate: backendsApproximate(planned.plan.backendCalls),
    plan,
  });
}

/**
 * Emit executable `next.*` continuations (contract Gate 10). Every continuation
 * is a full canonical OQL query runnable as-is:
 *  - next.page      — more result pages remain
 *  - next.matchPage — per-file matches were capped
 *  - row.next.fetch — read a code hit's exact content
 *  - row.next.charRange — page a large content body
 */
function attachContinuations(
  query: OqlQueryV1,
  exec: AdapterResult
): Record<string, OqlContinuation> {
  const next: Record<string, OqlContinuation> = {};

  if (exec.pagination?.hasMore) {
    next['next.page'] = {
      query: { ...query, page: (query.page ?? 1) + 1 },
      why: 'More result pages remain.',
      confidence: 'exact',
    };
  }

  if (exec.diagnostics.some(d => d.code === 'matchTruncated')) {
    next['next.matchPage'] = {
      query: {
        ...query,
        controls: {
          ...query.controls,
          search: {
            ...query.controls?.search,
            matchPage: (query.controls?.search?.matchPage ?? 1) + 1,
          },
        },
      },
      why: 'Per-file matches were capped; page within files.',
      confidence: 'exact',
    };
  }

  // Per-row continuations.
  const fileFrom = localFileSource(query);
  for (const row of exec.results) {
    if (row.kind === 'code') {
      const code = row as OqlCodeResultRow;
      const from = fileFrom ? fileFrom(code.path) : (code.source ?? query.from);
      if (!from) continue;
      const range =
        typeof code.line === 'number'
          ? { startLine: code.line, contextLines: 2 }
          : undefined;
      code.next = {
        'next.fetch': {
          query: {
            schema: 'oql/v1',
            target: 'content',
            from,
            ...(fileFrom ? {} : { scope: { path: code.path } }),
            fetch: {
              content: { contentView: 'exact', ...(range ? { range } : {}) },
            },
          },
          why: 'Read the exact content at this hit.',
          confidence: 'exact',
        },
      };
    } else if (row.kind === 'content') {
      const content = row as OqlContentResultRow;
      const off = content.range?.charOffset;
      if (typeof off === 'number') {
        content.next = {
          'next.charRange': {
            query: {
              ...query,
              fetch: {
                ...query.fetch,
                content: {
                  ...query.fetch?.content,
                  charOffset: off + (content.range?.charLength ?? 20000),
                },
              },
            },
            why: 'Read the next content window.',
            confidence: 'exact',
          },
        };
      }
    }
  }
  return next;
}

/**
 * For local/materialized sources, return a builder that turns a relativized
 * row path back into an absolute-file `from` (relativization stripped exactly
 * `dirname(resolve(root)) + '/'`, so re-adding it round-trips).
 */
function localFileSource(
  query: OqlQueryV1
): ((rowPath: string) => OqlQueryV1['from']) | undefined {
  const root =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;
  if (!root) return undefined;
  const base = path.dirname(path.resolve(root));
  return (rowPath: string) => ({
    kind: 'local',
    path: path.isAbsolute(rowPath) ? rowPath : path.join(base, rowPath),
  });
}

function unsupportedEnvelopeFromPlan(
  planned: PlanQueryResult,
  plan: OqlResultEnvelope['plan'],
  queryId?: string,
  queryIndex?: number
): OqlResultEnvelope {
  if (!planned.executable) {
    return unsupportedEnvelope(
      planned.plan.diagnostics,
      plan,
      queryId,
      queryIndex
    );
  }
  // dry run of an executable query: report plan, evidence partial (not executed)
  return {
    ...(queryId ? { queryId } : {}),
    ...(queryIndex !== undefined ? { queryIndex } : {}),
    results: [],
    diagnostics: planned.plan.diagnostics,
    provenance: [],
    evidence: { answerReady: false, complete: false, kind: 'partial' },
    ...(plan ? { plan } : {}),
  };
}

/**
 * Relativize absolute local result paths to the query root's parent, matching
 * the relativization the raw tools/CLI apply (e.g. `/…/src/oql/x.ts` ->
 * `oql/x.ts`). Keeps `search` aligned with grep/ls/find and far less verbose.
 * Provider (GitHub) paths are already repo-relative and left untouched.
 */
function relativizeResultPaths(
  query: OqlQueryV1,
  results: OqlResultEnvelope['results']
): void {
  const root =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;
  if (!root) return;
  const abs = path.resolve(root);
  const prefix = `${path.dirname(abs)}/`;
  for (const row of results) {
    const p = (row as { path?: string }).path;
    if (typeof p === 'string' && p.startsWith(prefix)) {
      (row as { path: string }).path = p.slice(prefix.length);
    }
  }
}

/** Choose the execution lane from the plan. */
async function dispatch(
  query: OqlQueryV1,
  planned: PlanQueryResult
): Promise<AdapterResult> {
  // V2 research targets each own their lane (incl. semantics' internal
  // materialize-for-remote); route by target first.
  const v2 = V2_ADAPTERS[query.target];
  if (v2) return v2(query);

  if (query.from?.kind === 'local' || query.from?.kind === 'materialized') {
    return executeLocal(query);
  }
  // GitHub source: route to materialization when any predicate needs local
  // proof or materialization is required.
  const needsMaterialize =
    planned.plan.nodes.some(n => n.route === 'ROUTE') ||
    planned.plan.materialization?.required === true ||
    query.materialize?.mode === 'required';
  if (needsMaterialize) {
    return executeMaterialize(query);
  }
  return executeGithub(query);
}

/* ------------------------------- batch ---------------------------------- */

async function runBatch(
  batch: OqlBatchV1,
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
  let approximate = false;
  for (const c of children) {
    for (const r of c.envelope.results) {
      const key = rowKey(r);
      if (seen.has(key)) continue;
      seen.add(key);
      results.push(r);
    }
    diagnostics.push(...c.envelope.diagnostics);
    provenance.push(...c.envelope.provenance);
    if (c.envelope.evidence.kind === 'candidate') approximate = true;
  }

  return {
    envelope: buildEnvelope({
      results,
      diagnostics,
      provenance,
      executable: children.every(
        c => c.envelope.evidence.kind !== 'unsupported'
      ),
      approximate,
    }),
  };
}

function rowKey(r: OqlResultEnvelope['results'][number]): string {
  const path = (r as { path?: string }).path ?? '';
  const line = (r as { line?: number }).line ?? '';
  const src = JSON.stringify((r as { source?: unknown }).source ?? {});
  return `${r.kind}:${src}:${path}:${line}`;
}
