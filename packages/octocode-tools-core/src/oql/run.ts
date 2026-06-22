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
import { OqlValidationError, diagnostic } from './diagnostics.js';
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
import { RESEARCH_TARGET_ADAPTERS } from './adapters/researchTargets.js';
import {
  isCanonicalBatch,
  type OqlBatchResultEnvelope,
  type OqlBatch,
  type OqlCodeResultRow,
  type OqlContinuation,
  type OqlContentResultRow,
  type OqlQuery,
  type OqlRecordResultRow,
  type OqlResultEnvelope,
  type OqlResultRow,
  type OqlRunResult,
  type OqlSearchInput,
} from './types.js';

export interface RunOptions {
  authInfo?: AuthInfo;
  /** Plan only; do not execute. Maps to `octocode search --dry-run`. */
  dryRun?: boolean;
}

export async function runOqlSearch(
  input: OqlSearchInput,
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
  query: OqlQuery,
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
  applyResultRowWindow(query, exec);
  const next = attachContinuations(query, exec);

  // select: project row fields + continuations (projection only — never changes
  // result domains or triggers fetches). Unknown fields are reported, not fatal.
  const projectionDiagnostics = applySelect(query, exec.results);

  return buildEnvelope({
    queryId: query.id,
    queryIndex,
    results: exec.results,
    ...(exec.pagination ? { pagination: exec.pagination } : {}),
    ...(Object.keys(next).length ? { next } : {}),
    diagnostics: [
      ...planned.plan.diagnostics,
      ...exec.diagnostics,
      ...projectionDiagnostics,
    ],
    provenance: exec.provenance,
    executable: true,
    approximate: backendsApproximate(planned.plan.backendCalls),
    plan,
  });
}

function applyResultRowWindow(query: OqlQuery, exec: AdapterResult): void {
  // Content has its own char-window pagination and per-row next.charRange.
  if (query.target === 'content') return;

  const cap =
    typeof query.limit === 'number'
      ? query.limit
      : typeof query.itemsPerPage === 'number'
        ? query.itemsPerPage
        : undefined;
  if (!cap || cap < 1 || exec.results.length <= cap) return;

  const totalItems = exec.pagination?.totalItems ?? exec.results.length;
  const currentPage = exec.pagination?.currentPage ?? query.page ?? 1;
  exec.results = exec.results.slice(0, cap);
  exec.pagination = {
    ...exec.pagination,
    currentPage,
    itemsPerPage: exec.pagination?.itemsPerPage ?? cap,
    totalItems,
    totalPages:
      exec.pagination?.totalPages ?? Math.max(1, Math.ceil(totalItems / cap)),
    hasMore: true,
  };
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
  query: OqlQuery;
  /** code rows: rebuild an absolute `from` from a relativized row path. */
  fileFrom?: (rowPath: string) => OqlQuery['from'];
}

type RowContinuationBuilder = (
  row: OqlResultRow,
  ctx: ContinuationCtx
) => Record<string, OqlContinuation> | undefined;

function contentMatchFromQuery(
  query: OqlQuery
): NonNullable<NonNullable<OqlQuery['fetch']>['content']>['match'] | undefined {
  const where = query.where;
  if (!where) return undefined;
  if (where.kind === 'text') {
    return {
      text: where.value,
      ...(where.case === 'sensitive' ? { caseSensitive: true } : {}),
    };
  }
  if (where.kind === 'regex') {
    return {
      text: where.value,
      regex: true,
      ...(where.case === 'sensitive' ? { caseSensitive: true } : {}),
    };
  }
  return undefined;
}

