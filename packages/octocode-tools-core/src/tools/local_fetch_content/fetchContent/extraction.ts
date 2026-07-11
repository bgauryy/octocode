import { extractMatchingLines } from '../contentExtractor.js';
import { countLines } from '../../../utils/core/lines.js';
import type { LocalGetFileContentToolResult } from '@octocodeai/octocode-core/extra-types';
import type { FetchContentQuery } from '../scheme.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../../errors/localToolErrors.js';
import { createNoMatchesResult } from './validation.js';

export interface ExtractionState {
  resultContent?: string;
  isPartial: boolean;
  actualStartLine?: number;
  actualEndLine?: number;
  matchRanges?: Array<{ start: number; end: number }>;
  warnings?: string[];
  earlyResult?: LocalGetFileContentToolResult;
}

function buildMatchExtractionState(
  query: FetchContentQuery,
  lines: string[],
  totalLines: number
): ExtractionState {
  const result = extractMatchingLines(
    lines,
    query.matchString!,
    (query as { contextLines?: number }).contextLines ?? 5,
    query.matchStringIsRegex ?? false,
    query.matchStringCaseSensitive ?? false
  );

  if (result.lines.length === 0) {
    return {
      isPartial: false,
      earlyResult: createNoMatchesResult(query, totalLines),
    };
  }

  const resultContent = result.lines.join('\n');
  const contextLines = (query as { contextLines?: number }).contextLines ?? 5;
  const shownLines = result.matchingLines.slice(0, 10).join(', ');
  const extraCount =
    result.matchingLines.length > 10
      ? ` (+${result.matchingLines.length - 10} more)`
      : '';
  const matchSummary = `Found ${result.matchCount} occurrence${result.matchCount === 1 ? '' : 's'} of "${query.matchString}" on line${result.matchingLines.length === 1 ? '' : 's'} ${shownLines}${extraCount} — all shown as ${result.matchRanges.length} slice${result.matchRanges.length === 1 ? '' : 's'}, ±${contextLines} lines of context each; these lines are lineHint anchors for lspGetSemantics.`;
  let actualStartLine: number | undefined;
  let actualEndLine: number | undefined;
  let matchRanges: Array<{ start: number; end: number }> | undefined;

  if (result.matchRanges.length > 0) {
    const firstRange = result.matchRanges[0];
    const lastRange = result.matchRanges[result.matchRanges.length - 1];
    if (firstRange && lastRange) {
      actualStartLine = firstRange.start;
      actualEndLine = lastRange.end;
      if (result.matchRanges.length > 1) {
        matchRanges = result.matchRanges;
      }
    }
  }

  return {
    resultContent,
    isPartial: true,
    actualStartLine,
    actualEndLine,
    matchRanges,
    warnings: [matchSummary],
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
      isPartial: false,
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
      isPartial: false,
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
      .join('\n'),
    isPartial: true,
    actualStartLine: effectiveStartLine,
    actualEndLine: effectiveEndLine,
    warnings,
  };
}

export function buildExtractionState(
  query: FetchContentQuery,
  content: string,
  _defaultOutputCharLength: number
): ExtractionState {
  const lines = content.split('\n');
  const totalLines = countLines(content);

  if (query.matchString) {
    return buildMatchExtractionState(query, lines, totalLines);
  }

  if (hasLineRangeRequest(query)) {
    return buildLineRangeExtractionState(query, lines, totalLines);
  }

  return {
    resultContent: content,
    isPartial: false,
  };
}
