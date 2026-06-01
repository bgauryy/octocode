/**
 * Extract matching lines from content based on pattern
 * Supports both regex and literal string matching with context lines
 */
import { createSafeRegExp } from '../../utils/core/safeRegex.js';

/** Remove all whitespace so anchors survive minification/reflow differences. */
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

  // Compile regex once if needed — with ReDoS protection
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

  // Whitespace-tolerant fallback (literal anchors only): an anchor copied from
  // a minified search snippet has its whitespace stripped (e.g.
  // `foo(a,b,c)`), so an exact `includes` against the raw line `foo(a, b, c)`
  // misses. Retry once comparing both sides with all whitespace removed so the
  // anchor still resolves instead of returning a false "not found".
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

  // Group consecutive matches to avoid duplicating context
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
