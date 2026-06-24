/**
 * OQL runner — the single entry point behind `octocode search`.
 *
 *   normalize -> plan -> (explain) -> execute via adapter -> envelope
 *
 * Handles single queries and bounded batches (1-5). `--explain` includes the
 * plan; `--dry-run` returns the plan without executing.
 */
import path from 'node:path';
import { statSync } from 'node:fs';
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
  type OqlProofGrade,
  type OqlProofGradedResultRow,
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
  const env = await runSingle(canonical, input, options);
  stripUniformSource(env.results);
  return env;
}

/**
 * Per-row `source` is identical for every row of a single-source query (one
 * `from`), so repeating it on each row is pure token noise — the source already
 * lives once in `provenance`. Strip it when uniform. A merged cross-source batch
 * has rows from different sources (NOT uniform) → kept, so mergeChildren's
 * rowKey dedup stays exact. Always runs AFTER merge.
 */
function stripUniformSource(results: OqlResultRow[]): void {
  if (results.length === 0) return;
  const key = (r: OqlResultRow) =>
    JSON.stringify((r as { source?: unknown }).source ?? null);
  const first = key(results[0]!);
  if (!results.every(r => key(r) === first)) return;
  for (const r of results) delete (r as { source?: unknown }).source;
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
    return unsupportedEnvelopeFromPlan(
      planned,
      plan,
      query.id,
      queryIndex,
      options.dryRun,
      query
    );
  }

  const exec = await dispatch(query, planned);
  relativizeResultPaths(query, exec.results);
  applyResultRowWindow(query, exec);
  const next = attachContinuations(query, exec);
  applyProofGrades(query, exec.results);

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
  // Local code search paginates matched files and caps per-file matches; the
  // mapped OQL rows are match rows. Slicing those rows would create `next.page`
  // queries that advance the file page, not the hidden match row, so leave the
  // backend pagination intact and rely on next.matchPage for noisy files.
  // EXCEPTION: an explicit `limit` is a hard cap on the primary result-row
  // domain — the caller asked for at most N rows, so it must be honored even
  // here. Page-size paging (itemsPerPage only) still defers to backend paging.
  if (
    query.target === 'code' &&
    exec.pagination?.totalItemsKind === 'files' &&
    typeof query.limit !== 'number'
  ) {
    return;
  }

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
 *  - graph       → next.graph (bounded LSP proof for candidate graph pages)
 */
