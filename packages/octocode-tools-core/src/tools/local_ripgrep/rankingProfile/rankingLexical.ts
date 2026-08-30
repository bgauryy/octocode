/**
 * Pure lexical helpers shared by ranking result orchestration and scoring.
 *
 * This leaf module must not import either rankingResults or rankingScoring:
 * keeping those dependencies one-way prevents ESM initialization cycles.
 */
import type { RankingProfileId } from './rankingProfiles.js';
import { COMMENT_LINE } from './rankingProfiles.js';

export function matchesAny(patterns: readonly RegExp[], line: string): boolean {
  for (const re of patterns) if (re.test(line)) return true;
  return false;
}

export function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** A search whose keyword is a single bare identifier — the only case where
 * whole-word/substring line-exactness is meaningful. */
export function isPlainIdentifierKeyword(keyword?: string): boolean {
  return !!keyword && /^[A-Za-z_$][\w$]*$/.test(keyword.trim());
}

/** Pick the line within a context snippet that actually contains the query. */
export function matchedLineOf(
  snippet: string,
  token: string | undefined,
  caseSensitive?: boolean
): string {
  const newline = snippet.indexOf('\n');
  if (newline < 0) return snippet;
  const lines = snippet.split('\n');
  if (token) {
    const needle = caseSensitive ? token : token.toLowerCase();
    for (const line of lines) {
      const haystack = caseSensitive ? line : line.toLowerCase();
      if (haystack.includes(needle)) return line;
    }
  }
  return lines.find(line => line.trim().length > 0) ?? lines[0] ?? '';
}

type LexicalCategory = 'comment' | 'string' | 'code';

/** Conservative fallback used when grammar-accurate AST kind is unavailable. */
export function lexicalCategoryOf(
  line: string,
  token: string | undefined,
  profileId: RankingProfileId,
  caseSensitive?: boolean
): LexicalCategory {
  if (profileId === 'markdown') {
    return /<!--/.test(line) ? 'comment' : 'code';
  }
  if (profileId === 'json') return 'code';
  if (COMMENT_LINE.test(line)) return 'comment';
  const index = tokenIndexOf(line, token, caseSensitive);
  if (index < 0) return 'code';
  const commentIndex = commentStartIndex(line);
  if (commentIndex >= 0 && index > commentIndex) return 'comment';
  if (isIndexInsideString(line, index)) return 'string';
  return 'code';
}

function tokenIndexOf(
  line: string,
  token: string | undefined,
  caseSensitive?: boolean
): number {
  if (!token) return -1;
  return caseSensitive
    ? line.indexOf(token)
    : line.toLowerCase().indexOf(token.toLowerCase());
}

function commentStartIndex(line: string): number {
  let minimum = -1;
  const take = (index: number) => {
    if (index >= 0 && (minimum < 0 || index < minimum)) minimum = index;
  };
  for (const marker of ['//', '/*', '<!--']) take(line.indexOf(marker));
  const hash = /(^|\s)#(?![!{])/.exec(line);
  if (hash) take(hash.index + (hash[1]?.length ?? 0));
  const dash = /(^|\s)--\s/.exec(line);
  if (dash) take(dash.index + (dash[1]?.length ?? 0));
  return minimum;
}

/** Whether byte index index falls inside a quoted region of the line. */
export function isIndexInsideString(line: string, index: number): boolean {
  let quote = '';
  for (let offset = 0; offset < index && offset < line.length; offset++) {
    const character = line[offset];
    if (quote) {
      if (character === quote && line[offset - 1] !== '\\') quote = '';
    } else if (character === '"' || character === "'" || character === '`') {
      quote = character;
    }
  }
  return quote !== '';
}
