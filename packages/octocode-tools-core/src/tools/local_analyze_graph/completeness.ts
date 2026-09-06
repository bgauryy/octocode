import type { AnalyzeGraphOutput } from './analysisTypes.js';

const COVERAGE_GAP_REASONS = [
  'parseRecovery',
  'unresolvedImports',
  'unsupportedLinking',
] as const;

type CoverageGapReason = (typeof COVERAGE_GAP_REASONS)[number];

function isCoverageGapReason(reason: string): reason is CoverageGapReason {
  return COVERAGE_GAP_REASONS.some(candidate => candidate === reason);
}

/** Keep result, graph, and diagnostic completeness independent and explicit. */
export function attachGraphCompleteness(
  output: AnalyzeGraphOutput
): AnalyzeGraphOutput {
  const reasons = output.partialReasons ?? [];
  const coverageGapReasons = reasons.filter(isCoverageGapReason);
  const scanIncomplete = reasons.some(reason =>
    ['maxFiles', 'filesSkipped'].includes(reason)
  );
  const resultTruncated = reasons.some(reason =>
    ['limit', 'maxFiles', 'filesSkipped'].includes(reason)
  );
  const diagnosticsPagination = output.coverage?.diagnosticsPagination;
  return {
    ...output,
    completeness: {
      results: output.pagination?.hasMore
        ? 'pageable'
        : resultTruncated
          ? 'truncated'
          : 'complete',
      graph: scanIncomplete
        ? 'scan-truncated'
        : coverageGapReasons.length > 0
          ? 'coverage-incomplete'
          : 'complete',
      diagnostics: diagnosticsPagination?.terminalLimit
        ? 'truncated'
        : diagnosticsPagination?.hasMore
          ? 'pageable'
          : 'complete',
      ...(coverageGapReasons.length > 0 ? { coverageGapReasons } : {}),
    },
  };
}