export interface ContinuationCtx {
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

function firstScopePath(query: OqlQuery): string | undefined {
  const scopePath = query.scope?.path;
  return Array.isArray(scopePath) ? scopePath[0] : scopePath;
}

function githubRepoLabel(query: OqlQuery): string {
  if (query.from?.kind !== 'github') return 'owner/repo';
  if (query.from.repo?.includes('/')) return query.from.repo;
  if (query.from.owner && query.from.repo) {
    return `${query.from.owner}/${query.from.repo}`;
  }
  return query.from.repo ?? 'owner/repo';
}

function githubLocalProofHint(query: OqlQuery): string {
  const repo = githubRepoLabel(query);
  const scopePath = firstScopePath(query);
  const repoWithRef =
    query.from?.kind === 'github' && query.from.ref
      ? `${repo}@${query.from.ref}`
      : repo;
  const scopedRef = scopePath ? `${repo}/${scopePath}` : repo;
  const branchFlag =
    query.from?.kind === 'github' && query.from.ref
      ? ` --branch ${query.from.ref}`
      : '';

  if (scopePath) {
    return `Use \`search ${firstSearchTerm(query)} ${scopePath} --repo ${repoWithRef} --materialize required\` for one-step local proof, or \`clone ${scopedRef}${branchFlag}\` / \`cache fetch ${repo} ${scopePath}${branchFlag} --depth tree\` before retrying local search.`;
  }

  return `Choose a bounded path first with \`search ${repo} --tree\`, then use \`search ${firstSearchTerm(query)} <path> --repo ${repoWithRef} --materialize required\`, \`clone ${repo}/<path>${branchFlag}\`, or \`cache fetch ${repo} <path>${branchFlag} --depth tree\`. For deliberate whole-repo work, use \`clone ${repo}${branchFlag}\` or \`cache fetch ${repo}${branchFlag} --depth clone\`.`;
}

function firstSearchTerm(query: OqlQuery): string {
  const where = query.where;
  if (where?.kind === 'text' || where?.kind === 'regex') return where.value;
  if (where?.kind === 'structural') return 'pattern';
  return '<term>';
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

  // GitHub provider returned zero results — code search may not index this repo.
  // Emit next.materialize so agents can clone locally and retry with full coverage.
  if (
    exec.diagnostics.some(d => d.code === 'providerUnindexed') &&
    query.from?.kind === 'github' &&
    query.target === 'code'
  ) {
    next['next.materialize'] = {
      query: {
        schema: 'oql',
        target: 'materialize',
        from: query.from,
        ...(query.scope ? { scope: query.scope } : {}),
        materialize: { mode: 'required' },
      },
      why: `GitHub code search returned no results; this is not proof of absence. ${githubLocalProofHint(query)}`,
      confidence: 'heuristic',
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

function applyProofGrades(
  query: OqlQuery,
  results: OqlResultRow[]
): asserts results is OqlProofGradedResultRow[] {
  for (const row of results) {
    row.proofGrade ??= inferProofGrade(query, row);
  }
}

function inferProofGrade(query: OqlQuery, row: OqlResultRow): OqlProofGrade {
  if (row.kind === 'code') {
    if (
      hasPredicateKind(query.where, 'structural') ||
      row.metavars !== undefined ||
      row.metavarRanges !== undefined
    ) {
      return 'structural';
    }
    if (
      hasPredicateKind(query.where, 'text') ||
      hasPredicateKind(query.where, 'regex') ||
      row.line !== undefined ||
      row.snippet !== undefined ||
      row.matchIndices !== undefined
    ) {
      return 'text';
    }
    return 'candidate';
  }

  if (row.kind === 'content' || row.kind === 'file' || row.kind === 'tree') {
    return 'text';
  }

  if (row.kind !== 'record') {
    return 'candidate';
  }

  if (row.recordType === 'semantics') {
    return 'semantic';
  }
  if (row.recordType === 'graph') {
    return hasMissingProof(row.data) ? 'missing' : 'graph';
  }
  if (row.recordType === 'research') {
    if (hasMissingProof(row.data)) {
      return 'missing';
    }
    return row.data.mode === 'prove' ? 'graph' : 'candidate';
  }
  if (row.recordType === 'diff' || row.recordType === 'artifact') {
    return 'text';
  }

  return 'candidate';
}

function hasPredicateKind(
  predicate: OqlQuery['where'],
  kind: 'text' | 'regex' | 'structural'
): boolean {
  if (!predicate) {
    return false;
  }
  if (predicate.kind === kind) {
    return true;
  }
  if (predicate.kind === 'all' || predicate.kind === 'any') {
    return predicate.of.some(child => hasPredicateKind(child, kind));
  }
  if (predicate.kind === 'not') {
    return hasPredicateKind(predicate.predicate, kind);
  }
  return false;
}

function hasMissingProof(data: Record<string, unknown>): boolean {
  const missingProof = data.missingProof;
  if (Array.isArray(missingProof) && missingProof.length > 0) {
    return true;
  }

  const packets = data.packets;
  return (
    Array.isArray(packets) &&
    packets.some(packet => {
      if (!isRecord(packet)) {
        return false;
      }
      const packetMissing = packet.missingProof;
      return (
        packet.proofStatus === 'missing' ||
        (Array.isArray(packetMissing) && packetMissing.length > 0)
      );
    })
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

const ROW_CONTINUATION_BUILDERS: Record<string, RowContinuationBuilder> = {
  code: buildCodeContinuations,
  content: buildContentContinuations,
  'record:artifact': buildArtifactContinuations,
  'record:materialized': buildMaterializedContinuations,
  'record:semantics': buildSemanticsContinuations,
  'record:research': buildResearchContinuations,
  'record:graph': buildGraphContinuations,
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

export function buildArtifactContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const out: Record<string, OqlContinuation> = {};
  const data = (row as OqlRecordResultRow).data;
  const mode = typeof data?.mode === 'string' ? data.mode : undefined;
  const lp = derivedLocalPath(row);

  if (lp) {
    if (mode === 'strings') {
      // `strings` writes this scan window's printable runs to a flat text file
      // at localPath; the inline `content` is only a preview. Listing or file
      // discovery over a flat dump is useless — the right move is to search the
      // dump with local code search (ripgrep): a regex/pattern over a text file
      // paginates losslessly (matchPage / maxMatchesPerFile) and never quits on
      // NUL the way searching the raw binary would.
      out['next.search'] = {
        query: {
          schema: 'oql',
          target: 'code',
          from: { kind: 'local', path: lp },
          where: { kind: 'regex', value: 'https?://\\S+' },
          controls: { search: { maxMatchesPerFile: 100, matchPage: 1 } },
        },
        why: 'Grep this strings dump with local code search (ripgrep) — swap the regex/pattern for what you need (URLs, hosts, symbols); page noisy hits losslessly with matchPage. For a huge binary this beats reading the capped inline preview.',
        confidence: 'heuristic',
      };
    } else {
      // extract / unpack / decompress materialize a real tree/file on disk.
      Object.assign(out, localRootContinuations(lp, 'extracted'));
    }
  }

  // Binary `strings` scan cursor: nextScanOffset → next scan window (a typed
  // per-domain continuation instead of a raw params round-trip).
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
  const pagination =
    data?.pagination && typeof data.pagination === 'object'
      ? (data.pagination as Record<string, unknown>)
      : undefined;
  const nextCharOffset =
    typeof pagination?.nextCharOffset === 'number'
      ? pagination.nextCharOffset
      : undefined;
  if (pagination?.hasMore === true && nextCharOffset !== undefined) {
    const charLength =
      typeof pagination.charLength === 'number'
        ? pagination.charLength
        : undefined;
    out['next.artifactContent'] = {
      query: {
        ...ctx.query,
        params: {
          ...(ctx.query.params ?? {}),
          charOffset: nextCharOffset,
          ...(charLength !== undefined ? { charLength } : {}),
        },
      },
      why: 'Read the next inline artifact text window.',
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
 * `target:"research"` stays candidate-grade — it never runs LSP internally — but
 * it emits a one-call `next.graph` upgrade. `target:"graph"` emits the same
 * upgrade when an analyze row still has missing proof. The upgrade is page-aligned
 * and bounded by `proofLimit`, so agents can close proof without losing the
 * research/graph honesty boundary.
 */
function buildResearchContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  return graphProofUpgrade(row, ctx, {
    why: 'Upgrade this candidate research to LSP-proven relationships for the current page (bounded proof).',
    force: true,
  });
}

function buildGraphContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const params = ctx.query.params ?? {};
  if (
    params.proof === 'none' ||
    params.proof === 'lsp' ||
    params.mode === 'prove'
  ) {
    return undefined;
  }
  const data = (row as OqlRecordResultRow).data;
  if (!hasMissingProof(data)) return undefined;
  return graphProofUpgrade(row, ctx, {
    why: 'Upgrade this candidate graph page to LSP-proven relationships (bounded proof).',
  });
}

function graphProofUpgrade(
  row: OqlResultRow,
  ctx: ContinuationCtx,
  options: { why: string; force?: boolean }
): Record<string, OqlContinuation> | undefined {
  const from = ctx.query.from;
  // Graph proof needs a complete local file universe (local/materialized).
  if (from?.kind !== 'local' && from?.kind !== 'materialized') return undefined;
  const data = (row as OqlRecordResultRow).data;
  if (!options.force && !hasMissingProof(data)) return undefined;
  const intent =
    typeof data?.intent === 'string' && data.intent.length > 0
      ? data.intent
      : typeof ctx.query.params?.intent === 'string' &&
          ctx.query.params.intent.length > 0
        ? ctx.query.params.intent
        : 'reachability';
  // proofLimit is bounded (graphParams caps at 25); align it to the page size so
  // the upgrade proves roughly the same number of subjects shown this page.
  const proofLimit = Math.min(25, Math.max(1, ctx.query.itemsPerPage ?? 10));
  const params = ctx.query.params ?? {};
  const facets = continuationResearchFacets(data, params);
  return {
    'next.graph': {
      query: {
        schema: 'oql',
        target: 'graph',
        from,
        params: {
          ...params,
          mode: 'prove',
          proof: 'lsp',
          intent,
          proofLimit,
          ...(facets ? { facets } : {}),
        },
        ...(ctx.query.page ? { page: ctx.query.page } : {}),
        ...(ctx.query.itemsPerPage
          ? { itemsPerPage: ctx.query.itemsPerPage }
          : {}),
      },
      why: options.why,
      confidence: 'exact',
    },
  };
}

const PUBLIC_RESEARCH_FACETS = new Set([
  'symbols',
  'files',
  'dependencies',
  'relations',
]);

function continuationResearchFacets(
  data: Record<string, unknown>,
  params: Record<string, unknown>
): string[] | undefined {
  const raw = Array.isArray(params.facets)
    ? params.facets
    : Array.isArray(data.facets)
      ? data.facets
      : undefined;
  if (!raw) return undefined;
  const facets = raw.filter(
    (facet): facet is string =>
      typeof facet === 'string' && PUBLIC_RESEARCH_FACETS.has(facet)
  );
  return facets.length > 0 ? facets : undefined;
}

function buildSemanticsContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const data = (row as OqlRecordResultRow).data;
  const rawUri = typeof data?.uri === 'string' ? data.uri : undefined;
  const sourceAnchor =
    semanticSourceAnchor(row as OqlRecordResultRow) ??
    semanticQueryAnchor(ctx.query);
  const uri = semanticContinuationUri(rawUri, sourceAnchor);
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

function semanticContinuationUri(
  rawUri: string | undefined,
  sourceAnchor: string | undefined
): string | undefined {
  if (rawUri && path.isAbsolute(rawUri)) return rawUri;
  if (sourceAnchor && path.isAbsolute(sourceAnchor)) {
    const base = semanticAnchorBase(sourceAnchor);
    return rawUri ? path.resolve(base, rawUri) : sourceAnchor;
  }
  return sourceAnchor ?? rawUri;
}

function semanticAnchorBase(sourceAnchor: string): string {
  try {
    return statSync(sourceAnchor).isDirectory()
      ? sourceAnchor
      : path.dirname(sourceAnchor);
  } catch {
    return path.dirname(sourceAnchor);
  }
}

function semanticSourceAnchor(row: OqlRecordResultRow): string | undefined {
  const source = row.source;
  if (source?.kind === 'local') return source.path;
  if (source?.kind === 'materialized') return source.localPath;
  return undefined;
}

function semanticQueryAnchor(query: OqlQuery): string | undefined {
  if (query.from?.kind === 'local') return query.from.path;
  if (query.from?.kind === 'materialized') return query.from.localPath;
  return undefined;
}

/**
 * For local/materialized sources, return a builder that turns a relativized
 * row path back into an absolute-file `from` (relativization stripped exactly
 * `dirname(resolve(root)) + '/'`, so re-adding it round-trips).
 */
function localFileSource(
  query: OqlQuery
): ((rowPath: string) => OqlQuery['from']) | undefined {
  const root = queryLocalRoot(query);
  if (!root) return undefined;
  // Same base as relativizeResultPaths, minus the trailing separator, so a
  // relativized row path re-joins to the exact absolute file (round-trip).
  const base = relativizeBase(root).slice(0, -path.sep.length);
  return (rowPath: string) => ({
    kind: 'local',
    path: path.isAbsolute(rowPath) ? rowPath : path.join(base, rowPath),
  });
}

/**
 * `target:"files"` over a GitHub source with no `where` cannot be enumerated by
 * the provider, so the plan is non-executable (audit #6). Instead of a dead end,
 * hand the agent a runnable next.materialize query — clone a bounded corpus and
 * list files from the materialized checkpoint — mirroring the providerUnindexed
 * recovery path.
 */
function blockedMaterializeContinuation(
  query?: OqlQuery
): OqlResultEnvelope['next'] | undefined {
  if (
    !query ||
    query.from?.kind !== 'github' ||
    query.target !== 'files' ||
    query.where
  ) {
    return undefined;
  }
  return {
    'next.materialize': {
      query: {
        schema: 'oql',
        target: 'materialize',
        from: query.from,
        ...(query.scope ? { scope: query.scope } : {}),
        materialize: { mode: 'required' },
      },
      why: 'target:"files" over GitHub needs a local corpus to enumerate; clone a bounded path (add scope.path to narrow), then list files from the materialized checkpoint.',
      confidence: 'heuristic',
    },
  };
}

function unsupportedEnvelopeFromPlan(
  planned: PlanQueryResult,
  plan: OqlResultEnvelope['plan'],
  queryId?: string,
  queryIndex?: number,
  dryRun?: boolean,
  query?: OqlQuery
): OqlResultEnvelope {
  if (!planned.executable) {
    // In dry-run mode, distinguish repairable blocks (e.g. missing scope.path
    // for materialization) from structural capability gaps (UNSUPPORTED route
    // nodes). Repairable queries have a valid plan with executable routing
    // decisions — they just need a constraint fix. Show 'partial' so the plan
    // and diagnostics are the primary output, not 'unsupported'.
    const hasUnsupportedRoute = planned.plan.nodes.some(
      n => n.route === 'UNSUPPORTED'
    );
    if (dryRun && !hasUnsupportedRoute) {
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
    return unsupportedEnvelope(
      planned.plan.diagnostics,
      plan,
      queryId,
      queryIndex,
      blockedMaterializeContinuation(query)
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
 * The prefix stripped from absolute local result paths so rows are concise and
 * match what the raw tools return. When the query root is a DIRECTORY, paths are
 * relative to it (`/…/src/oql` + `/…/src/oql/x.ts` -> `x.ts`, `adapters/y.ts`) —
 * exactly what `localSearchCode`/`ls`/`find` emit. When the root is a single
 * FILE (content reads), strip its directory so the row shows the basename. A
 * GitHub/unresolved root has no on-disk shape, so fall back to the parent.
 *
 * The SAME base feeds `localFileSource` (the inverse), so a relativized row path
 * always round-trips back to the correct absolute file for next.* continuations.
 */
function relativizeBase(root: string): string {
  const abs = path.resolve(root);
  try {
    if (statSync(abs).isDirectory()) return `${abs}${path.sep}`;
  } catch {
    /* not on disk yet — treat as parent-relative */
  }
  return `${path.dirname(abs)}${path.sep}`;
}

function queryLocalRoot(query: OqlQuery): string | undefined {
  return query.from?.kind === 'local'
    ? query.from.path
    : query.from?.kind === 'materialized'
      ? query.from.localPath
      : undefined;
}

/**
 * Relativize absolute local result paths to the query root (see relativizeBase),
 * keeping `search` aligned with the former grep/ls shortcuts and far less verbose. Provider
 * (GitHub) paths are already repo-relative and left untouched.
 */
function relativizeResultPaths(query: OqlQuery, results: OqlResultRow[]): void {
  const root = queryLocalRoot(query);
  if (!root) return;
  const prefix = relativizeBase(root);
  for (const row of results) {
    const p = (row as { path?: string }).path;
    if (typeof p === 'string' && p.startsWith(prefix)) {
      (row as { path: string }).path = p.slice(prefix.length);
    }
  }
}

/* ------------------------------ select ---------------------------------- */

// Row identity always survives projection (needed to cite + continue).
const SELECT_ALWAYS_KEEP = new Set([
  'kind',
  'source',
  'recordType',
  'id',
  'proofGrade',
]);

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
  'proofGrade',
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
      ...(anyOpenPages ? { pagination: { hasMore: true } } : {}),
      diagnostics,
      provenance,
      executable: children.every(
        c => c.envelope.evidence.kind !== 'unsupported'
      ),
      approximate,
    }),
  };
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
