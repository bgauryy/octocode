import { CallToolResult } from '@modelcontextprotocol/sdk/types';
import { incrementToolCharSavings } from 'octocode-shared';
import { executeWithErrorIsolation } from '../core/promise.js';
import {
  createResponseFormat,
  sanitizeStructuredContent,
} from '../../responses.js';
import type {
  ProcessedBulkResult,
  FlatQueryResult,
  QueryError,
  EvidenceMetadata,
} from '../../types/toolResults.js';
import type { BulkResponseConfig, BulkToolResponse } from '../../types/bulk.js';
import type { PromiseResult } from '../../types/promise.js';
import {
  applyBulkResponsePagination,
  applyQueryOutputPagination,
} from './structuredPagination.js';
import { countSerializedChars, getRawResponseChars } from './charSavings.js';
import { relativizeResultPaths, hoistSharedFields } from './pathRelativize.js';
import { isConcise } from '../../scheme/verbosity.js';
import type { Verbosity } from '../../scheme/localSchemaOverlay.js';

/** Default concurrency for bulk operations */
const DEFAULT_BULK_CONCURRENCY = 3;

/**
 * Maximum timeout per query in bulk operations (default 60s).
 * Configurable via OCTOCODE_BULK_QUERY_TIMEOUT_MS.
 */
const BULK_QUERY_TIMEOUT_MS =
  parseInt(process.env.OCTOCODE_BULK_QUERY_TIMEOUT_MS || '60000', 10) || 60000;

/**
 * The outer (security wrapper) timeout that bounds the entire tool call.
 * Used to compute an adaptive per-query timeout so multi-query operations
 * don't hit the outer wall before all queries complete.
 */
const OUTER_TIMEOUT_MS =
  parseInt(process.env.OCTOCODE_TOOL_TIMEOUT_MS || '60000', 10) || 60000;

/** Minimum per-query timeout to avoid impractically short budgets */
const MIN_QUERY_TIMEOUT_MS = 5_000;

/**
 * Compute per-query timeout that respects the outer tool timeout.
 *
 * Accounts for concurrency: when queries run in parallel, the wall-clock
 * time equals the slowest query in each batch, not the sum. So each query
 * in a fully-parallel batch can safely use the full outer budget.
 *
 * @param queryCount    Total number of queries.
 * @param concurrency   Max concurrent queries (determines batch count).
 * @param minTimeoutMs  Optional floor — guarantees a minimum per-query budget
 *                      for expensive operations (e.g. LSP cold-start).
 * @internal Exported for testing.
 */
export function computeQueryTimeout(
  queryCount: number,
  concurrency: number,
  minTimeoutMs?: number
): number {
  if (queryCount <= 1) return BULK_QUERY_TIMEOUT_MS;
  const effectiveConcurrency = Math.min(Math.max(concurrency, 1), queryCount);
  const batches = Math.ceil(queryCount / effectiveConcurrency);
  const fair = Math.floor(OUTER_TIMEOUT_MS / batches);
  const computed = Math.max(
    MIN_QUERY_TIMEOUT_MS,
    Math.min(fair, BULK_QUERY_TIMEOUT_MS)
  );
  return minTimeoutMs ? Math.max(computed, minTimeoutMs) : computed;
}

export async function executeBulkOperation<
  TQuery extends object,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
>(
  queries: Array<TQuery>,
  processor: (query: TQuery, index: number) => Promise<ProcessedBulkResult>,
  config: BulkResponseConfig<TQuery, TOutput>
): Promise<CallToolResult> {
  const concurrency = config.concurrency ?? DEFAULT_BULK_CONCURRENCY;
  const { results, errors } = await processBulkQueries<TQuery>(
    queries,
    processor,
    concurrency,
    config.minQueryTimeoutMs
  );
  return createBulkResponse<TQuery, TOutput>(config, results, errors, queries);
}

function createBulkResponse<
  TQuery extends object,
  TOutput extends Record<string, unknown>,
