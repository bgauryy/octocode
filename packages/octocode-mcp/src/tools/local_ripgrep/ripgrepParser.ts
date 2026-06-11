import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';

import type { RipgrepQuery } from './scheme.js';
import type { SearchStats } from '../../utils/core/types.js';
import { parseRipgrepJson } from '../../utils/parsers/ripgrep.js';

export function parseFilesOnlyOutput(stdout: string): LocalSearchCodeFile[] {
  const lines = stdout.trim().split('\n').filter(Boolean);
  return lines
    .filter(line => !isRipgrepStatsLine(line))
    .map(path => ({
      path,
      matchCount: 1,
      matches: [],
    }));
}

export function parseCountOutput(stdout: string): LocalSearchCodeFile[] {
  const lines = stdout.trim().split('\n').filter(Boolean);
  return lines
    .filter(line => !isRipgrepStatsLine(line))
    .map(line => {
      const lastColonIdx = line.lastIndexOf(':');
      if (lastColonIdx === -1) {
        return { path: line, matchCount: 1, matches: [] };
      }
      const path = line.slice(0, lastColonIdx);
      const count = parseInt(line.slice(lastColonIdx + 1), 10);
      return {
        path,
        matchCount: isNaN(count) ? 1 : count,
        matches: [],
      };
    });
}

function isRipgrepStatsLine(line: string): boolean {
  return (
    /^\d+\s/.test(line) &&
    /\b(?:files|bytes|seconds|matches|searched|printed|spent)\b/.test(line)
  );
}

export function parseRipgrepOutput(
  stdout: string,
  configuredQuery: RipgrepQuery
): {
  files: LocalSearchCodeFile[];
  stats: SearchStats;
} {
  const isCountOutput =
    configuredQuery.countLinesPerFile || configuredQuery.countMatchesPerFile;
  const isPlainTextOutput =
    configuredQuery.filesOnly || configuredQuery.filesWithoutMatch;

  if (isCountOutput) {
    const files = parseCountOutput(stdout);
    const totalMatches = files.reduce((sum, f) => sum + (f.matchCount ?? 0), 0);
    return {
      files,
      stats: { matchCount: totalMatches },
    };
  } else if (isPlainTextOutput) {
    return {
      files: parseFilesOnlyOutput(stdout),
      stats: {},
    };
  } else {
    return parseRipgrepJson(stdout, configuredQuery);
  }
}
