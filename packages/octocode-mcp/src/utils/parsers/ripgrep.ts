import type { z } from 'zod';
import type { RipgrepQuerySchema } from '@octocodeai/octocode-core/schemas';
import { RESOURCE_LIMITS } from '../core/constants.js';
import type {
  LocalSearchCodeFile,
  LocalSearchCodeMatch,
} from '@octocodeai/octocode-core/types';

type RipgrepQuery = z.infer<typeof RipgrepQuerySchema>;
import type { SearchStats } from '../core/types.js';
import { RipgrepJsonMessageSchema } from './schemas.js';

function stripOneTrailingLineBreak(text: string): string {
  return text.replace(/\r?\n$/, '');
}

export function parseRipgrepJson(
  jsonOutput: string,
  query: RipgrepQuery
): {
  files: LocalSearchCodeFile[];
  stats: SearchStats;
} {
  const lines = jsonOutput.trim().split('\n').filter(Boolean);
  type RawMatch = {
    lineText: string;
    lineNumber: number;
    absoluteOffset: number;
    column: number;
    matchLength: number;
  };

  const fileMap = new Map<
    string,
    {
      rawMatches: RawMatch[];
      contexts: Map<number, string>;
    }
  >();

  let stats: SearchStats = {};

  for (const line of lines) {
    if (!line.trim()) continue;

    if (!line.trim().startsWith('{')) {
      continue;
    }

    try {
      const parsed = JSON.parse(line);
      const validation = RipgrepJsonMessageSchema.safeParse(parsed);
      if (!validation.success) continue;
      const msg = validation.data;

      if (msg.type === 'match') {
        const path = msg.data.path.text;
        const lineText = stripOneTrailingLineBreak(msg.data.lines.text);
        const lineNumber = msg.data.line_number;
        const absoluteOffset = msg.data.absolute_offset;

        if (!fileMap.has(path)) {
          fileMap.set(path, { rawMatches: [], contexts: new Map() });
        }
        const firstSubmatch = msg.data.submatches[0];
        const column = firstSubmatch?.start ?? 0;
        const matchLength = firstSubmatch
          ? firstSubmatch.end - firstSubmatch.start
          : lineText.length;

        const fileEntry = fileMap.get(path);
        if (fileEntry) {
          fileEntry.rawMatches.push({
            lineText,
            lineNumber,
            absoluteOffset,
            column,
            matchLength,
          });
        }
      } else if (msg.type === 'context') {
        const path = msg.data.path.text;
        const lineNumber = msg.data.line_number;
        const lineText = stripOneTrailingLineBreak(msg.data.lines.text);

        if (!fileMap.has(path)) {
          fileMap.set(path, { rawMatches: [], contexts: new Map() });
        }
        const fileEntry = fileMap.get(path);
        if (fileEntry) {
          fileEntry.contexts.set(lineNumber, lineText);
        }
      } else if (msg.type === 'summary') {
        stats = {
          matchCount: msg.data.stats.matches,
          matchedLines: msg.data.stats.matched_lines,
          filesMatched: msg.data.stats.searches_with_match,
          filesSearched: msg.data.stats.searches,
          bytesSearched: msg.data.stats.bytes_searched,
          searchTime: msg.data.stats.elapsed.human,
        };
      }
    } catch {
      void 0;
    }
  }

  const before = query.beforeContext ?? query.contextLines ?? 0;
  const after = query.afterContext ?? query.contextLines ?? 0;
  const maxLength =
    query.matchContentLength || RESOURCE_LIMITS.DEFAULT_MATCH_CONTENT_LENGTH;

  const files: LocalSearchCodeFile[] = Array.from(fileMap.entries()).map(
    ([path, entry]) => {
      const matches: LocalSearchCodeMatch[] = entry.rawMatches.map(m => {
        const contextLines: string[] = [];
        for (let i = before; i > 0; i--) {
          const ctx = entry.contexts.get(m.lineNumber - i);
          if (ctx) contextLines.push(ctx);
        }
        contextLines.push(m.lineText);
        for (let i = 1; i <= after; i++) {
          const ctx = entry.contexts.get(m.lineNumber + i);
          if (ctx) contextLines.push(ctx);
        }

        let value = contextLines.join('\n').replace(/\n+$/, '');
        const charArray = [...value];
        if (charArray.length > maxLength) {
          value = charArray.slice(0, maxLength - 3).join('') + '...';
        }

        return {
          value,
          line: m.lineNumber,
          column: m.column,
        };
      });

      return {
        path,
        matchCount: matches.length,
        matches,
      };
    }
  );

  return { files, stats };
}
