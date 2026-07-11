/**
 * Ranking results — the top-level `rankFiles` entry point plus the small
 * lexical helpers (comment/string detection, token indexing) that classify a
 * matched line before the scorer in `rankingScoring.ts` weighs it.
 *
 * `rankFiles` composes the scoring engine (`scoreFileWithRarity`,
 * `buildCandidateTermRarity`) over the whole candidate set: it applies the
 * candidate cap, builds candidate-local term rarity once, scores every file,
 * and produces a total, deterministic order. See the module doc in
 * `../rankingProfile.ts` for the full contract.
 */
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';

import type { RankingProfileId, RankSort } from './rankingProfiles.js';
import { COMMENT_LINE, RANK_CANDIDATE_CAP } from './rankingProfiles.js';
import type { FileScore, RankContext } from './rankingScoring.js';
import {
  buildCandidateTermRarity,
  scoreFileWithRarity,
} from './rankingScoring.js';

export interface RankResult {
  files: LocalSearchCodeFile[];
  /** Per-path debug info, only populated when debug is requested. */
  debug?: Map<string, FileScore>;
  /** Number of files dropped by the candidate cap before scoring (0 if none). */
  cappedCandidates: number;
}

/**
 * Rank files by the requested sort mode. `relevance` is the language-aware
 * scorer (default); `matchCount` and `path` are deterministic escape hatches;
 * filesystem sorts (created/modified/accessed) preserve the engine's order.
 */
export function rankFiles(
  files: LocalSearchCodeFile[],
  sort: RankSort,
  ctx: RankContext,
  opts: { debug?: boolean; candidateCap?: number } = {}
): RankResult {
  if (sort === 'matchCount') {
    return {
      files: [...files].sort(compareByMatchCount),
      cappedCandidates: 0,
    };
  }
  if (sort === 'path') {
    return {
      files: [...files].sort((a, b) => a.path.localeCompare(b.path)),
      cappedCandidates: 0,
    };
  }
  if (sort === 'created' || sort === 'modified' || sort === 'accessed') {
    // Engine already applied the filesystem sort; preserve it stably.
    return { files: [...files], cappedCandidates: 0 };
  }

  // relevance
  const cap = opts.candidateCap ?? RANK_CANDIDATE_CAP;
  let candidates = files;
  let tail: LocalSearchCodeFile[] = [];
  let cappedCandidates = 0;
  if (files.length > cap) {
    // Deterministic prefilter before expensive scoring: relevance-score only
    // the highest match-count files, but KEEP the remainder as a matchCount-
    // ordered tail. Ranking reorders the top of the set; it must never drop
    // files, or pagination (which derives totalFiles from this list) could
    // never reach them. See the "ranking enriches, never gates" invariant.
    const ordered = [...files].sort(compareByMatchCount);
    candidates = ordered.slice(0, cap);
    tail = ordered.slice(cap);
    cappedCandidates = tail.length;
  }
  const rarity = buildCandidateTermRarity(candidates, ctx);

  // Per-file guard: a pathological file must never drop the whole result set.
  // On any scoring error the file is kept with a neutral score (sorts to the
  // bottom but is still returned) — ranking enriches, it never gates results.
  const scored = candidates.map(file => {
    try {
      return { file, s: scoreFileWithRarity(file, ctx, rarity) };
    } catch {
      return { file, s: neutralScore() };
    }
  });
  scored.sort((a, b) => {
    if (b.s.score !== a.s.score) return b.s.score - a.s.score;
    const mc = (b.file.matchCount ?? 0) - (a.file.matchCount ?? 0);
    if (mc !== 0) return mc;
    return a.file.path.localeCompare(b.file.path);
  });

  const result: RankResult = {
    // Relevance-scored files first, then the unscored matchCount-ordered tail.
    // The tail keeps every matched file reachable through pagination.
    files: [...scored.map(x => x.file), ...tail],
    cappedCandidates,
  };
  if (opts.debug) {
    result.debug = new Map(scored.map(x => [x.file.path, x.s]));
  }
  return result;
}

