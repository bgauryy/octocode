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
  BulkResponseConfig,
  BulkToolResponse,
  PromiseResult,
} from '../../types.js';
import {
  applyBulkResponsePagination,
  applyQueryOutputPagination,
} from './structuredPagination.js';
import { countSerializedChars, getRawResponseChars } from './charSavings.js';

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

export async function executeBulkOperation<TQuery extends object>(
  queries: Array<TQuery>,
  processor: (query: TQuery, index: number) => Promise<ProcessedBulkResult>,
  config: BulkResponseConfig
): Promise<CallToolResult> {
  const concurrency = config.concurrency ?? DEFAULT_BULK_CONCURRENCY;
  const { results, errors } = await processBulkQueries<TQuery>(
    queries,
    processor,
    concurrency,
    config.minQueryTimeoutMs
  );
  return createBulkResponse<TQuery>(config, results, errors, queries);
}

function createBulkResponse<TQuery extends object>(
  config: BulkResponseConfig,
  results: Array<{
    result: ProcessedBulkResult;
    queryIndex: number;
    originalQuery: TQuery;
  }>,
  errors: QueryError[],
  queries: Array<TQuery>
): CallToolResult {
  const topLevelFields = ['results'];
  const resultFields = ['id', 'status', 'data', 'hints'];
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

  const queryPaginatedResults = flatQueries.map((queryResult, index) =>
    applyQueryOutputPagination(
      queryResult,
      (queries[index] as Record<string, unknown>) ?? {},
      config.toolName
    )
  );
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
  const text = createResponseFormat(responseData, fullKeysPriority);
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
