/**
 * Single-query execution: normalize -> plan -> (explain) -> execute via
 * adapter -> envelope. Also the per-result-set helpers (pagination window,
 * shared-ref pruning, uniform-source stripping) that only apply within one
 * query's result set.
 */
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { executeGithub } from '../adapters/github.js';
import { executeLocal, type AdapterResult } from '../adapters/local.js';
import {
  executeMaterialize,
  executeMaterializeCheckpoint,
} from '../adapters/materialize.js';
import { RESEARCH_TARGET_ADAPTERS } from '../adapters/researchTargets.js';
import { backendsApproximate, buildEnvelope } from '../envelope.js';
import { planQuery, type PlanQueryResult } from '../planner.js';
import type { OqlQuery, OqlResultEnvelope, OqlResultRow } from '../types.js';
import {
  attachContinuations,
  compactRowContinuationHints,
} from './continuations/registry.js';
import { unsupportedEnvelopeFromPlan } from './dryRun.js';
import { relativizeResultPaths } from './paths.js';
import { applyProofGrades } from './proofGrades.js';
import { applySelect } from './select.js';

export interface RunOptions {
  authInfo?: AuthInfo;
  /** Plan only; do not execute. Maps to `octocode search --dry-run`. */
  dryRun?: boolean;
}

/**
 * Per-row `source` is identical for every row of a single-source query (one
 * `from`), so repeating it on each row is pure token noise — the source already
 * lives once in `provenance`. Strip it when uniform. A merged cross-source batch
 * has rows from different sources (NOT uniform) → kept, so mergeChildren's
 * rowKey dedup stays exact. Always runs AFTER merge.
 */
export function stripUniformSource(results: OqlResultRow[]): void {
  if (results.length === 0) return;
  const key = (r: OqlResultRow) =>
    JSON.stringify((r as { source?: unknown }).source ?? null);
  const first = key(results[0]!);
  if (!results.every(r => key(r) === first)) return;
  for (const r of results) delete (r as { source?: unknown }).source;
}

export async function runSingle(
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
  pruneSharedRefs(exec);
  const next = attachContinuations(query, exec);
  applyProofGrades(query, exec.results);

  // select: project row fields + continuations (projection only — never changes
  // result domains or triggers fetches). Unknown fields are reported, not fatal.
  const projectionDiagnostics = applySelect(query, exec.results);
  const nextHints = compactRowContinuationHints(exec.results);

  return buildEnvelope({
    queryId: query.id,
    queryIndex,
    results: exec.results,
    ...(exec.shared ? { shared: exec.shared } : {}),
    ...(exec.pagination ? { pagination: exec.pagination } : {}),
    ...(Object.keys(next).length ? { next } : {}),
    ...(nextHints ? { nextHints } : {}),
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
  const limitIsHardCap = typeof query.limit === 'number' && query.limit > 0;
  // Code search paginates matched files and caps per-file matches; the mapped
  // OQL rows are match rows (local marks this totalItemsKind:'files', GitHub
  // itemUnit:'files'). Slicing those rows would create `next.page` queries
  // that advance the file page, not the hidden match row — page 2 would then
  // silently skip the tail matches of page-1 files — so leave the backend
  // pagination intact and rely on next.matchPage for noisy files.
  // EXCEPTION: an explicit `limit` is a hard cap on the primary result-row
  // domain — the caller asked for at most N rows, so it must be honored even
  // here. Page-size paging (itemsPerPage only) still defers to backend paging.
  if (
    query.target === 'code' &&
    (exec.pagination?.totalItemsKind === 'files' ||
      exec.pagination?.itemUnit === 'files') &&
    !limitIsHardCap
  ) {
    return;
  }

  const cap = limitIsHardCap
    ? query.limit
    : typeof query.itemsPerPage === 'number'
      ? query.itemsPerPage
      : undefined;
  if (!cap || cap < 1 || exec.results.length <= cap) return;

  const totalItems = exec.pagination?.totalItems ?? exec.results.length;
  const currentPage = exec.pagination?.currentPage ?? query.page ?? 1;
  exec.results = exec.results.slice(0, cap);
  if (limitIsHardCap) {
    const hasMore = exec.pagination?.hasMore ?? true;
    exec.pagination = {
      ...exec.pagination,
      currentPage,
      itemsPerPage: cap,
      totalItems,
      totalItemsCapped: true,
      hasMore,
    };
    return;
  }
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

function pruneSharedRefs(exec: AdapterResult): void {
  const repositories = exec.shared?.repositories;
  if (
    !repositories ||
    typeof repositories !== 'object' ||
    Array.isArray(repositories)
  ) {
    return;
  }

  const referenced = new Set<string>();
  for (const row of exec.results) {
    if (row.kind !== 'record' || row.recordType !== 'package') continue;
    const repositoryId = row.data.repositoryId;
    if (typeof repositoryId === 'string') referenced.add(repositoryId);
  }

  if (referenced.size === 0) {
    delete exec.shared?.repositories;
    if (exec.shared && Object.keys(exec.shared).length === 0) {
      delete exec.shared;
    }
    return;
  }

  const pruned: Record<string, unknown> = {};
  for (const id of referenced) {
    const ref = (repositories as Record<string, unknown>)[id];
    if (ref !== undefined) pruned[id] = ref;
  }

  exec.shared = { ...exec.shared, repositories: pruned };
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