>(
  config: BulkResponseConfig<TQuery, TOutput>,
  results: Array<{
    result: ProcessedBulkResult;
    queryIndex: number;
    originalQuery: TQuery;
  }>,
  errors: QueryError[],
  queries: Array<TQuery>
): CallToolResult {
  const topLevelFields = ['results', 'hints', 'evidence', 'base', 'shared'];
  const resultFields = ['id', 'status', 'data'];
  const fullKeysPriority = [
    ...new Set([
      ...topLevelFields,
      ...resultFields,
      ...(config.keysPriority || []),
    ]),
  ];

  const orderedQueries: Array<FlatQueryResult | undefined> = new Array(
    queries.length
  );

  results.forEach(r => {
    // Omit status when absent — success is signaled by the lack of a
    // status field. Only 'empty' / 'error' are emitted.
    const status = r.result.status;
    orderedQueries[r.queryIndex] = {
      id: resolveQueryId(r.originalQuery, r.queryIndex),
      ...(status !== undefined ? { status } : {}),
      data: extractToolData(r.result),
    };
  });

  errors.forEach(err => {
    const originalQuery = queries[err.queryIndex];
    if (!originalQuery) return;

    orderedQueries[err.queryIndex] = {
      id: resolveQueryId(originalQuery, err.queryIndex),
      status: 'error',
      data: { error: err.error },
    };
  });

  const flatQueries = orderedQueries.filter(
    (query): query is FlatQueryResult => query !== undefined
  );

  // Finalizer hook — tools with a non-default response shape (e.g. flat
  // owner/repo grouped responses) own the rest of the pipeline from here.
  if (config.finalize) {
    const finalized = config.finalize({
      queries,
      results: flatQueries,
      config,
    });
    recordBulkCharSavings(
      config.toolName,
      results,
      errors,
      finalized.text.length
    );
    return {
      content: [{ type: 'text' as const, text: finalized.text }],
      // No cast needed — TOutput is constrained to `Record<string, unknown>`,
      // so it is structurally compatible with `CallToolResult.structuredContent`.
      structuredContent: finalized.structuredContent,
      isError:
        finalized.isError ??
        (flatQueries.length > 0 &&
          flatQueries.every(queryResult => queryResult.status === 'error')),
    };
  }

  const queryPaginatedResults = flatQueries.map((queryResult, index) =>
    applyQueryOutputPagination(
      queryResult,
      (queries[index] as Record<string, unknown>) ?? {},
      config.toolName
    )
  );

  // Lift hints out of each query's `data` so they appear once at peer level.
  // Opt-in: some output schemas (local/lsp) are strict about top-level keys,
  // so callers explicitly enable this with `config.peerHints` once they have
  // widened their output schema to accept `hints` at root.
  const aggregatedHints = config.peerHints
    ? dedupePeerHints(queryPaginatedResults)
    : [];

  // When every query asked for concise, the bulk runs in probe mode: the
  // display arrays are dropped and the per-query counts are the answer, so
  // display-pagination "has more" must not mark the aggregate incomplete.
  const allConcise =
    queries.length > 0 &&
    queries.every((q): boolean =>
      isConcise((q as { verbosity?: Verbosity } | undefined)?.verbosity)
    );

  // Same idea for `evidence`: lift per-query `data.evidence` blocks into a
  // single top-level summary (kind taken from first present; answerReady /
  // complete combined with AND; confidence is the weakest of all).
  const aggregatedEvidence = config.peerEvidence
    ? aggregatePeerEvidence(queryPaginatedResults, allConcise)
    : undefined;

  const responseData: BulkToolResponse = applyBulkResponsePagination(
    {
      results: queryPaginatedResults,
    },
    {
      offset: config.responseCharOffset,
      length: config.responseCharLength,
    },
    config.toolName
  );

  // Leanness: hoist redundancy out of the canonical structuredContent (the
  // payload the model reads). Both are lossless and reconstructable.
  //   - `base`: common directory of absolute `path`/`uri` fields; abs =
  //     `${base}/${path}`. No-op for repo-relative github paths.
  //   - `shared`: scalar fields identical across every leaf object; each leaf
  //     re-gains every `shared` key on reconstruction.
  if (!allConcise && Array.isArray(responseData.results)) {
    const dataBase = relativizeResultPaths(
      responseData.results as Array<{ data?: unknown }>
    );
    if (dataBase) responseData.base = dataBase;

    const shared = hoistSharedFields(
      responseData.results as Array<{ data?: unknown }>
    );
    if (shared) responseData.shared = shared;
  }

  // Second lift-and-dedupe pass: applyBulkResponsePagination can re-introduce
  // hints into per-query `data` (via withPaginationHints) AFTER the initial
  // dedupePeerHints pass ran. Lift those once more so each pagination
  // breadcrumb shows up exactly once at the top-level `hints[]`.
  const postPaginationHints = config.peerHints
    ? dedupePeerHints(
        Array.isArray(responseData.results)
          ? (responseData.results as FlatQueryResult[])
          : []
      )
    : [];

  const mergedHints = config.peerHints
    ? Array.from(new Set([...aggregatedHints, ...postPaginationHints]))
    : aggregatedHints;

  if (mergedHints.length > 0) {
    responseData.hints = mergedHints;
  }

  const finalEvidence = aggregatedEvidence
    ? dropRedundantPaginationReason(
        withEvidenceReasons(
          aggregatedEvidence,
          responsePaginationReasons(responseData)
        ),
        mergedHints
      )
    : undefined;

  if (finalEvidence) {
    responseData.evidence = finalEvidence;
  }

  // Structured YAML/JSON output: the full per-query `results` array is the
  // single source of truth, surfaced identically in content[0].text and
  // structuredContent. `base` relativization keeps paths compact; evidence /
  // pagination / hints carry the response-state signal.
  const text = createResponseFormat(responseData, fullKeysPriority);
  recordBulkCharSavings(config.toolName, results, errors, text.length);

  return {
    content: [
      {
        type: 'text' as const,
        text,
      },
    ],
    // structuredContent holds the canonical structured records (results +
    // pagination + hints + evidence + `base`). `base` is retained here:
    // canonical paths are relativized against it, so the model reconstructs
    // abs = `${base}/${path}`. (#A1)
    structuredContent: sanitizeStructuredContent(responseData) as Record<
      string,
      unknown
    >,
    isError:
      flatQueries.length > 0 &&
      flatQueries.every(queryResult => queryResult.status === 'error'),
  };
}

