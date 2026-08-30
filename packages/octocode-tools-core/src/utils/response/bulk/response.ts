import { CallToolResult } from '@modelcontextprotocol/server';
import { incrementToolCharSavings } from '../../../shared/index.js';
import type {
  ProcessedBulkResult,
  FlatQueryResult,
  EvidenceKind,
  QueryError,
} from '../../../types/toolResults.js';
import type {
  BulkResponseConfig,
  BulkResponsePagination,
  BulkToolResponse,
} from '../../../types/bulk.js';
import { countSerializedChars, getRawResponseChars } from '../charSavings.js';
import { buildResponseChannels } from '../responseChannels.js';

import { paginateBulkText, appendResponsePagination } from './pagination.js';
import { processBulkQueries } from './queries.js';

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
  const resultFields = [
    'index',
    'status',
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
    const data = extractToolData(r.result);
    orderedQueries[r.queryIndex] = {
      index: r.queryIndex,
      ...(status !== undefined ? { status } : {}),
      meta: buildToolResultMeta(config.toolName, r.originalQuery, data, status),
      data,
    };
  });

  errors.forEach(err => {
    const originalQuery = queries[err.queryIndex];
    if (!originalQuery) return;

    orderedQueries[err.queryIndex] = {
      index: err.queryIndex,
      status: 'error',
      meta: buildToolResultMeta(
        config.toolName,
        originalQuery,
        { error: err.error },
        'error'
      ),
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
      attachFinalizedResultMeta(finalized.structuredContent, flatQueries),
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

  const responseChannels = buildResponseChannels(
    responseData,
    fullKeysPriority
  );
  const formattedText = responseChannels.text;
  const paginated = paginateBulkText(formattedText, pagination);
  const structuredContent = appendResponsePagination(
    responseChannels.structuredContent as unknown as Record<string, unknown>,
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

function attachFinalizedResultMeta<TOutput extends Record<string, unknown>>(
  structuredContent: TOutput,
  sourceRows: FlatQueryResult[]
): TOutput {
  if (!Array.isArray(structuredContent.results)) return structuredContent;
  const byIndex = new Map(sourceRows.map(row => [row.index, row]));
  const results = structuredContent.results.map((value, index) => {
    if (value === null || typeof value !== 'object' || Array.isArray(value)) {
      return value;
    }
    const row = value as Record<string, unknown>;
    const source =
      (typeof row.index === 'number' ? byIndex.get(row.index) : undefined) ??
      sourceRows[index];
    return source && row.meta === undefined
      ? { ...row, meta: source.meta }
      : row;
  });
  return { ...structuredContent, results };
}

export function buildToolResultMeta(
  toolName: string,
  query: object,
  data: Record<string, unknown>,
  status?: 'empty' | 'error'
): FlatQueryResult['meta'] {
  const kind = inferEvidenceKind(toolName, query);
  const reportedConfidence = data.confidence;
  const confidence =
    status === 'error' || reportedConfidence === 'low'
      ? 'low'
      : reportedConfidence === 'high' || reportedConfidence === 'medium'
        ? reportedConfidence
        : kind === 'provider' || kind === 'lexical' || kind === 'syntactic'
          ? 'medium'
          : 'high';
  const codes =
    typeof data.errorCode === 'string' ? [data.errorCode] : undefined;
  const pagination =
    data.pagination && typeof data.pagination === 'object'
      ? (data.pagination as Record<string, unknown>)
      : undefined;
  const partial =
    data.truncated === true ||
    data.isPartial === true ||
    pagination?.hasMore === true;
  // Existing result/finalizer hints remain in their established location.
  // Copying them into metadata doubles response bytes without adding evidence.
  const hasDiagnostics = (codes?.length ?? 0) > 0 || partial;

  return {
    evidence: { kind, confidence },
    ...(hasDiagnostics
      ? {
          diagnostics: {
            ...(codes ? { codes } : {}),
            ...(partial ? { partial: true } : {}),
          },
        }
      : {}),
  };
}

function inferEvidenceKind(toolName: string, query: object): EvidenceKind {
  if (toolName === 'localAnalyzeGraph') return 'syntactic';
  if (toolName === 'lspGetSemantics') return 'semantic';
  if (toolName === 'localSearchCode') {
    return (query as Record<string, unknown>).mode === 'structural'
      ? 'structural'
      : 'lexical';
  }
  if (toolName.startsWith('gh') || toolName === 'npmSearch') return 'provider';
  return 'exact';
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
    'goal',
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
