import { matchContext } from '../../../utils/file/matchContext.js';
import { buildSecurityLimitResult } from './pagination.js';
import { selectMatchingSource } from '../../../utils/file/contentExtractor.js';
import { countLines } from '../../../utils/core/lines.js';
import type { FetchContentQuery } from '@octocodeai/octocode-core/schema';
import { LOCAL_TOOL_ERROR_CODES } from '../../../errors/localToolErrors.js';
import { createNoMatchesResult } from './validation.js';
import type { ExtractionState } from './types.js';

export type { ExtractionState } from './types.js';

function buildMatchExtractionState(
  query: FetchContentQuery,
  totalLines: number,
  sourceRecords: string[]
): ExtractionState {
  const context = matchContext(query);
  const result = selectMatchingSource(
    sourceRecords.join(''),
    query.matchString!,
    context.contextLines ?? 0,
    query.matchStringIsRegex ?? false,
    query.matchStringCaseSensitive ?? false,
    context.contextBytes,
    query.path
  );
  if (result.securityLimited)
    return { earlyResult: buildSecurityLimitResult(query, totalLines) };
  if (result.matchCount === 0) {
    return { earlyResult: createNoMatchesResult(query, totalLines) };
  }
  let actualStartLine: number | undefined;
  let actualEndLine: number | undefined;
  let matchRanges: Array<{ start: number; end: number }> | undefined;

  if (result.matchRanges.length > 0) {
    const firstRange = result.matchRanges[0];
    const lastRange = result.matchRanges[result.matchRanges.length - 1];
    if (firstRange && lastRange) {
      actualStartLine = firstRange.start;
      actualEndLine = lastRange.end;
      // Always emit matchRanges — startLine/endLine include ±context lines,
      // so for a single match they do NOT pinpoint the matched line; without
      // this the only structured anchor for lspSearch lineHint is lost.
      matchRanges = result.matchRanges;
    }
  }

  return {
    resultContent: result.content,
    warnings: result.warnings,
    sourceLines: result.sourceLines,
    actualStartLine,
    actualEndLine,
    matchRanges,
    matchedLines: result.matchingLines,
    selectedMatchCount: result.matchCount,
  };
}

function hasLineRangeRequest(query: FetchContentQuery): boolean {
  return query.startLine !== undefined && query.endLine !== undefined;
}

function buildLineRangeExtractionState(
  query: FetchContentQuery,
  lines: string[],
  totalLines: number
): ExtractionState {
  const requestedStartLine = query.startLine!;
  const requestedEndLine = query.endLine!;
  const effectiveStartLine = Math.max(1, requestedStartLine);
  const effectiveEndLine = Math.min(requestedEndLine, totalLines);

  if (requestedEndLine < requestedStartLine) {
    return {
      earlyResult: {
        status: 'empty',
        totalLines,
        errorCode: LOCAL_TOOL_ERROR_CODES.NO_MATCHES,
        warnings: [
          `startLine ${requestedStartLine} is greater than endLine ${requestedEndLine} — startLine must be ≤ endLine`,
          `Use startLine=1 to ${totalLines} with startLine ≤ endLine for a valid range`,
        ],
      },
    };
  }

  if (effectiveStartLine > totalLines) {
    return {
      earlyResult: {
        status: 'empty',
        totalLines,
        errorCode: LOCAL_TOOL_ERROR_CODES.NO_MATCHES,
        warnings: [
          `Requested startLine ${requestedStartLine} exceeds file length (${totalLines} lines)`,
          `Use startLine=1 to ${totalLines} for valid range`,
        ],
      },
    };
  }

  const warnings: string[] = [];
  if (requestedEndLine > totalLines) {
    warnings.push(
      `Requested endLine ${requestedEndLine} adjusted to ${totalLines} (file end)`
    );
  }

  return {
    resultContent: lines
      .slice(effectiveStartLine - 1, effectiveEndLine)
      .join(''),
    sourceLines: Array.from(
      { length: effectiveEndLine - effectiveStartLine + 1 },
      (_, index) => effectiveStartLine + index
    ),
    actualStartLine: effectiveStartLine,
    actualEndLine: effectiveEndLine,
    warnings,
  };
}

export function buildExtractionState(
  query: FetchContentQuery,
  content: string
): ExtractionState {
  const lines = content.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  const totalLines = countLines(content);

  if (query.matchString) {
    return buildMatchExtractionState(query, totalLines, lines);
  }

  if (hasLineRangeRequest(query)) {
    return buildLineRangeExtractionState(query, lines, totalLines);
  }

  return {
    resultContent: content,
  };
}
