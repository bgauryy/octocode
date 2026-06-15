import { contextUtils, type RipgrepParseOptions } from '../contextUtils.js';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { SearchStats } from '../core/types.js';

interface RipgrepParserQuery {
  contextLines?: number;
  matchContentLength?: number;
}

export function parseRipgrepJson(
  jsonOutput: string,
  query: RipgrepParserQuery
): {
  files: LocalSearchCodeFile[];
  stats: SearchStats;
} {
  const options: RipgrepParseOptions = {
    contextLines: query.contextLines ?? 0,
    maxSnippetChars: query.matchContentLength,
  };

  const result = contextUtils.parseRipgrepJson(jsonOutput, options);

  const files: LocalSearchCodeFile[] = result.files.map(f => ({
    path: f.path,
    matchCount: f.matchCount,
    matches: f.matches.map(m => ({
      line: m.line,
      column: m.column,
      value: m.value,
    })),
  }));

  const stats: SearchStats = {
    matchCount: result.stats.matchCount,
    matchedLines: result.stats.matchedLines,
    filesMatched: result.stats.filesMatched,
    filesSearched: result.stats.filesSearched,
    bytesSearched: result.stats.bytesSearched ?? undefined,
    searchTime: result.stats.searchTime,
  };

  return { files, stats };
}
