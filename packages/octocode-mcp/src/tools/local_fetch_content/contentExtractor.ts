import { createSafeRegExp } from '../../utils/core/safeRegex.js';

function stripWhitespace(s: string): string {
  return s.replace(/\s+/g, '');
}

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
} {
  const matchingLineNumbers: number[] = [];

  let regex: RegExp | null = null;
  if (isRegex) {
    try {
      const flags = caseSensitive ? '' : 'i';
      regex = createSafeRegExp(pattern, flags);
    } catch (error) {
      throw new Error(
        `Invalid regex pattern: ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }

  const literalPattern = caseSensitive ? pattern : pattern.toLowerCase();

  lines.forEach((line, index) => {
    const matches =
      isRegex && regex
        ? regex.test(line)
        : caseSensitive
          ? line.includes(pattern)
          : line.toLowerCase().includes(literalPattern);

    if (matches) {
      matchingLineNumbers.push(index + 1);
    }
  });

  if (!isRegex && matchingLineNumbers.length === 0) {
    const needle = stripWhitespace(caseSensitive ? pattern : literalPattern);
    if (needle.length > 0) {
      lines.forEach((line, index) => {
        const haystack = stripWhitespace(
          caseSensitive ? line : line.toLowerCase()
        );
        if (haystack.includes(needle)) {
          matchingLineNumbers.push(index + 1);
        }
      });
    }
  }

  const totalMatchCount = matchingLineNumbers.length;

  if (totalMatchCount === 0) {
    return { lines: [], matchRanges: [], matchCount: 0 };
  }

  const matchesToProcess = maxMatches
    ? matchingLineNumbers.slice(0, maxMatches)
    : matchingLineNumbers;

  const ranges: Array<{ start: number; end: number }> = [];
  const firstMatchLine = matchesToProcess[0];
  if (firstMatchLine === undefined) {
    return { lines: [], matchRanges: [], matchCount: 0 };
  }

  let currentRange = {
    start: Math.max(1, firstMatchLine - contextLines),
    end: Math.min(lines.length, firstMatchLine + contextLines),
  };

  for (let i = 1; i < matchesToProcess.length; i++) {
    const matchLine = matchesToProcess[i];
    if (matchLine === undefined) continue;
    const rangeStart = Math.max(1, matchLine - contextLines);
    const rangeEnd = Math.min(lines.length, matchLine + contextLines);

    if (rangeStart <= currentRange.end + 1) {
      currentRange.end = Math.max(currentRange.end, rangeEnd);
    } else {
      ranges.push({ ...currentRange });
      currentRange = { start: rangeStart, end: rangeEnd };
    }
  }
  ranges.push(currentRange);

  const resultLines: string[] = [];
  ranges.forEach((range, idx) => {
    if (idx > 0) {
      const prevRange = ranges[idx - 1];
      if (prevRange) {
        const omittedLines = range.start - prevRange.end - 1;
        if (omittedLines > 0) {
          resultLines.push('');
          resultLines.push(`... [${omittedLines} lines omitted] ...`);
          resultLines.push('');
        }
      }
    }
    resultLines.push(...lines.slice(range.start - 1, range.end));
  });

  return {
    lines: resultLines,
    matchRanges: ranges,
    matchCount: totalMatchCount,
  };
}
