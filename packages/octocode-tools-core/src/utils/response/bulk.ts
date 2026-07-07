import { CallToolResult } from '@modelcontextprotocol/sdk/types';
import { incrementToolCharSavings } from '../../shared/index.js';
import { executeWithErrorIsolation } from '../core/promise.js';
import {
  cleanJsonObject,
  createResponseFormat,
  sanitizeStructuredContent,
} from '../../responses.js';
import type {
  ProcessedBulkResult,
  FlatQueryResult,
  QueryError,
} from '../../types/toolResults.js';
import type {
  BulkResponseConfig,
  BulkResponsePagination,
  BulkToolResponse,
} from '../../types/bulk.js';
import type { PromiseResult } from '../../types/promise.js';
import { countSerializedChars, getRawResponseChars } from './charSavings.js';
import { relativizeResultPaths, hoistSharedFields } from './pathRelativize.js';

const DEFAULT_BULK_CONCURRENCY = 3;

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

export async function executeBulkOperation<
  TQuery extends object,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
>(
  queries: Array<TQuery>,
  processor: (query: TQuery, index: number) => Promise<ProcessedBulkResult>,
  config: BulkResponseConfig<TQuery, TOutput>,
  pagination?: BulkResponsePagination
): Promise<CallToolResult> {
  const concurrency = config.concurrency ?? DEFAULT_BULK_CONCURRENCY;
  const { results, errors } = await processBulkQueries<TQuery>(
    queries,
    processor,
    concurrency,
    config.minQueryTimeoutMs
  );
  return createBulkResponse<TQuery, TOutput>(
    config,
    results,
    errors,
    queries,
    pagination
  );
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
  queries: Array<TQuery>,
  pagination?: BulkResponsePagination
): CallToolResult {
  const topLevelFields = ['results', 'base', 'shared'];
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

  if (config.finalize) {
    const finalized = config.finalize({
      queries,
      results: flatQueries,
      config,
    });
    const paginated = paginateBulkText(finalized.text, pagination);
    const structuredContent = appendResponsePagination(
      finalized.structuredContent,
      paginated.pagination
    );
    recordBulkCharSavings(
      config.toolName,
      results,
      errors,
      paginated.text.length
    );
    const text = paginated.text;
    return {
      content: [{ type: 'text' as const, text }],
      structuredContent,
      isError:
        finalized.isError ??
        (flatQueries.length > 0 &&
          flatQueries.every(queryResult => queryResult.status === 'error')),
    };
  }

  const responseData: BulkToolResponse = { results: flatQueries };

  if (Array.isArray(responseData.results)) {
    const dataBase = relativizeResultPaths(
      responseData.results as Array<{ data?: unknown }>
    );
    if (dataBase) responseData.base = dataBase;

    const shared = hoistSharedFields(
      responseData.results as Array<{ data?: unknown }>
    );
    if (shared) responseData.shared = shared;
  }

  const formattedText = createResponseFormat(responseData, fullKeysPriority);
  const paginated = paginateBulkText(formattedText, pagination);
  // Clean before sanitizing so structuredContent matches the text channel:
  // compactMcpTextContent points MCP agents at structuredContent, so it must
  // carry the same trimmed payload the text channel already produces via
  // createResponseFormat (cleanJsonObject → sanitize).
  const structuredContent = appendResponsePagination(
    sanitizeStructuredContent(cleanJsonObject(responseData) ?? {}) as Record<
      string,
      unknown
    >,
    paginated.pagination
  );
  recordBulkCharSavings(
    config.toolName,
    results,
    errors,
    paginated.text.length
  );
  const text = paginated.text;

  return {
    content: [
      {
        type: 'text' as const,
        text,
      },
    ],
    structuredContent,
    isError:
      flatQueries.length > 0 &&
      flatQueries.every(queryResult => queryResult.status === 'error'),
  };
}

function chooseLineAwareEndOffset(
  text: string,
  startOffset: number,
  pageLength: number
): number {
  const rawEndOffset = Math.min(startOffset + pageLength, text.length);
  if (rawEndOffset >= text.length) return text.length;

  const minimumUsefulPageLength = Math.max(1, Math.floor(pageLength / 2));
  const boundaryOffsets = [
    text.lastIndexOf('\n', rawEndOffset - 1) + 1,
    text.lastIndexOf('\\n', rawEndOffset - 1) + 2,
  ].filter(offset => offset > startOffset && offset <= rawEndOffset);

  const bestBoundaryOffset = Math.max(...boundaryOffsets, -1);
  if (bestBoundaryOffset - startOffset >= minimumUsefulPageLength) {
    return bestBoundaryOffset;
  }

  return rawEndOffset;
}

function calculateLineAwarePageNumber(
  text: string,
  offset: number,
  pageLength: number
): number {
  let page = 1;
  let cursor = 0;

  while (cursor < offset && cursor < text.length) {
    const nextCursor = chooseLineAwareEndOffset(text, cursor, pageLength);
    if (nextCursor <= cursor) break;
    cursor = nextCursor;
    page += 1;
  }

  return cursor === offset ? page : Math.floor(offset / pageLength) + 1;
}

function calculateLineAwareTotalPages(
  text: string,
  pageLength: number
): number {
  if (text.length === 0) return 1;

  let pages = 0;
  let cursor = 0;

  while (cursor < text.length) {
    const nextCursor = chooseLineAwareEndOffset(text, cursor, pageLength);
    if (nextCursor <= cursor)
      return Math.max(1, Math.ceil(text.length / pageLength));
    cursor = nextCursor;
    pages += 1;
  }

  return Math.max(1, pages);
}

function paginateBulkText(
  text: string,
  pagination?: BulkResponsePagination
): {
  text: string;
  pagination?: NonNullable<BulkToolResponse['responsePagination']>;
} {
  const requestedLength = pagination?.responseCharLength;
  const requestedOffset = pagination?.responseCharOffset ?? 0;
  if (requestedLength === undefined) {
    return { text };
  }

  const totalChars = text.length;
  const safeLength = Math.max(1, requestedLength);
  const safeOffset = Math.min(Math.max(0, requestedOffset), totalChars);
  const endOffset = chooseLineAwareEndOffset(text, safeOffset, safeLength);
  const hasMore = endOffset < totalChars;
  const currentPage = calculateLineAwarePageNumber(
    text,
    safeOffset,
    safeLength
  );
  const totalPages = calculateLineAwareTotalPages(text, safeLength);

  const pageText = text.slice(safeOffset, endOffset);
  const header = hasMore
    ? `# Response page ${currentPage}/${totalPages}. Next: responseCharOffset=${endOffset}\n`
    : `# Response page ${currentPage}/${totalPages}.\n`;

  return {
    text: `${header}${pageText}`,
    pagination: {
      currentPage,
      totalPages,
      hasMore,
      charOffset: safeOffset,
      charLength: endOffset - safeOffset,
      totalChars,
      ...(hasMore ? { nextCharOffset: endOffset } : {}),
    },
  };
}

function appendResponsePagination<T extends Record<string, unknown>>(
  structuredContent: T,
  pagination?: NonNullable<BulkToolResponse['responsePagination']>
): T {
  if (!pagination) return structuredContent;
  // The responsePagination object carries the cursor; restating it as a hint is
  // redundant token waste. The page banner remains in the text channel header.
  return {
    ...structuredContent,
    responsePagination: pagination,
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
    void 0;
  }
}

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
      toolData[key] = value;
    }
  }

  return toolData;
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
