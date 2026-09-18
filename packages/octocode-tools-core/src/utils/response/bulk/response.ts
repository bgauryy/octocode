import { CallToolResult } from '@modelcontextprotocol/server';
import { inferEvidenceKind } from './evidence.js';
import { incrementToolCharSavings } from '../../../shared/session/index.js';
import type {
  ProcessedBulkResult,
  FlatQueryResult,
  QueryError,
  ToolResultMeta,
} from '../../../types/toolResults.js';
import type {
  BulkResponseConfig,
  BulkResponseExecutionOptions,
  BulkToolResponse,
} from '../../../types/bulk.js';
import { countSerializedChars, getRawResponseChars } from '../charSavings.js';
import { buildResponseChannels } from '../responseChannels.js';
import {
  paginateBulkText,
  appendResponsePagination,
  buildResponsePaginationContinuation,
} from './pagination.js';
import {
  buildPaginationDiagnosticCodes,
  isPartialResult,
} from './paginationDiagnostics.js';
import { processBulkQueries } from './queries.js';
import { preserveContinuationMetadata } from './continuationMetadata.js';

const DEFAULT_BULK_CONCURRENCY = 3;

function isDebugQuery(query: unknown): boolean {
  return (
    query !== null &&
    typeof query === 'object' &&
    (query as Record<string, unknown>).debug === true
  );
}

export async function executeBulkOperation<
  TQuery extends object,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
>(
  queries: Array<TQuery>,
  processor: (query: TQuery, index: number) => Promise<ProcessedBulkResult>,
  config: BulkResponseConfig<TQuery, TOutput>,
  execution?: BulkResponseExecutionOptions
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
    execution
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
  execution?: BulkResponseExecutionOptions
): CallToolResult {
  const topLevelFields = ['results', 'base', 'shared'];
  const resultFields = [
    'index',
    'status',
    'cache',
    'meta',
    'evidence',
    'diagnostics',
    'data',
  ];
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
    const data = preserveContinuationMetadata(
      extractToolData(r.result),
      r.originalQuery as Readonly<Record<string, unknown>>
    ) as Record<string, unknown>;
    orderedQueries[r.queryIndex] = {
      index: r.queryIndex,
      ...(status !== undefined ? { status } : {}),
      ...(r.result.cache === 1 ? { cache: 1 as const } : {}),
      ...(isDebugQuery(r.originalQuery)
        ? {
            meta: buildToolResultMeta(
              config.toolName,
              r.originalQuery,
              data,
              status
            ),
          }
        : {}),
      data,
    };
  });

  errors.forEach(err => {
    const originalQuery = queries[err.queryIndex];
    if (!originalQuery) return;

    orderedQueries[err.queryIndex] = {
      index: err.queryIndex,
      status: 'error',
      ...(isDebugQuery(originalQuery)
        ? {
            meta: buildToolResultMeta(
              config.toolName,
              originalQuery,
              { error: err.error },
              'error'
            ),
          }
        : {}),
      data: { error: err.error },
    };
  });

  const flatQueries = orderedQueries.filter(
    (query): query is FlatQueryResult => query !== undefined
  );
  const finalized = config.finalize?.({
    queries,
    results: flatQueries,
    config,
  });
  const responseData: Record<string, unknown> = finalized
    ? attachFinalizedResultMeta(
        finalized.structuredContent,
        flatQueries,
        queries
      )
    : ({ results: flatQueries } satisfies BulkToolResponse);
  const renderText = shouldRenderText(
    execution,
    flatQueries,
    finalized?.isError === true
  );
  const responseChannels = buildResponseChannels(
    responseData,
    finalized?.keysPriority ?? fullKeysPriority,
    { toolName: config.toolName, queries },
    { renderText: renderText && !finalized?.renderText }
  );
  const text = renderText
    ? finalized?.renderText
      ? finalized.renderText(responseChannels.structuredContent)
      : responseChannels.text!
    : undefined;
  const paginated =
    text === undefined ? undefined : paginateBulkText(text, execution);
  const structuredContent = paginated
    ? appendResponsePagination(
        responseChannels.structuredContent,
        paginated.pagination,
        buildResponsePaginationContinuation(
          config.toolName,
          queries,
          execution,
          paginated.pagination
        )
      )
    : responseChannels.structuredContent;
  recordBulkCharSavings(
    config.toolName,
    results,
    errors,
    paginated?.text.length ?? countSerializedChars(structuredContent)
  );
  return {
    content: paginated ? [{ type: 'text' as const, text: paginated.text }] : [],
    structuredContent,
    isError:
      finalized?.isError ??
      (flatQueries.length > 0 &&
        flatQueries.every(queryResult => queryResult.status === 'error')),
  };
}

