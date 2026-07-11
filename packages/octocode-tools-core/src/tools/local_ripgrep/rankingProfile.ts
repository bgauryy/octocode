/**
 * Tier 1 — within-source language-aware ranking for localSearchCode.
 *
 * Replaces "more matches first, then path" with a stable, inspectable relevance
 * score. See `.octocode/RANKING-ARCHITECTURE.md`.
 *
 * Hard rules (Tier 1 determinism):
 *  - Same inputs -> same ranking. No LLM, no clock, no randomness, no
 *    filesystem-order dependency.
 *  - Every score is explainable from explicit features (`debugRanking`).
 *  - Sort is total: score desc -> raw matchCount desc -> path asc.
 *  - Profile selection is deterministic from query + file path; unknown
 *    languages fall back to the generic profile.
 *
 * Not rigid: profiles are declarative tables, all weights live in one policy
 * object, new languages register by adding a profile. AST/LSP signals (Phase
 * 2/3) layer on top without changing this contract.
 *
 * This file is a thin barrel: the implementation lives in `rankingProfile/`,
 * split by concern —
 *  - `rankingProfile/rankingProfiles.ts`: profile data/config (language
 *    tables, path-role regexes, `selectProfile`/`classifyPathRole`).
 *  - `rankingProfile/rankingScoring.ts`: the scoring engine (`bestLineScore`,
 *    `pathAffinityScore`, `candidateTermRarityScore`, `scoreFileWithRarity`).
 *  - `rankingProfile/rankingResults.ts`: `rankFiles`/`RankResult` plus the
 *    lexical helpers (`lexicalCategoryOf`, `isIndexInsideString`, etc.).
 * Re-exported here so no other module needs to change its import path.
 */
export type {
  RankingProfileId,
  RankSort,
  PathRole,
  RankingProfile,
} from './rankingProfile/rankingProfiles.js';
export {
  RANKING_PROFILE_IDS,
  RANK_WEIGHTS,
  RANK_CANDIDATE_CAP,
  COMMENT_LINE,
  HEADING_LINE,
  LOW_SIGNAL_PATH,
  EXT_RE,
  fileExtension,
  selectProfile,
  classifyPathRole,
  isLowSignalQueryPath,
} from './rankingProfile/rankingProfiles.js';

export type {
  RankContext,
  FileScore,
} from './rankingProfile/rankingScoring.js';
export {
  buildCandidateTermRarity,
  scoreFileWithRarity,
  scoreFile,
} from './rankingProfile/rankingScoring.js';

export type { RankResult } from './rankingProfile/rankingResults.js';
export {
  rankFiles,
  matchesAny,
  escapeRegex,
  isPlainIdentifierKeyword,
  matchedLineOf,
  lexicalCategoryOf,
  isIndexInsideString,
} from './rankingProfile/rankingResults.js';
