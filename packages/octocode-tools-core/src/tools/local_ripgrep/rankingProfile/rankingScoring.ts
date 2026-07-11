/**
 * Ranking scoring engine — turns a candidate file's matches, path, and the
 * query context into an additive, explainable `FileScore`. Every function
 * here appends to a shared `reasons: string[]` accumulator; call order is
 * significant because scores are additive and reasons are appended in the
 * order they are computed. Do not reorder the calls inside
 * `scoreFileWithRarity` or `bestLineScore`.
 *
 * See the module doc in `../rankingProfile.ts` for the full contract.
 */
import type {
  LocalSearchCodeFile,
  LocalSearchCodeMatch,
} from '@octocodeai/octocode-core/types';

import {
  EXT_RE,
  HEADING_LINE,
  LOW_SIGNAL_PATH,
  RANK_WEIGHTS,
  classifyPathRole,
  fileExtension,
  selectProfile,
  type PathRole,
  type RankingProfile,
  type RankingProfileId,
} from './rankingProfiles.js';
import {
  escapeRegex,
  isPlainIdentifierKeyword,
  lexicalCategoryOf,
  matchedLineOf,
  matchesAny,
} from './rankingResults.js';

export interface RankContext {
  /** Validated, sanitized search base path (query.path). */
  queryPath?: string;
  /** The literal keyword/pattern, used for whole-word / token / path checks. */
  keyword?: string;
  langType?: string;
  caseSensitive?: boolean;
  wholeWord?: boolean;
  /** Profile override from query.rankingProfile. */
  profileOverride?: RankingProfileId | 'auto';
  /** True when the user explicitly searched into a low-signal/test/docs area. */
  explicitLowSignal?: boolean;
}

export interface FileScore {
  score: number;
  profile: RankingProfileId;
  pathRole: PathRole;
  reasons: string[];
}

type CandidateTermRarity = {
  candidateCount: number;
  fileTokens: ReadonlyMap<string, readonly string[]>;
  documentFrequency: ReadonlyMap<string, number>;
};

function tokenFromKeyword(keyword?: string): string | undefined {
  if (!keyword) return undefined;
  // Take the first identifier-ish token of a regex/keyword for path/word checks.
  const m = /[A-Za-z_$][\w$]*/.exec(keyword);
  return m ? m[0] : undefined;
}

function queryTokensFromKeyword(
  keyword: string | undefined,
  caseSensitive?: boolean
): string[] {
  if (!keyword) return [];
  const seen = new Set<string>();
  const tokens: string[] = [];
  for (const match of keyword.matchAll(/[A-Za-z_$][\w$]*/g)) {
    const raw = match[0];
    const key = caseSensitive ? raw : raw.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    tokens.push(raw);
  }
  return tokens;
}

