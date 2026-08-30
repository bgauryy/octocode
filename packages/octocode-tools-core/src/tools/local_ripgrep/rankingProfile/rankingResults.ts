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

import type { RankSort } from './rankingProfiles.js';
import { RANK_CANDIDATE_CAP } from './rankingProfiles.js';
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
  opts: { debug?: boolean; candidateCap?: number; sortReverse?: boolean } = {}
): RankResult {
  if (sort === 'matchCount') {
    const sorted = [...files].sort(compareByMatchCount);
    return {
      files: opts.sortReverse ? sorted.reverse() : sorted,
      cappedCandidates: 0,
    };
  }
  if (sort === 'path') {
    const sorted = [...files].sort((a, b) => a.path.localeCompare(b.path));
    return {
      files: opts.sortReverse ? sorted.reverse() : sorted,
      cappedCandidates: 0,
    };
  }
  if (sort === 'created' || sort === 'modified' || sort === 'accessed') {
    // Engine already applied the filesystem sort (including any reverse);
    // preserve it stably — reversing again here would double-reverse it.
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

  const relevanceOrdered = [...scored.map(x => x.file), ...tail];
  const result: RankResult = {
    // Relevance-scored files first, then the unscored matchCount-ordered tail.
    // The tail keeps every matched file reachable through pagination.
    files: opts.sortReverse ? relevanceOrdered.reverse() : relevanceOrdered,
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