function attachContinuations(
  query: OqlQuery,
  exec: AdapterResult
): Record<string, OqlContinuation> {
  const next: Record<string, OqlContinuation> = {};

  // Content reads page the char-window domain, not the result-row domain. The
  // per-row `next.charRange` is the executable continuation there, so never
  // emit a misleading `next.page` for target:"content".
  if (exec.pagination?.hasMore && query.target !== 'content') {
    next['next.page'] = exec.pagination.next ?? {
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
  'record:research': buildResearchContinuations,
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
  const match = range ? undefined : contentMatchFromQuery(ctx.query);
  const out: Record<string, OqlContinuation> = {
    'next.fetch': {
      query: {
        schema: 'oql',
        target: 'content',
        from,
        ...(ctx.fileFrom ? {} : { scope: { path: code.path } }),
        fetch: {
          content: {
            contentView: 'exact',
            ...(range ? { range } : {}),
            ...(match ? { match } : {}),
          },
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
        schema: 'oql',
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
      query: { schema: 'oql', target: 'structure', from },
      why: `List the ${label} tree.`,
      confidence: 'exact',
    },
    'next.files': {
      query: { schema: 'oql', target: 'files', from },
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

/**
 * P5 (Option A): `target:"research"` stays candidate-grade — it never runs LSP
 * internally — but it emits a *one-call* upgrade. `next.graph` is a pre-filled
 * `proof:"lsp"` graph query over the same root/intent, page-aligned and bounded
 * by `proofLimit`, so a single follow-up run turns the current page's candidate
 * packets into LSP-proven relationships without blurring the research/graph
 * honesty boundary.
 */
function buildResearchContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const from = ctx.query.from;
  // Graph proof needs a complete local file universe (local/materialized).
  if (from?.kind !== 'local' && from?.kind !== 'materialized') return undefined;
  const data = (row as OqlRecordResultRow).data;
  const intent =
    typeof data?.intent === 'string' && data.intent.length > 0
      ? data.intent
      : 'reachability';
  // proofLimit is bounded (graphParams caps at 25); align it to the page size so
  // the upgrade proves roughly the same number of subjects shown this page.
  const proofLimit = Math.min(25, Math.max(1, ctx.query.itemsPerPage ?? 10));
  return {
    'next.graph': {
      query: {
        schema: 'oql',
        target: 'graph',
        from,
        params: {
          mode: 'prove',
          proof: 'lsp',
          intent,
          proofLimit,
          ...(Array.isArray(data?.facets) ? { facets: data.facets } : {}),
        },
        ...(ctx.query.page ? { page: ctx.query.page } : {}),
        ...(ctx.query.itemsPerPage
          ? { itemsPerPage: ctx.query.itemsPerPage }
          : {}),
      },
      why: 'Upgrade this candidate research to LSP-proven relationships for the current page (bounded proof).',
      confidence: 'exact',
    },
  };
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
        schema: 'oql',
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
  query: OqlQuery
): ((rowPath: string) => OqlQuery['from']) | undefined {
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
  query: OqlQuery,
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

/* ------------------------------ select ---------------------------------- */

// Row identity always survives projection (needed to cite + continue).
const SELECT_ALWAYS_KEEP = new Set(['kind', 'source', 'recordType', 'id']);

// Projectable per-row fields across all row kinds.
const SELECTABLE_ROW_FIELDS = new Set([
  'path',
  'line',
  'endLine',
  'column',
  'snippet',
  'matchIndices',
  'metadata',
  'content',
  'contentView',
  'range',
  'metavars',
  'metavarRanges',
  'size',
  'modified',
  'entryType',
  'depth',
  'children',
  'data',
]);

// Record-data sub-domains (research/graph detailed payloads). A bare selector
// like "symbols" or "files" sub-projects WITHIN `data` — the research/graph
// adapter performs that projection — so here the token just keeps the carrying
// `data` field and never warns (P1: narrow `select` drops unrequested domains).
const RECORD_DATA_SUBFIELDS = new Set([
  'manifests',
  'files',
  'dependencies',
  'symbols',
  'graphFacts',
  'packets',
  'nodes',
  'edges',
  'facts',
]);

// Envelope-level select tokens: recognized, no per-row effect (the envelope
// always carries them). `repo`/`localPath` are identity carried by `source`.
const SELECT_ENVELOPE_TOKENS = new Set([
  'pagination',
  'diagnostics',
  'provenance',
  'evidence',
  'repo',
  'localPath',
]);

/**
 * Project result rows to the requested `select` fields. Projection only: it
 * filters which fields/continuations appear, never adds data or changes the
 * result domain. Identity fields always survive. Unknown selectors yield a
 * non-blocking `unknownField` diagnostic. Dotted record-data selectors
 * (e.g. `data.summary`) are accepted but not sub-projected (the whole `data`
 * stays if `data` is selected).
 */
function applySelect(
  query: OqlQuery,
  results: OqlResultRow[]
): OqlResultEnvelope['diagnostics'] {
  const select = query.select;
  if (!select || select.length === 0) return [];

  const nextKeys = new Set<string>();
  const rowFields = new Set<string>();
  let keepAllNext = false;
  const unknown: string[] = [];

  for (const raw of select) {
    const token = raw.trim();
    if (token === 'next') {
      keepAllNext = true;
    } else if (token.startsWith('next.')) {
      nextKeys.add(token);
    } else if (SELECTABLE_ROW_FIELDS.has(token)) {
      rowFields.add(token);
    } else if (RECORD_DATA_SUBFIELDS.has(token)) {
      // bare record-data sub-domain → keep `data`; adapter sub-projects it.
      rowFields.add('data');
    } else if (SELECT_ENVELOPE_TOKENS.has(token)) {
      // recognized envelope token — no row projection needed
    } else if (token.includes('.')) {
      // dotted record-data selector (e.g. packets.subject / data.summary):
      // keep the carrying field; do not sub-project.
      rowFields.add('data');
    } else {
      unknown.push(token);
    }
  }

  for (const row of results) {
    const r = row as unknown as Record<string, unknown>;
    for (const key of Object.keys(r)) {
      if (SELECT_ALWAYS_KEEP.has(key)) continue;
      if (key === 'next') {
        if (keepAllNext) continue;
        const next = r.next as Record<string, unknown> | undefined;
        if (!next) continue;
        if (nextKeys.size === 0) {
          delete r.next;
          continue;
        }
        for (const nk of Object.keys(next)) {
          if (!nextKeys.has(nk)) delete next[nk];
        }
        if (Object.keys(next).length === 0) delete r.next;
        continue;
      }
      if (!rowFields.has(key)) delete r[key];
    }
  }

  return unknown.length
    ? [
        diagnostic(
          'unknownField',
          `select contains unknown field(s): ${unknown.join(', ')}. They were ignored.`,
          { queryPath: 'select', severity: 'warning', blocksAnswer: false }
        ),
      ]
    : [];
}

/** Choose the execution lane from the plan. */
async function dispatch(
  query: OqlQuery,
  planned: PlanQueryResult
): Promise<AdapterResult> {
  // Addressable materialization: clone/cache once, return a checkpoint row.
  if (query.target === 'materialize') {
    return executeMaterializeCheckpoint(query);
  }

  // Research targets each own their lane (incl. semantics' internal
  // materialize-for-remote); route by target first.
  const targetAdapter = RESEARCH_TARGET_ADAPTERS[query.target];
  if (targetAdapter) return targetAdapter(query);

  if (query.from?.kind === 'local' || query.from?.kind === 'materialized') {
    return executeLocal(query);
  }
  // GitHub source: route to materialization when any predicate needs local
  // proof, materialization is required, or `files` is requested with no `where`
  // (listing the whole file set has no provider lane — needs the local universe).
  const needsMaterialize =
    (query.from?.kind === 'github' &&
      query.target === 'files' &&
      !query.where) ||
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