function bestLineScore(
  matches: LocalSearchCodeMatch[] | undefined,
  profile: RankingProfile,
  ctx: RankContext,
  token: string | undefined,
  reasons: string[]
): number {
  if (!matches || matches.length === 0) return 0;
  // Exactness is only meaningful for a plain single-identifier search; for
  // regex/multi-term patterns the "token" is an arbitrary fragment (Fix #5).
  const exactnessEnabled = isPlainIdentifierKeyword(ctx.keyword);
  let best = 0;
  let bestReason = '';
  for (const m of matches) {
    const snippet = m.value ?? '';
    if (!snippet) continue;
    // Classify the line that actually contains the match, not the whole
    // context window — context lines must not pollute classification (Fix #4).
    const line = matchedLineOf(snippet, token, ctx.caseSensitive);
    let s = 0;
    const localReasons: string[] = [];

    // Exactness against the keyword token.
    if (token && exactnessEnabled) {
      const wordRe = new RegExp(
        `(^|[^\\w$])${escapeRegex(token)}([^\\w$]|$)`,
        ctx.caseSensitive ? '' : 'i'
      );
      if (wordRe.test(line)) {
        s += RANK_WEIGHTS.exactWholeWord;
        localReasons.push('whole-word match');
      } else if (
        !ctx.caseSensitive &&
        line.toLowerCase().includes(token.toLowerCase())
      ) {
        s += RANK_WEIGHTS.exactCaseInsensitive;
        localReasons.push('case-insensitive match');
      } else if (line.includes(token)) {
        s += RANK_WEIGHTS.substring;
        localReasons.push('substring match');
      }
    }

    // Language-line signal. Prefer the engine's AST classification (Phase 2)
    // when present — it is comment/string immune and grammar-accurate — and
    // fall back to the declarative regex profile (Phase 1) otherwise.
    const engineKind = (m as { kind?: string }).kind;
    if (engineKind) {
      s += scoreEngineKind(engineKind, localReasons);
    } else {
      // Lexical gate first, mirroring the AST's ordering: if the token sits in
      // a comment or string, it is weak evidence — score it as such and bail
      // before declaration/export scoring, so a commented-out or stringified
      // `export function foo()` cannot score like a live one (Fix #2/#3).
      const category = lexicalCategoryOf(
        line,
        token,
        profile.id,
        ctx.caseSensitive
      );
      if (category !== 'code') {
        s += RANK_WEIGHTS.commentOrStringPenalty;
        localReasons.push(
          category === 'comment'
            ? 'comment match (weak)'
            : 'string literal (weak)'
        );
      } else {
        if (matchesAny(profile.declaration, line)) {
          s += RANK_WEIGHTS.declarationLine;
          localReasons.push('declaration line');
        }
        if (matchesAny(profile.export, line)) {
          s += RANK_WEIGHTS.exportLine;
          localReasons.push('export/public line');
        }
        if (matchesAny(profile.import, line)) {
          s += RANK_WEIGHTS.importLine;
          localReasons.push('import line');
        }
        if (profile.id === 'markdown' && HEADING_LINE.test(line)) {
          s += RANK_WEIGHTS.headingLine;
          localReasons.push('markdown heading');
        }
        if (
          (profile.id === 'json' || profile.id === 'yaml') &&
          isConfigKeyLine(line, token)
        ) {
          s += RANK_WEIGHTS.configKeyLine;
          localReasons.push('config key match');
        }
      }
    }

    if (s > best) {
      best = s;
      bestReason = localReasons.join(', ');
    }
  }
  if (bestReason) reasons.push(bestReason);
  return best;
}

/** Map an engine AST kind label to a language-line contribution + reason. */
function scoreEngineKind(kind: string, reasons: string[]): number {
  switch (kind) {
    case 'declaration':
      reasons.push('AST: declaration');
      return RANK_WEIGHTS.declarationLine;
    case 'export':
      reasons.push('AST: export/public');
      return RANK_WEIGHTS.exportLine;
    case 'configKey':
      reasons.push('AST: config key');
      return RANK_WEIGHTS.configKeyLine;
    case 'heading':
      reasons.push('AST: heading');
      return RANK_WEIGHTS.headingLine;
    case 'import':
      reasons.push('AST: import');
      return RANK_WEIGHTS.importLine;
    case 'comment':
    case 'string':
      reasons.push(`AST: ${kind} (weak)`);
      return RANK_WEIGHTS.commentOrStringPenalty;
    case 'callsite':
      reasons.push('AST: callsite');
      return 1;
    default:
      return 0;
  }
}

