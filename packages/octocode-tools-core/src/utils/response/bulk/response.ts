import { CallToolResult } from '@modelcontextprotocol/sdk/types';
import { incrementToolCharSavings } from '../../../shared/index.js';
import {
  cleanJsonObject,
  createResponseFormat,
  sanitizeStructuredContent,
} from '../../../responses.js';
import type {
  ProcessedBulkResult,
  FlatQueryResult,
  QueryError,
} from '../../../types/toolResults.js';
import type {
  BulkResponseConfig,
  BulkResponsePagination,
  BulkToolResponse,
} from '../../../types/bulk.js';
import { countSerializedChars, getRawResponseChars } from '../charSavings.js';
import { relativizeResultPaths, hoistSharedFields } from '../pathRelativize.js';
import { paginateBulkText, appendResponsePagination } from './pagination.js';
import {
  processBulkQueries,
  resolveQueryId,
  resolveUniqueQueryIds,
} from './queries.js';

const DEFAULT_BULK_CONCURRENCY = 3;

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

  const uniqueQueryIds = resolveUniqueQueryIds(queries);

  results.forEach(r => {
    const status = r.result.status;
    orderedQueries[r.queryIndex] = {
      id:
        uniqueQueryIds[r.queryIndex] ??
        resolveQueryId(r.originalQuery, r.queryIndex),
      ...(status !== undefined ? { status } : {}),
      data: extractToolData(r.result),
    };
  });

  errors.forEach(err => {
    const originalQuery = queries[err.queryIndex];
    if (!originalQuery) return;

    orderedQueries[err.queryIndex] = {
      id:
        uniqueQueryIds[err.queryIndex] ??
        resolveQueryId(originalQuery, err.queryIndex),
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
