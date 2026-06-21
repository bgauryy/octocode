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
import {
  executeMaterialize,
  executeMaterializeCheckpoint,
} from './adapters/materialize.js';
import { V2_ADAPTERS } from './adapters/v2.js';
import {
  isCanonicalBatch,
  type OqlBatchResultEnvelope,
  type OqlBatchV1,
  type OqlCodeResultRow,
  type OqlContinuation,
  type OqlContentResultRow,
  type OqlQueryV1,
  type OqlRecordResultRow,
  type OqlResultEnvelope,
  type OqlResultRow,
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
 * is a full canonical OQL query runnable as-is.
 *
 * Envelope-level:
 *  - next.page      — more result pages remain
 *  - next.matchPage — per-file matches were capped
 *
 * Per-row continuations are produced by a registry keyed by row kind (and, for
 * record rows, recordType) so adding a new row's continuations is one entry,
 * never another `else if`:
 *  - code        → next.fetch (read exact content) [+ next.semantic on local]
 *  - content     → next.charRange (page the body)
 *  - artifact    → next.structure / next.files rooted at the extracted path
 *  - materialized→ next.structure / next.files rooted at the checkpoint path
 *  - semantics   → next.fetch (read the code at a symbol location)
 */
interface ContinuationCtx {
  query: OqlQueryV1;
  /** code rows: rebuild an absolute `from` from a relativized row path. */
  fileFrom?: (rowPath: string) => OqlQueryV1['from'];
}

type RowContinuationBuilder = (
  row: OqlResultRow,
  ctx: ContinuationCtx
) => Record<string, OqlContinuation> | undefined;

function attachContinuations(
  query: OqlQueryV1,
  exec: AdapterResult
): Record<string, OqlContinuation> {
  const next: Record<string, OqlContinuation> = {};

  // Content reads page the char-window domain, not the result-row domain. The
  // per-row `next.charRange` is the executable continuation there, so never
  // emit a misleading `next.page` for target:"content".
  if (exec.pagination?.hasMore && query.target !== 'content') {
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

  // Per-row continuations via the registry.
  const ctx: ContinuationCtx = { query, fileFrom: localFileSource(query) };
  for (const row of exec.results) {
    const key =
      row.kind === 'record'
        ? `record:${(row as OqlRecordResultRow).recordType}`
        : row.kind;
    const build = ROW_CONTINUATION_BUILDERS[key];
    if (!build) continue;
    const rowNext = build(row, ctx);
    if (rowNext && Object.keys(rowNext).length) {
      (row as { next?: Record<string, OqlContinuation> }).next = rowNext;
    }
  }
  return next;
}

const ROW_CONTINUATION_BUILDERS: Record<string, RowContinuationBuilder> = {
  code: buildCodeContinuations,
  content: buildContentContinuations,
  'record:artifact': buildArtifactContinuations,
  'record:materialized': buildMaterializedContinuations,
  'record:semantics': buildSemanticsContinuations,
};

function buildCodeContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const code = row as OqlCodeResultRow;
  const from = ctx.fileFrom
    ? ctx.fileFrom(code.path)
    : (code.source ?? ctx.query.from);
  if (!from) return undefined;
  const range =
    typeof code.line === 'number'
      ? { startLine: code.line, contextLines: 2 }
      : undefined;
  const out: Record<string, OqlContinuation> = {
    'next.fetch': {
      query: {
        schema: 'oql/v1',
        target: 'content',
        from,
        ...(ctx.fileFrom ? {} : { scope: { path: code.path } }),
        fetch: {
          content: { contentView: 'exact', ...(range ? { range } : {}) },
        },
      },
      why: 'Read the exact content at this hit.',
      confidence: 'exact',
    },
  };
  // Semantic outline of the file. Local/materialized only: this is always
  // executable from the file anchor; a remote semantic would re-clone per hit.
  if (ctx.fileFrom) {
    out['next.semantic'] = {
      query: {
        schema: 'oql/v1',
        target: 'semantics',
        from,
        params: { type: 'documentSymbols' },
      },
      why: 'List the semantic symbols in this file.',
      confidence: 'exact',
    };
  }
  return out;
}

function buildContentContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const content = row as OqlContentResultRow;
  const off = content.range?.charOffset;
  if (typeof off !== 'number') return undefined;
  return {
    'next.charRange': {
      query: {
        ...ctx.query,
        fetch: {
          ...ctx.query.fetch,
          content: {
            ...ctx.query.fetch?.content,
            charOffset: off + (content.range?.charLength ?? 20000),
          },
        },
      },
      why: 'Read the next content window.',
      confidence: 'exact',
    },
  };
}

/** next.structure / next.files rooted at a derived local path. */
function localRootContinuations(
  localPath: string,
  label: string
): Record<string, OqlContinuation> {
  const from = { kind: 'local' as const, path: localPath };
  return {
    'next.structure': {
      query: { schema: 'oql/v1', target: 'structure', from },
      why: `List the ${label} tree.`,
      confidence: 'exact',
    },
    'next.files': {
      query: { schema: 'oql/v1', target: 'files', from },
      why: `Enumerate files in the ${label}.`,
      confidence: 'exact',
    },
  };
}

function derivedLocalPath(row: OqlResultRow): string | undefined {
  const data = (row as OqlRecordResultRow).data;
  return typeof data?.localPath === 'string' ? data.localPath : undefined;
}

function buildArtifactContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const out: Record<string, OqlContinuation> = {};
  const lp = derivedLocalPath(row);
  if (lp) Object.assign(out, localRootContinuations(lp, 'extracted'));

  // Binary `strings` scan cursor: nextScanOffset → next scan window (a typed
  // per-domain continuation instead of a raw params round-trip).
  const data = (row as OqlRecordResultRow).data;
  const nextScan =
    typeof data?.nextScanOffset === 'number' ? data.nextScanOffset : undefined;
  if (nextScan !== undefined) {
    out['next.artifactStrings'] = {
      query: {
        ...ctx.query,
        params: { ...(ctx.query.params ?? {}), scanOffset: nextScan },
      },
      why: 'Scan the next window of printable strings.',
      confidence: 'exact',
    };
  }
  return Object.keys(out).length ? out : undefined;
}

function buildMaterializedContinuations(
  row: OqlResultRow
): Record<string, OqlContinuation> | undefined {
  const lp = derivedLocalPath(row);
  return lp ? localRootContinuations(lp, 'materialized') : undefined;
}

function buildSemanticsContinuations(
  row: OqlResultRow
): Record<string, OqlContinuation> | undefined {
  const data = (row as OqlRecordResultRow).data;
  const uri = typeof data?.uri === 'string' ? data.uri : undefined;
  if (!uri) return undefined;
  const line =
    typeof data.line === 'number'
      ? data.line
      : typeof data.startLine === 'number'
        ? data.startLine
        : undefined;
  return {
    'next.fetch': {
      query: {
        schema: 'oql/v1',
        target: 'content',
        from: { kind: 'local', path: uri },
        fetch: {
          content: {
            contentView: 'exact',
            ...(line ? { range: { startLine: line, contextLines: 2 } } : {}),
          },
        },
      },
      why: 'Read the code at this symbol location.',
      confidence: 'exact',
    },
  };
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
  // Addressable materialization: clone/cache once, return a checkpoint row.
  if (query.target === 'materialize') {
    return executeMaterializeCheckpoint(query);
  }

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
