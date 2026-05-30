import type { z } from 'zod/v4';
import type { RipgrepQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';

type RipgrepQuery = z.infer<typeof RipgrepQuerySchema>;
import type { SearchStats } from '../../utils/core/types.js';
import { parseRipgrepJson } from '../../utils/parsers/ripgrep.js';

/**
 * Parse ripgrep plain text output (filesOnly or filesWithoutMatch mode).
 * When using -l (--files-with-matches) or --files-without-match flags,
 * ripgrep outputs one filename per line instead of JSON.
 *
 * @param stdout - Plain text output from ripgrep (one filename per line)
 * @returns Array of file matches with path only (no match details)
 */
export function parseFilesOnlyOutput(stdout: string): LocalSearchCodeFile[] {
  const lines = stdout.trim().split('\n').filter(Boolean);
  return lines
    .filter(line => !isRipgrepStatsLine(line))
    .map(path => ({
      path,
      matchCount: 1, // At least one match exists (that's why file is listed)
      matches: [], // No match details in plain text mode
    }));
}

/**
 * Parse ripgrep count output (-c or --count-matches mode).
 * These flags output one "path:count" per line instead of JSON.
 * -c counts lines with matches; --count-matches counts individual matches.
 *
 * @param stdout - Plain text output from ripgrep (path:count per line)
 * @returns Array of file matches with accurate per-file match counts
 */
export function parseCountOutput(stdout: string): LocalSearchCodeFile[] {
  const lines = stdout.trim().split('\n').filter(Boolean);
  return lines
    .filter(line => !isRipgrepStatsLine(line))
    .map(line => {
      // Format: path:count — count is always the last colon-separated segment
      const lastColonIdx = line.lastIndexOf(':');
      if (lastColonIdx === -1) {
        // No colon found — treat as path with 1 match (shouldn't happen with -c)
        return { path: line, matchCount: 1, matches: [] };
      }
      const path = line.slice(0, lastColonIdx);
      const count = parseInt(line.slice(lastColonIdx + 1), 10);
      return {
        path,
        matchCount: isNaN(count) ? 1 : count,
        matches: [], // No match details in count mode
      };
    });
}

/**
 * Detect ripgrep --stats summary lines that may appear in plain text output.
 * Stats lines follow patterns like "N files contained matches", "N files searched", etc.
 */
function isRipgrepStatsLine(line: string): boolean {
  return (
    /^\d+\s/.test(line) &&
    /\b(?:files|bytes|seconds|matches|searched|printed|spent)\b/.test(line)
  );
}

/**
 * Parse ripgrep output (JSON, count, or plain text)
 */
export function parseRipgrepOutput(
  stdout: string,
  configuredQuery: RipgrepQuery
): {
  files: LocalSearchCodeFile[];
  stats: SearchStats;
} {
  const isCountOutput = configuredQuery.count || configuredQuery.countMatches;
  const isPlainTextOutput =
    configuredQuery.filesOnly || configuredQuery.filesWithoutMatch;

  if (isCountOutput) {
    // Count output: path:count per line (from -c or --count-matches)
    const files = parseCountOutput(stdout);
    const totalMatches = files.reduce((sum, f) => sum + f.matchCount, 0);
    return {
      files,
      stats: { matchCount: totalMatches },
    };
  } else if (isPlainTextOutput) {
    // Plain text output: one filename per line (no JSON)
    return {
      files: parseFilesOnlyOutput(stdout),
      stats: {},
    };
  } else {
    // JSON output: structured match data with line numbers, columns, etc.
    return parseRipgrepJson(stdout, configuredQuery);
  }
}
