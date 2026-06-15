import {
  contextUtils,
  type ExtractMatchingLinesOptions,
} from '../../utils/contextUtils.js';

export function extractMatchingLines(
  lines: string[],
  pattern: string,
  contextLines: number,
  isRegex: boolean = false,
  caseSensitive: boolean = false,
  maxMatches?: number
): {
  lines: string[];
  matchRanges: Array<{ start: number; end: number }>;
  matchCount: number;
  matchingLines: number[];
} {
  if (isRegex) {
    try {
      new RegExp(pattern);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      throw new Error(`Invalid regex pattern: ${message}`);
    }
  }

  if (maxMatches !== undefined && maxMatches <= 0) {
    return {
      lines: [],
      matchRanges: [],
      matchCount: 0,
      matchingLines: [],
    };
  }

  const content = lines.join('\n');

  const options: ExtractMatchingLinesOptions = {
    isRegex,
    caseSensitive,
    contextLines,
    maxMatches,
  };

  const result = contextUtils.extractMatchingLines(content, pattern, options);

  return {
    lines: result.lines,
    matchRanges: result.matchRanges.map(r => ({ start: r.start, end: r.end })),
    matchCount: result.matchCount,
    matchingLines: result.matchingLines.map(n => n),
  };
}
