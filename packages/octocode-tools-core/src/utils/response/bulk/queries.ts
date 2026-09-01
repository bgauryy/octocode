import { executeWithErrorIsolation } from '../../core/promise.js';
import type {
  ProcessedBulkResult,
  QueryError,
} from '../../../types/toolResults.js';
import type { PromiseResult } from '../../../types/promise.js';
import { traceResponseCache } from '../../http/cache/trace.js';

const BULK_QUERY_TIMEOUT_MS =
  parseInt(process.env.OCTOCODE_BULK_QUERY_TIMEOUT_MS || '60000', 10) || 60000;

const OUTER_TIMEOUT_MS =
  parseInt(process.env.OCTOCODE_TOOL_TIMEOUT_MS || '60000', 10) || 60000;

const MIN_QUERY_TIMEOUT_MS = 5_000;

export function computeQueryTimeout(
  queryCount: number,
  concurrency: number,
  minTimeoutMs?: number
): number {
  if (queryCount <= 1) {
    return minTimeoutMs
      ? Math.max(BULK_QUERY_TIMEOUT_MS, minTimeoutMs)
      : BULK_QUERY_TIMEOUT_MS;
  }
  const effectiveConcurrency = Math.min(Math.max(concurrency, 1), queryCount);
  const batches = Math.ceil(queryCount / effectiveConcurrency);
  const fair = Math.floor(OUTER_TIMEOUT_MS / batches);
  const computed = Math.max(
    MIN_QUERY_TIMEOUT_MS,
    Math.min(fair, BULK_QUERY_TIMEOUT_MS)
  );
  return minTimeoutMs ? Math.max(computed, minTimeoutMs) : computed;
}

export async function processBulkQueries<TQuery extends object>(
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

  const queryPromiseFunctions = queries.map((query, index) => async () => {
    const { value: result, cacheHit } = await traceResponseCache(() =>
      processor(query, index)
    );
    return {
      result:
        cacheHit && result.status !== 'error'
          ? { ...result, cache: 1 as const }
          : result,
      queryIndex: index,
      originalQuery: query,
    };
  });

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
        const processed = result.data.result;
        // ProcessedBulkResult carries its payload either under `data` or as
        // top-level keys (index signature) — "empty" means neither is present.
        const hasPayloadKeys =
          processed &&
          Object.keys(processed).some(
            k => k !== 'status' && k !== 'error' && k !== 'data'
          );
        const resolvedEmpty =
          processed &&
          processed.error === undefined &&
          (processed.data === null || processed.data === undefined) &&
          !hasPayloadKeys &&
          processed.status !== 'error';
        if (resolvedEmpty) {
          // Resolved without an error but carrying no payload (seen when a
          // heavy query dies mid-flight): surface an explicit error row —
          // a null payload presented as success reads as a false result.
          errors.push({
            queryIndex: result.data.queryIndex,
            error:
              'Query resolved without a payload (possible timeout or resource exhaustion) — retry this query alone, with tighter caps if it was large',
          });
          return;
        }
        results.push({
          result: result.data.result,
          queryIndex: result.data.queryIndex,
          originalQuery: result.data.originalQuery,
        });
      } else if (result.success) {
        // Fulfilled but with no data — record a typed error keyed by the query
        // index so this query still yields a row instead of vanishing. A silent
        // drop leaves a hole that misaligns downstream positional grouping.
        errors.push({
          queryIndex: result.index,
          error: 'Query produced no result',
        });
      }
    }
  );

  return { results, errors };
}