function isConfigKeyLine(line: string, token?: string): boolean {
  // "key": ...  or  key: ...  with the token on the key side of the colon.
  const colon = line.indexOf(':');
  if (colon < 0) return false;
  const keySide = line.slice(0, colon);
  return token ? keySide.includes(token) : /["'\w]/.test(keySide);
}

function pathAffinityScore(
  path: string,
  ctx: RankContext,
  token: string | undefined,
  profile: RankingProfile,
  role: PathRole,
  reasons: string[]
): number {
  let s = 0;
  if (token) {
    const seg = path.toLowerCase();
    const t = token.toLowerCase();
    // filename / path segment contains the query token.
    if (seg.split('/').some(p => p.replace(EXT_RE, '').includes(t))) {
      s += RANK_WEIGHTS.pathSegmentToken;
      reasons.push('query token in path');
    }
  }
  const ext = fileExtension(path);
  if (profile.extensions.includes(ext) && ctx.langType) {
    s += RANK_WEIGHTS.extMatchesLangType;
    reasons.push('extension matches langType');
  }
  // Boost the source role itself (ecosystem-agnostic), not a hardcoded dir list.
  if (role === 'source') {
    s += RANK_WEIGHTS.sourceDir;
    reasons.push('source file');
  }
  return s;
}

function fileRoleScore(
  role: PathRole,
  ctx: RankContext,
  reasons: string[]
): number {
  if (ctx.explicitLowSignal) return 0;
  if (role === 'generated' || role === 'fixture') {
    reasons.push(`${role} file (penalized)`);
    return RANK_WEIGHTS.lowSignalPathPenalty;
  }
  return 0;
}

function saturatedMatchCount(matchCount: number, reasons: string[]): number {
  if (matchCount <= 0) return 0;
  const raw = Math.log2(matchCount + 1) * RANK_WEIGHTS.matchCountScale;
  const capped = Math.min(raw, RANK_WEIGHTS.matchCountCap);
  if (matchCount > 1) reasons.push(`match count saturated (${matchCount})`);
  return capped;
}

export function buildCandidateTermRarity(
  files: LocalSearchCodeFile[],
  ctx: RankContext
): CandidateTermRarity | undefined {
  const queryTokens = queryTokensFromKeyword(ctx.keyword, ctx.caseSensitive);
  if (queryTokens.length < 2 || files.length < 2) return undefined;

  const fileTokens = new Map<string, readonly string[]>();
  const documentFrequency = new Map<string, number>();

  for (const file of files) {
    const tokens = queryTokens.filter(token =>
      fileContainsToken(file, token, ctx.caseSensitive)
    );
    if (tokens.length === 0) continue;
    fileTokens.set(file.path, tokens);
    for (const token of tokens) {
      documentFrequency.set(token, (documentFrequency.get(token) ?? 0) + 1);
    }
  }

  if (fileTokens.size === 0) return undefined;
  return { candidateCount: files.length, fileTokens, documentFrequency };
}

function fileContainsToken(
  file: LocalSearchCodeFile,
  token: string,
  caseSensitive?: boolean
): boolean {
  const flags = caseSensitive ? '' : 'i';
  const re = new RegExp(`(^|[^\\w$])${escapeRegex(token)}([^\\w$]|$)`, flags);
  for (const match of file.matches ?? []) {
    if (re.test(match.value ?? '')) return true;
  }
  return re.test(file.path);
}

function candidateTermRarityScore(
  file: LocalSearchCodeFile,
  rarity: CandidateTermRarity | undefined,
  reasons: string[]
): number {
  if (!rarity) return 0;
  const tokens = rarity.fileTokens.get(file.path);
  if (!tokens?.length) return 0;

  let bestToken = '';
  let bestScore = 0;
  let bestDf = 0;
  for (const token of tokens) {
    const df = rarity.documentFrequency.get(token) ?? rarity.candidateCount;
    if (df > rarity.candidateCount / 2) continue;
    const idf = Math.log2((rarity.candidateCount + 1) / (df + 1));
    const score = Math.min(
      idf * RANK_WEIGHTS.rareQueryTokenScale,
      RANK_WEIGHTS.rareQueryTokenCap
    );
    if (score > bestScore) {
      bestScore = score;
      bestToken = token;
      bestDf = df;
    }
  }

  if (bestScore <= 0) return 0;
  reasons.push(
    `rare query token: ${bestToken} (${bestDf}/${rarity.candidateCount} files)`
  );
  return bestScore;
}

export function scoreFileWithRarity(
  file: LocalSearchCodeFile,
  ctx: RankContext,
  rarity?: CandidateTermRarity
): FileScore {
  const profile = selectProfile(file.path, ctx.langType, ctx.profileOverride);
  const role = classifyPathRole(file.path);
  const token = tokenFromKeyword(ctx.keyword);
  const reasons: string[] = [];

  let score = 0;
  score += bestLineScore(file.matches, profile, ctx, token, reasons);
  score += pathAffinityScore(file.path, ctx, token, profile, role, reasons);

  if (
    ctx.queryPath &&
    file.path.startsWith(ctx.queryPath.replace(/\/+$/, '') + '/')
  ) {
    score += RANK_WEIGHTS.pathUnderQuery;
    reasons.push('under requested subtree');
  }

  score += fileRoleScore(role, ctx, reasons);
  if (LOW_SIGNAL_PATH.test(file.path) && !ctx.explicitLowSignal) {
    score += RANK_WEIGHTS.lowSignalPathPenalty;
    reasons.push('low-signal path (penalized)');
  }
  score += saturatedMatchCount(file.matchCount ?? 0, reasons);
  score += candidateTermRarityScore(file, rarity, reasons);

  return {
    score: Math.round(score * 100) / 100,
    profile: profile.id,
    pathRole: role,
    reasons,
  };
}

/** Score one file. Pure and deterministic. */
export function scoreFile(
  file: LocalSearchCodeFile,
  ctx: RankContext
): FileScore {
  return scoreFileWithRarity(file, ctx);
}
