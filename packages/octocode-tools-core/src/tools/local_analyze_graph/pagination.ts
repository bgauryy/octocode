import { buildNextPageContinuation } from '../../scheme/pagination.js';
import { MAX_PAGE_NUMBER } from '../../config.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../toolNames.js';
import type { AnalyzeGraphOutput, AnalyzeGraphQuery } from './analysisTypes.js';

const DEFAULT_ITEMS_PER_PAGE = 50;
const DEFAULT_MAX_FILES = 20_000;
// Public schema bounds. Expansions stay executable by never exceeding them.
const MAX_QUERY_FILES = 50_000;
const MAX_QUERY_RESULTS = 5_000;

export function paginateGraphResults(
  items: Array<Record<string, unknown>>,
  query: AnalyzeGraphQuery
): Pick<
  AnalyzeGraphOutput,
  'results' | 'pagination' | 'truncated' | 'partialReasons' | 'totalAvailable'
> {
  const limited = query.limit ? items.slice(0, query.limit) : items;
  const limitTruncated = limited.length < items.length;
  const itemsPerPage = query.pageSize ?? DEFAULT_ITEMS_PER_PAGE;
  const requestedPage = query.page ?? 1;
  const totalPages = Math.max(1, Math.ceil(limited.length / itemsPerPage));
  const currentPage = Math.min(Math.max(1, requestedPage), totalPages);
  const start = (currentPage - 1) * itemsPerPage;
  return {
    results: limited.slice(start, start + itemsPerPage),
    pagination: {
      currentPage,
      totalPages,
      entriesPerPage: itemsPerPage,
      totalEntries: limited.length,
      hasMore: currentPage < totalPages,
      ...(requestedPage > totalPages ? { outOfRange: true } : {}),
    },
    ...(limitTruncated
      ? {
          truncated: true,
          partialReasons: ['limit' as const],
          totalAvailable: items.length,
        }
      : {}),
  };
}

function markScanCompleteness(
  output: AnalyzeGraphOutput,
  scanTruncated: boolean
): AnalyzeGraphOutput {
  const filesSkipped = (output.filesSkipped ?? 0) > 0;
  if (!scanTruncated && !filesSkipped) return output;
  return {
    ...output,
    truncated: true,
    partialReasons: [
      ...new Set([
        ...(output.partialReasons ?? []),
        ...(scanTruncated ? (['maxFiles'] as const) : []),
        ...(filesSkipped ? (['filesSkipped'] as const) : []),
      ]),
    ],
  };
}

function markTerminalLimit(
  output: AnalyzeGraphOutput,
  query: AnalyzeGraphQuery
): AnalyzeGraphOutput {
  const scanLimitReached =
    output.partialReasons?.includes('maxFiles') === true &&
    (query.maxFiles ?? DEFAULT_MAX_FILES) >= MAX_QUERY_FILES;
  const resultLimitReached =
    output.partialReasons?.includes('limit') === true &&
    query.limit !== undefined &&
    query.limit >= MAX_QUERY_RESULTS;
  const pageLimitReached =
    output.pagination?.hasMore === true &&
    output.pagination.currentPage >= MAX_PAGE_NUMBER;
  const skippedFilesCannotBePaged =
    output.partialReasons?.includes('filesSkipped') === true;
  return scanLimitReached ||
    resultLimitReached ||
    pageLimitReached ||
    skippedFilesCannotBePaged
    ? { ...output, terminalLimit: true }
    : output;
}

function addNextSteps(
  output: AnalyzeGraphOutput,
  query: AnalyzeGraphQuery,
  why: string
): AnalyzeGraphOutput {
  if (output.pagination?.outOfRange) {
    output = {
      ...output,
      warnings: [
        ...(output.warnings ?? []),
        `page:${query.page} is out of range (only ${output.pagination.totalPages} page(s)) — returned page ${output.pagination.currentPage} instead.`,
      ],
    };
  }
  const next: Record<string, unknown> = {};
  if (
    output.pagination?.hasMore &&
    output.pagination.currentPage < MAX_PAGE_NUMBER
  ) {
    next.nextPage = buildNextPageContinuation(
      'localAnalyzeGraph',
      { ...query, page: output.pagination.currentPage + 1 },
      why
    );
  }

  if (
    output.partialReasons?.includes('maxFiles') &&
    (query.maxFiles ?? DEFAULT_MAX_FILES) < MAX_QUERY_FILES
  ) {
    const currentMaxFiles = query.maxFiles ?? DEFAULT_MAX_FILES;
    next.expandScan = buildNextPageContinuation(
      'localAnalyzeGraph',
      {
        ...query,
        maxFiles: Math.min(
          MAX_QUERY_FILES,
          Math.max(currentMaxFiles + 1, currentMaxFiles * 2)
        ),
        page: 1,
      },
      'Re-run with a larger file-scan bound because this graph is partial.'
    );
  }

  if (
    output.partialReasons?.includes('limit') &&
    query.limit !== undefined &&
    query.limit < MAX_QUERY_RESULTS
  ) {
    next.expandLimit = buildNextPageContinuation(
      'localAnalyzeGraph',
      {
        ...query,
        limit: Math.min(
          MAX_QUERY_RESULTS,
          Math.max(query.limit + 1, query.limit * 2)
        ),
        page: 1,
      },
      'Re-run with a larger result limit because additional graph results exist.'
    );
  }

  if (query.operation === 'deadCode') {
    const candidate = output.results[0];
    if (
      candidate &&
      typeof candidate.file === 'string' &&
      typeof candidate.name === 'string' &&
      typeof candidate.line === 'number'
    ) {
      const root = query.path.replace(/\/+$/, '');
      next.verifyReferences = {
        tool: LSP_GET_SEMANTICS_TOOL_NAME,
        query: {
          type: 'references',
          uri: `${root}/${candidate.file}`,
          symbolName: candidate.name,
          lineHint: candidate.line,
          includeDeclaration: false,
          groupByFile: true,
        },
        why: `Verify candidate "${candidate.name}" before deletion; repeat for each result, prioritizing viaHeuristic:"reexport-chain".`,
        confidence: 'high',
      };
    }
  }

  return Object.keys(next).length > 0 ? { ...output, next } : output;
}

export function finalizeGraphOutput(
  output: AnalyzeGraphOutput,
  query: AnalyzeGraphQuery,
  scanTruncated: boolean,
  why: string
): AnalyzeGraphOutput {
  return addNextSteps(
    markTerminalLimit(markScanCompleteness(output, scanTruncated), query),
    query,
    why
  );
}