/** Fallback score for a file that could not be scored — keeps it in results. */
function neutralScore(): FileScore {
  return {
    score: 0,
    profile: 'generic',
    pathRole: 'unknown',
    reasons: ['ranking unavailable for this file'],
  };
}

function compareByMatchCount(
  a: LocalSearchCodeFile,
  b: LocalSearchCodeFile
): number {
  const delta = (b.matchCount ?? 0) - (a.matchCount ?? 0);
  if (delta !== 0) return delta;
  return a.path.localeCompare(b.path);
}

export function matchesAny(patterns: readonly RegExp[], line: string): boolean {
  for (const re of patterns) if (re.test(line)) return true;
  return false;
}

export function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** A search whose keyword is a single bare identifier — the only case where
 * whole-word/substring line-exactness is meaningful (regex/multi-term patterns
 * make the extracted token an arbitrary fragment). */
export function isPlainIdentifierKeyword(keyword?: string): boolean {
  return !!keyword && /^[A-Za-z_$][\w$]*$/.test(keyword.trim());
}

/** Pick the line within a (possibly multi-line context) snippet that actually
 * contains the query token; fall back to the first non-empty line. */
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
    for (const ln of lines) {
      const hay = caseSensitive ? ln : ln.toLowerCase();
      if (hay.includes(needle)) return ln;
    }
  }
  return lines.find(l => l.trim().length > 0) ?? lines[0] ?? '';
}

type LexicalCategory = 'comment' | 'string' | 'code';

/** Best-effort lexical category of the token's occurrence on a single line.
 * Deterministic and conservative — used only in the regex fallback path when
 * the engine's grammar-accurate AST kind is unavailable. */
export function lexicalCategoryOf(
  line: string,
  token: string | undefined,
  profileId: RankingProfileId,
  caseSensitive?: boolean
): LexicalCategory {
  // Markdown: `#`/`*`/`//` are headings/bullets/text, not comments — only the
  // HTML comment counts. Prose is treated as code so heading scoring can run.
  if (profileId === 'markdown') {
    return /<!--/.test(line) ? 'comment' : 'code';
  }
  // JSON has no comments, and its keys are quoted strings — string-gating would
  // wrongly suppress config-key detection. Only signal is the key-side check.
  if (profileId === 'json') return 'code';
  if (COMMENT_LINE.test(line)) return 'comment';
  const idx = tokenIndexOf(line, token, caseSensitive);
  if (idx < 0) return 'code'; // token not literally on this line (e.g. regex)
  const commentIdx = commentStartIndex(line);
  if (commentIdx >= 0 && idx > commentIdx) return 'comment';
  if (isIndexInsideString(line, idx)) return 'string';
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

/** Index of the first line comment marker, or -1. `#` and `--` require a
 * boundary to avoid matching `a--`, CSS hex colors, or template `#{}`. */
function commentStartIndex(line: string): number {
  let min = -1;
  const take = (i: number) => {
    if (i >= 0 && (min < 0 || i < min)) min = i;
  };
  for (const marker of ['//', '/*', '<!--']) take(line.indexOf(marker));
  const hash = /(^|\s)#(?![!{])/.exec(line);
  if (hash) take(hash.index + (hash[1]?.length ?? 0));
  const dash = /(^|\s)--\s/.exec(line);
  if (dash) take(dash.index + (dash[1]?.length ?? 0));
  return min;
}

/** Whether byte index `idx` falls inside a quoted region of the line. */
export function isIndexInsideString(line: string, idx: number): boolean {
  let quote = '';
  for (let i = 0; i < idx && i < line.length; i++) {
    const c = line[i];
    if (quote) {
      if (c === quote && line[i - 1] !== '\\') quote = '';
    } else if (c === '"' || c === "'" || c === '`') {
      quote = c;
    }
  }
  return quote !== '';
}