/**
 * Walk every flattened query and lift `data.hints` out into a deduped
 * top-level array. Mutates each query result to drop its local `hints` so
 * the field appears once at response root instead of repeated per query.
 */
function dedupePeerHints(queries: FlatQueryResult[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const q of queries) {
    const data = q.data as Record<string, unknown> | undefined;
    const raw =
      data && Array.isArray(data.hints) ? (data.hints as unknown[]) : [];
    for (const h of raw) {
      if (typeof h === 'string' && h.trim().length > 0 && !seen.has(h)) {
        seen.add(h);
        out.push(h);
      }
    }
    if (data && 'hints' in data) {
      delete (data as Record<string, unknown>).hints;
    }
  }
  return out;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function hasMorePagination(value: unknown): boolean {
  return isRecord(value) && value.hasMore === true;
}

function queryPaginationReasons(data: Record<string, unknown>): string[] {
  const reasons: string[] = [];
  if (
    hasMorePagination(data.outputPagination) ||
    hasMorePagination(data.charPagination)
  ) {
    reasons.push('One or more query-level output pages have more data.');
  }
  if (hasMorePagination(data.pagination)) {
    reasons.push('Result pagination has more results.');
  }
  return reasons;
}

function responsePaginationReasons(data: BulkToolResponse): string[] {
  return hasMorePagination(data.responsePagination)
    ? ['Bulk response pagination has more data.']
    : [];
}

function withEvidenceReasons(
  evidence: EvidenceMetadata,
  extraReasons: readonly string[]
): EvidenceMetadata {
  const reasons = Array.from(
    new Set(
      [
        typeof evidence.reason === 'string' ? evidence.reason : '',
        ...extraReasons,
      ]
        .map(reason => reason.trim())
        .filter(Boolean)
    )
  );
  if (reasons.length === 0) {
    return evidence;
  }
  return {
    ...evidence,
    complete: false,
    reason: reasons.join('; '),
  };
}

const RESULT_PAGINATION_REASON = 'Result pagination has more results.';

/** True when a hint already carries the actionable result-page cursor. */
function hasResultPageCursorHint(hints: readonly string[]): boolean {
  return hints.some(h => /\bNext:\s*page=|\bPage\s+\d+\/\d+/i.test(h));
}

/**
 * #B2: when a cursor hint (e.g. "Page 1/10 … Next: page=2") already tells the
 * agent there's more, the generic `evidence.reason` "Result pagination has more
 * results." is pure redundancy — drop it. `complete` stays false (the hint
 * conveys incompleteness); other reasons are preserved.
 */
export function dropRedundantPaginationReason(
  evidence: EvidenceMetadata,
  hints: readonly string[]
): EvidenceMetadata {
  if (!evidence.reason || !hasResultPageCursorHint(hints)) return evidence;
  const parts = evidence.reason
    .split(';')
    .map(part => part.trim())
    .filter(Boolean);
  const kept = parts.filter(part => part !== RESULT_PAGINATION_REASON);
  if (kept.length === parts.length) return evidence; // nothing removed
  const next: EvidenceMetadata = { ...evidence };
  if (kept.length > 0) next.reason = kept.join('; ');
  else delete next.reason;
  return next;
}

/**
 * Walk every query and combine their `data.evidence` blocks into one
 * top-level summary. Mutates each query to drop its local `evidence` so the
 * field appears once at root. Combination rules:
 *   - `kind`          → first non-empty value (most tools emit one kind).
 *   - `answerReady`   → true only if every query that set it is true.
 *   - `complete`      → true only if every query that set it is true.
 *   - `confidence`    → weakest of all present (low < medium < high).
 *   - `missingFields` → deduped union of all entries.
 *   - `reason`        → joined when multiple queries supplied one.
 */
export function aggregatePeerEvidence(
  queries: FlatQueryResult[],
  allConcise = false
): EvidenceMetadata | undefined {
  const rankConfidence: Record<
    NonNullable<EvidenceMetadata['confidence']>,
    number
  > = { low: 0, medium: 1, high: 2 };
  let combinedKind: EvidenceMetadata['kind'];
  let answerReadyAll: boolean | undefined;
  let completeAll: boolean | undefined;
  let weakestConfidence: EvidenceMetadata['confidence'];
  const reasons: string[] = [];
  const missing = new Set<string>();
  let sawAny = false;

  for (const q of queries) {
    const data = q.data as Record<string, unknown> | undefined;
    const raw = data?.evidence as EvidenceMetadata | undefined;
    if (!data || !raw || typeof raw !== 'object') continue;
    sawAny = true;
    if (!combinedKind && raw.kind) combinedKind = raw.kind;
    if (typeof raw.answerReady === 'boolean') {
      answerReadyAll =
        answerReadyAll === undefined
          ? raw.answerReady
          : answerReadyAll && raw.answerReady;
    }
    if (typeof raw.complete === 'boolean') {
      completeAll =
        completeAll === undefined ? raw.complete : completeAll && raw.complete;
    }
    if (raw.confidence) {
      if (
        !weakestConfidence ||
        rankConfidence[raw.confidence] < rankConfidence[weakestConfidence]
      ) {
        weakestConfidence = raw.confidence;
      }
    }
    if (typeof raw.reason === 'string' && raw.reason.trim().length > 0) {
      reasons.push(raw.reason.trim());
    }
    // In all-concise probe mode the display arrays were dropped and the counts
    // are the answer, so display-pagination "has more" is expected and must not
    // mark the aggregate incomplete. (The per-query builders already suppress
    // their own pagination reasons under concise.)
    if (!allConcise) {
      const paginationReasons = queryPaginationReasons(data);
      if (paginationReasons.length > 0) {
        completeAll = false;
        reasons.push(...paginationReasons);
      }
    }
    if (Array.isArray(raw.missingFields)) {
      for (const f of raw.missingFields) {
        if (typeof f === 'string' && f.length > 0) missing.add(f);
      }
    }
    if (data && 'evidence' in data) {
      delete (data as Record<string, unknown>).evidence;
    }
  }

  if (!sawAny) return undefined;

  const out: EvidenceMetadata = {};
  if (combinedKind) out.kind = combinedKind;
  if (answerReadyAll !== undefined) out.answerReady = answerReadyAll;
  if (completeAll !== undefined) out.complete = completeAll;
  if (weakestConfidence) out.confidence = weakestConfidence;
  const uniqueReasons = Array.from(new Set(reasons));
  if (uniqueReasons.length > 0) out.reason = uniqueReasons.join('; ');
  if (missing.size > 0) out.missingFields = Array.from(missing);
  return Object.keys(out).length > 0 ? out : undefined;
}

function recordBulkCharSavings(
  toolName: string,
  results: Array<{
    result: ProcessedBulkResult;
    queryIndex: number;
    originalQuery: unknown;
  }>,
  errors: QueryError[],
  responseChars: number
): void {
  const rawChars =
    results.reduce(
      (sum, entry) =>
        sum +
        (getRawResponseChars(entry.result) ??
          countSerializedChars(entry.result)),
      0
    ) + errors.reduce((sum, error) => sum + countSerializedChars(error), 0);

  try {
    incrementToolCharSavings(toolName, rawChars, responseChars);
  } catch {
    // Local stats are best-effort and must never affect tool responses.
  }
}

/**
 * Process multiple queries in parallel with error isolation.
 * Internal function used by executeBulkOperation().
 *
 * @param queries - Array of query objects to process
 * @param processor - Async function that processes each query
 * @param concurrency - Maximum number of concurrent operations
 * @returns Object containing successful results and errors
 */
async function processBulkQueries<TQuery extends object>(
  queries: Array<TQuery>,
  processor: (query: TQuery, index: number) => Promise<ProcessedBulkResult>,
  concurrency: number,
  minQueryTimeoutMs?: number
): Promise<{
  results: Array<{
    result: ProcessedBulkResult;
    queryIndex: number;
    originalQuery: TQuery;
  }>;
  errors: QueryError[];
}> {
  const results: Array<{
    result: ProcessedBulkResult;
    queryIndex: number;
    originalQuery: TQuery;
  }> = [];
  const errors: QueryError[] = [];

  if (!queries || queries.length === 0) {
    return { results, errors };
  }

  const queryPromiseFunctions = queries.map(
    (query, index) => () =>
      processor(query, index).then(result => ({
        result,
        queryIndex: index,
        originalQuery: query,
      }))
  );

  const queryResults = await executeWithErrorIsolation(queryPromiseFunctions, {
    timeout: computeQueryTimeout(
      queries.length,
      concurrency,
      minQueryTimeoutMs
    ),
    continueOnError: true,
    concurrency,
    onError: (error: Error, index: number) => {
      errors.push({
        queryIndex: index,
        error: error.message,
      });
    },
  });

  queryResults.forEach(
    (
      result: PromiseResult<{
        result: ProcessedBulkResult;
        queryIndex: number;
        originalQuery: TQuery;
      }>
    ) => {
      if (result.success && result.data) {
        results.push({
          result: result.data.result,
          queryIndex: result.data.queryIndex,
          originalQuery: result.data.originalQuery,
        });
      }
    }
  );

  return { results, errors };
}

function filterHints(hints: unknown): string[] | undefined {
  if (!Array.isArray(hints)) return undefined;
  const filtered = hints.filter(
    (h): h is string => typeof h === 'string' && h.trim().length > 0
  );
  return filtered.length > 0 ? filtered : undefined;
}

function extractToolData(result: ProcessedBulkResult): Record<string, unknown> {
  const excludedKeys = new Set([
    'status',
    'mainResearchGoal',
    'researchGoal',
    'reasoning',
    'researchSuggestions',
    'query',
  ]);

  if (result.status !== 'error') {
    excludedKeys.add('error');
  }

  const toolData: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(result)) {
    if (!excludedKeys.has(key)) {
      if (key === 'hints') {
        const filtered = filterHints(value);
        if (filtered) toolData[key] = filtered;
      } else {
        toolData[key] = value;
      }
    }
  }

  return toolData;
}

function resolveQueryId<TQuery extends object>(
  originalQuery: TQuery,
  queryIndex: number
): string {
  const queryRecord = originalQuery as Record<string, unknown>;
  const rawId = queryRecord.id;
  if (typeof rawId === 'string' && rawId.trim().length > 0) {
    return rawId;
  }
  if (typeof rawId === 'number' && Number.isFinite(rawId)) {
    return String(rawId);
  }
  return `q${queryIndex + 1}`;
}
