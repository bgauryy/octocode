import { buildNextPageContinuation } from '../../scheme/pagination.js';
import { MAX_PAGE_NUMBER } from '../../config.js';
import { prepareGraphDiagnostics } from '../../graph/diagnosticSnapshot.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../toolNames.js';
import type { AnalyzeGraphOutput, AnalyzeGraphQuery } from './analysisTypes.js';
import { attachGraphCompleteness } from './completeness.js';

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
  const diagnostics = output.coverage?.diagnostics ?? [];
  const parseRecovery = diagnostics.some(
    item => item.code === 'parse-recovery'
  );
  const unresolvedImports = diagnostics.some(
    item => item.code === 'unresolved-internal'
  );
  const unsupportedLinking = diagnostics.some(
    item => item.code === 'unsupported-linking'
  );
  if (
    !scanTruncated &&
    !filesSkipped &&
    !parseRecovery &&
    !unresolvedImports &&
    !unsupportedLinking
  )
    return output;
  return {
    ...output,
    truncated: true,
    partialReasons: [
      ...new Set([
        ...(output.partialReasons ?? []),
        ...(scanTruncated ? (['maxFiles'] as const) : []),
        ...(filesSkipped ? (['filesSkipped'] as const) : []),
        ...(parseRecovery ? (['parseRecovery'] as const) : []),
        ...(unresolvedImports ? (['unresolvedImports'] as const) : []),
        ...(unsupportedLinking ? (['unsupportedLinking'] as const) : []),
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
  const canExpandScan =
    output.partialReasons?.includes('maxFiles') === true &&
    (query.maxFiles ?? DEFAULT_MAX_FILES) < MAX_QUERY_FILES;
  const semanticCoverageCannotBePaged =
    output.partialReasons?.some(
      reason =>
        ['parseRecovery', 'unsupportedLinking'].includes(reason) ||
        (reason === 'unresolvedImports' && !canExpandScan)
    ) === true;
  return scanLimitReached ||
    resultLimitReached ||
    pageLimitReached ||
    skippedFilesCannotBePaged ||
    semanticCoverageCannotBePaged
    ? {
        ...output,
        terminalLimit: true,
        ...(semanticCoverageCannotBePaged
          ? {
              warnings: [
                ...(output.warnings ?? []),
                'Graph coverage is incomplete; parser recovery or unsupported/unresolved module linking cannot be repaired by pagination. Inspect coverage.diagnostics and verify with LSP.',
              ],
            }
          : {}),
      }
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

  // The analyzer resolves path before calling this helper, but keep this
  // exported continuation builder safe for direct callers as well. A missing
  // root must not turn into a relative LSP URI such as "/dead.ts".
  const graphRoot = query.path;
  if (query.operation === 'deadCode' && graphRoot) {
    const candidate = output.results[0];
    if (
      candidate &&
      typeof candidate.file === 'string' &&
      typeof candidate.name === 'string' &&
      typeof candidate.line === 'number'
    ) {
      const root = graphRoot.replace(/\/+$/, '');
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

  // Expansion changes the scanned diagnostic set; begin its diagnostic stream
  // afresh instead of carrying a snapshot belonging to the smaller graph.
  for (const key of ['expandScan', 'expandLimit']) {
    const step = next[key] as { query: AnalyzeGraphQuery } | undefined;
    if (!step) continue;
    delete step.query.diagnosticSnapshot;
    step.query.diagnosticPage = 1;
  }

  return Object.keys(next).length > 0 ? { ...output, next } : output;
}

export function finalizeGraphOutput(
  output: AnalyzeGraphOutput,
  query: AnalyzeGraphQuery,
  scanTruncated: boolean,
  why: string
): AnalyzeGraphOutput {
  return attachGraphCompleteness(
    paginateGraphDiagnostics(
      addNextSteps(
        markTerminalLimit(markScanCompleteness(output, scanTruncated), query),
        query,
        why
      ),
      query
    )
  );
}

function paginateGraphDiagnostics(
  output: AnalyzeGraphOutput,
  query: AnalyzeGraphQuery
): AnalyzeGraphOutput {
  const coverage = output.coverage;
  if (!coverage) return output;
  const { diagnostics, resultId, diagnosticCounts } = prepareGraphDiagnostics(
    coverage.diagnostics
  );
  const { diagnosticSnapshot: _previousSnapshot, ...restartQuery } = query;
  const restartDiagnostics = buildNextPageContinuation(
    'localAnalyzeGraph',
    { ...restartQuery, diagnosticPage: 1 },
    'Restart diagnostic pagination from the current diagnostic snapshot.'
  );
  if (query.diagnosticSnapshot && query.diagnosticSnapshot !== resultId) {
    return {
      ...output,
      status: 'error',
      errorCode: 'graphDiagnosticsChanged',
      error:
        'Graph diagnostics changed between pages. Restart before combining diagnostic pages.',
      results: [],
      coverage: { ...coverage, diagnostics: [], diagnosticCounts },
      next: { restartDiagnostics },
    };
  }
  const entriesPerPage = Math.max(
    1,
    Math.min(query.diagnosticPageSize ?? 25, 100)
  );
  const requestedPage = query.diagnosticPage ?? 1;
  const totalPages = Math.max(
    1,
    Math.ceil(diagnostics.length / entriesPerPage)
  );
  const currentPage = Math.max(
    1,
    Math.min(requestedPage, totalPages, MAX_PAGE_NUMBER)
  );
  const hasMore = currentPage < totalPages;
  const pageLimitReached = hasMore && currentPage >= MAX_PAGE_NUMBER;
  const terminalLimit = pageLimitReached && entriesPerPage === 100;
  const outOfRange = requestedPage > totalPages;
  const start = (currentPage - 1) * entriesPerPage;
  const next = { ...output.next };
  if (hasMore && !pageLimitReached) {
    next.nextDiagnostics = buildNextPageContinuation(
      'localAnalyzeGraph',
      {
        ...query,
        diagnosticPage: currentPage + 1,
        diagnosticPageSize: entriesPerPage,
        diagnosticSnapshot: resultId,
      },
      'Continue coverage diagnostics from the same diagnostic snapshot.'
    );
  }
  if (pageLimitReached && !terminalLimit) {
    const largerPageSize = Math.min(100, entriesPerPage * 2);
    next.nextDiagnostics = buildNextPageContinuation(
      'localAnalyzeGraph',
      {
        ...query,
        diagnosticPage: (currentPage * entriesPerPage) / largerPageSize + 1,
        diagnosticPageSize: largerPageSize,
        diagnosticSnapshot: resultId,
      },
      'Continue the diagnostic snapshot with larger pages from the next unread row.'
    );
  }
  if (outOfRange) next.restartDiagnostics = restartDiagnostics;
  return {
    ...output,
    coverage: {
      ...coverage,
      diagnosticCounts,
      diagnostics: diagnostics.slice(start, start + entriesPerPage),
      diagnosticsPagination: {
        currentPage,
        totalPages,
        entriesPerPage,
        totalEntries: diagnostics.length,
        hasMore,
        resultId,
        ...(outOfRange ? { outOfRange: true } : {}),
        ...(terminalLimit ? { terminalLimit: true } : {}),
      },
    },
    ...(Object.keys(next).length ? { next } : {}),
    ...(hasMore
      ? {
          truncated: true,
          partialReasons: [
            ...new Set([
              ...(output.partialReasons ?? []),
              'diagnosticPage' as const,
            ]),
          ],
        }
      : {}),
    ...(terminalLimit ? { terminalLimit: true } : {}),
    ...(outOfRange || terminalLimit
      ? {
          warnings: [
            ...(output.warnings ?? []),
            outOfRange
              ? `diagnosticPage:${requestedPage} is out of range; returned diagnostic page ${currentPage}.`
              : 'Diagnostic pagination reached its page limit; remaining diagnostics were not returned. Narrow the graph scope.',
          ],
        }
      : {}),
  };
}