function shouldRenderText(
  execution: BulkResponseExecutionOptions | undefined,
  results: FlatQueryResult[],
  finalizedIsError: boolean = false
): boolean {
  if (
    execution?.renderText !== false ||
    finalizedIsError ||
    results.some(result => result.status === 'error')
  ) {
    return true;
  }
  return (
    execution.responseCharLength !== undefined ||
    execution.responseCharOffset !== undefined ||
    execution.responseSnapshot !== undefined
  );
}

function attachFinalizedResultMeta<
  TQuery extends object,
  TOutput extends Record<string, unknown>,
>(
  structuredContent: TOutput,
  sourceRows: FlatQueryResult[],
  originalQueries: TQuery[]
): TOutput {
  if (!Array.isArray(structuredContent.results)) return structuredContent;
  const byIndex = new Map(sourceRows.map(row => [row.index, row]));
  const shared =
    structuredContent.shared !== null &&
    typeof structuredContent.shared === 'object' &&
    !Array.isArray(structuredContent.shared)
      ? (structuredContent.shared as Record<string, unknown>)
      : undefined;
  const results = structuredContent.results.map((value, index) => {
    if (value === null || typeof value !== 'object' || Array.isArray(value)) {
      return value;
    }
    const row = value as Record<string, unknown>;
    const source =
      (typeof row.index === 'number' ? byIndex.get(row.index) : undefined) ??
      sourceRows[index];
    if (!source) return row;
    const { cache: _untrustedCacheMarker, ...rawFinalizedRow } = row;
    const originalQuery = originalQueries[source.index] as
      Readonly<Record<string, unknown>> | undefined;
    const finalizedRow = originalQuery
      ? (preserveContinuationMetadata(rawFinalizedRow, originalQuery) as Record<
          string,
          unknown
        >)
      : rawFinalizedRow;
    const data =
      row.data !== null &&
      typeof row.data === 'object' &&
      !Array.isArray(row.data)
        ? (row.data as Record<string, unknown>)
        : source.data;
    const sourceMeta =
      (row.meta as FlatQueryResult['meta'] | undefined) ?? source.meta;
    const meta = sourceMeta
      ? reconcilePaginationDiagnostics(
          sourceMeta,
          shared ? { ...shared, ...data } : data
        )
      : undefined;
    return {
      ...finalizedRow,
      ...(source.cache === 1 ? { cache: 1 } : {}),
      ...(meta ? { meta } : {}),
    };
  });
  return { ...structuredContent, results };
}

export function buildToolResultMeta(
  toolName: string,
  query: object,
  data: Record<string, unknown>,
  status?: 'empty' | 'error'
): ToolResultMeta {
  const kind = inferEvidenceKind(toolName, query, data);
  const reportedConfidence = data.confidence;
  const confidence =
    status === 'error' || reportedConfidence === 'low'
      ? 'low'
      : reportedConfidence === 'high' || reportedConfidence === 'medium'
        ? reportedConfidence
        : kind === 'provider' || kind === 'lexical' || kind === 'syntactic'
          ? 'medium'
          : 'high';
  const partial = isPartialResult(data);
  const codes = [
    ...(typeof data.errorCode === 'string' ? [data.errorCode] : []),
    ...buildPaginationDiagnosticCodes(data),
  ];
  // Existing result/finalizer hints remain in their established location.
  // Copying them into metadata doubles response bytes without adding evidence.
  const hasDiagnostics = codes.length > 0 || partial;

  return {
    evidence: { kind, confidence },
    ...(hasDiagnostics
      ? {
          diagnostics: {
            ...(codes.length > 0 ? { codes } : {}),
            ...(partial ? { partial: true } : {}),
          },
        }
      : {}),
  };
}

function reconcilePaginationDiagnostics(
  meta: ToolResultMeta,
  data: Record<string, unknown>
): ToolResultMeta {
  const { diagnostics, ...stableMeta } = meta;
  const codes = [
    ...(diagnostics?.codes ?? []).filter(
      code => code !== 'continuationMissing' && code !== 'terminalLimitReached'
    ),
    ...buildPaginationDiagnosticCodes(data),
  ];
  const partial = isPartialResult(data);
  const hasDiagnostics =
    codes.length > 0 || partial || (diagnostics?.hints?.length ?? 0) > 0;

  return {
    ...stableMeta,
    ...(hasDiagnostics
      ? {
          diagnostics: {
            ...(codes.length > 0 ? { codes } : {}),
            ...(diagnostics?.hints?.length ? { hints: diagnostics.hints } : {}),
            ...(partial ? { partial: true } : {}),
          },
        }
      : {}),
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
    'cache',
    'goal',
    'reasoning',
    'debug',
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
