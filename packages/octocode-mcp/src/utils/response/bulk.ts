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
import { tsvFormat } from './tsvFormat.js';
import { getTsvProjection } from './tsvColumns.js';

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
  const topLevelFields = [
    'format',
    'columns',
    'rows',
    'results',
    'hints',
    'evidence',
  ];
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
    orderedQueries[r.queryIndex] = {
      id: resolveQueryId(r.originalQuery, r.queryIndex),
      status: r.result.status,
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

  // Same idea for `evidence`: lift per-query `data.evidence` blocks into a
  // single top-level summary (kind taken from first present; answerReady /
  // complete combined with AND; confidence is the weakest of all).
  const aggregatedEvidence = config.peerEvidence
    ? aggregatePeerEvidence(queryPaginatedResults)
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

  if (aggregatedHints.length > 0) {
    responseData.hints = aggregatedHints;
  }

  if (aggregatedEvidence) {
    responseData.evidence = aggregatedEvidence;
  }

  // TSV mode — emit columns/rows derived from the per-tool projection and
  // mark the envelope with `format: "tsv"`. Skip TSV entirely when every
  // query is verbosity:"ultra" (ultra wipes the data field, so there are
  // no rows to emit; emitting empty columns/rows is just noise).
  const allUltra =
    queries.length > 0 &&
    queries.every(
      (q): boolean =>
        (q as Record<string, unknown> | undefined)?.verbosity === 'ultra'
    );
  const tsvEmitted = config.format === 'tsv' && !allUltra;
  if (tsvEmitted) {
    const projection = getTsvProjection(config.toolName);
    if (projection) {
      const rows = queryPaginatedResults.flatMap(q =>
        projection.toRows(q.data)
      );
      responseData.format = 'tsv';
      responseData.columns = projection.columns;
      responseData.rows = tsvFormat(projection.columns, rows);
    }
  }

  // In TSV mode, exclude `results` from content[0].text — agents reading the
  // text only need the compact rows, not the full per-query JSON structs.
  // The complete `results` array is still available via structuredContent.
  type BulkResponseForText = Omit<BulkToolResponse, 'results'> &
    Partial<Pick<BulkToolResponse, 'results'>>;
  const textPayload: BulkResponseForText = tsvEmitted
    ? { ...responseData, results: undefined }
    : responseData;
  const text = createResponseFormat(
    textPayload as BulkToolResponse,
    fullKeysPriority
  );
  recordBulkCharSavings(config.toolName, results, errors, text.length);

  return {
    content: [
      {
        type: 'text' as const,
        text,
      },
    ],
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
function aggregatePeerEvidence(
  queries: FlatQueryResult[]
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
    if (!raw || typeof raw !== 'object') continue;
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
  if (reasons.length > 0) out.reason = reasons.join('; ');
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
