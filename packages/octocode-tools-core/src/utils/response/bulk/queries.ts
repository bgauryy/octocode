import { executeWithErrorIsolation } from '../../core/promise.js';
import type {
  ProcessedBulkResult,
  QueryError,
} from '../../../types/toolResults.js';
import type { PromiseResult } from '../../../types/promise.js';

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

export function resolveQueryId<TQuery extends object>(
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

/**
 * Resolve one id per query, guaranteed unique within the batch. Everything
 * downstream (finalizer grouping, per-query pagination maps) keys off these
 * ids, so two queries submitted with the same explicit `id` would otherwise
 * silently merge and the second query's pagination would overwrite the first.
 * Collisions get a `#2`, `#3`… suffix in submission order.
 */
export function resolveUniqueQueryIds<TQuery extends object>(
  queries: readonly TQuery[]
): string[] {
  const seen = new Set<string>();
  return queries.map((query, index) => {
    let id = resolveQueryId(query, index);
    if (seen.has(id)) {
      let suffix = 2;
      while (seen.has(`${id}#${suffix}`)) suffix += 1;
      id = `${id}#${suffix}`;
    }
    seen.add(id);
    return id;
  });
}
