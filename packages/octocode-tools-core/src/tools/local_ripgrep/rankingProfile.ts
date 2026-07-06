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
 */
import type {
  LocalSearchCodeFile,
  LocalSearchCodeMatch,
} from '@octocodeai/octocode-core/types';

export type RankingProfileId =
  | 'typescript'
  | 'javascript'
  | 'rust'
  | 'python'
  | 'go'
  | 'java'
  | 'scala'
  | 'markdown'
  | 'json'
  | 'yaml'
  | 'generic';

/** TS-level result orderings. `relevance` is the language-aware scorer. */
export type RankSort =
  | 'relevance'
  | 'matchCount'
  | 'path'
  | 'created'
  | 'modified'
  | 'accessed';

export const RANKING_PROFILE_IDS = [
  'typescript',
  'javascript',
  'rust',
  'python',
  'go',
  'java',
  'scala',
  'markdown',
  'json',
  'yaml',
  'generic',
] as const satisfies readonly RankingProfileId[];

type PathRole =
  | 'source'
  | 'test'
  | 'docs'
  | 'config'
  | 'generated'
  | 'fixture'
  | 'unknown';

/**
 * All ranking weights in one place. Tuning happens here and in the golden
 * fixtures, never scattered across the scorer. Values are deliberately
 * integer-ish and additive so a score can be read back from its reasons.
 */
export const RANK_WEIGHTS = {
  // Exactness of the matched token on its line.
  exactWholeWord: 6,
  exactCaseInsensitive: 3,
  substring: 1,
  // Language-line signal (profile decides which lines matter).
  declarationLine: 8,
  exportLine: 4,
  importLine: 3,
  configKeyLine: 4,
  headingLine: 4,
  // A match that only appears inside a comment/string is weak evidence.
  commentOrStringPenalty: -2,
  // Path affinity.
  pathUnderQuery: 4,
  pathSegmentToken: 3,
  extMatchesLangType: 2,
  sourceDir: 1.5,
  // Penalize low-signal locations unless explicitly searched.
  lowSignalPathPenalty: -5,
  // Candidate-local IDF: rewards rarer query tokens within the bounded
  // candidate set, never by crawling or indexing the whole repo.
  rareQueryTokenScale: 2,
  rareQueryTokenCap: 3,
  // Match count must never dominate: saturate it.
  matchCountScale: 1.5,
  matchCountCap: 6,
} as const;

/**
 * Bound on how many files get full scoring before pagination. The engine text
 * path returns the entire result set unbounded (no global file/match cap), so
 * for broad terms in a large monorepo we must not score O(total files). When
 * the cap bites we deterministically prefilter (matchCount desc, path asc)
 * before scoring, and the builder surfaces a `truncated`-style note.
 */
export const RANK_CANDIDATE_CAP = 2000;

interface RankingProfile {
  id: RankingProfileId;
  extensions: readonly string[];
  /** Lines that declare/define a symbol named like the query — highest signal. */
  declaration: readonly RegExp[];
  /** Exported/public surface. */
  export: readonly RegExp[];
  /** Import/use lines. */
  import: readonly RegExp[];
}

// Comment/string heuristics are language-agnostic enough for Phase 1; AST
// classification (Phase 2) replaces these with real node kinds.
const COMMENT_LINE = /^\s*(\/\/|#|\*|\/\*|<!--|--)/;

const TS_LIKE: Pick<RankingProfile, 'declaration' | 'export' | 'import'> = {
  declaration: [
    /\b(function|class|interface|type|enum|const|let|var)\s+\w/,
    /\b\w+\s*[:=]\s*(async\s+)?(function|\()/,
  ],
  export: [/\bexport\b/, /\bmodule\.exports\b/, /\bexport\s+default\b/],
  import: [/\b(import|require)\b/, /\bfrom\s+['"]/],
};

const PROFILES: Record<RankingProfileId, RankingProfile> = {
  typescript: {
    id: 'typescript',
    extensions: ['ts', 'tsx', 'mts', 'cts'],
    ...TS_LIKE,
  },
  javascript: {
    id: 'javascript',
    extensions: ['js', 'jsx', 'mjs', 'cjs'],
    ...TS_LIKE,
  },
  rust: {
    id: 'rust',
    extensions: ['rs'],
    declaration: [
      /\b(fn|struct|enum|trait|impl|mod|type|const|static|macro_rules!)\b/,
    ],
    export: [
      /\bpub(\s*\([^)]*\))?\s+(fn|struct|enum|trait|mod|type|const|static)\b/,
    ],
    import: [/\buse\s+/, /\bextern\s+crate\b/],
  },
  python: {
    id: 'python',
    extensions: ['py', 'pyi'],
    declaration: [/^\s*(async\s+)?def\s+\w/, /^\s*class\s+\w/, /^\s*@\w/],
    export: [/^\s*__all__\s*=/],
    import: [/^\s*(import|from)\s+/],
  },
  go: {
    id: 'go',
    extensions: ['go'],
    declaration: [/\b(func|type|struct|interface)\b/, /^\s*(const|var)\s+/],
    // Go exports by capitalization; treat exported decls as declarations.
    export: [/\b(func|type)\s+[A-Z]\w*/],
    import: [/^\s*import\s+/, /^\s*"[^"]+"\s*$/],
  },
  java: {
    id: 'java',
    extensions: ['java'],
    declaration: [
      /\b(class|interface|enum|record)\s+\w/,
      /\b\w+\s*\([^)]*\)\s*\{/,
    ],
    export: [/\b(public|protected)\b/, /^\s*@\w/],
    import: [/^\s*import\s+/],
  },
  scala: {
    id: 'scala',
    extensions: ['scala', 'sc'],
    declaration: [/\b(class|object|trait|def|val|var|type|case\s+class)\s+\w/],
    export: [/\b(export|implicit)\b/, /^\s*@\w/],
    import: [/^\s*import\s+/],
  },
  markdown: {
    id: 'markdown',
    extensions: ['md', 'markdown', 'mdx'],
    // headings handled explicitly via HEADING_LINE; keep tables empty.
    declaration: [],
    export: [],
    import: [/^\s*\[[^\]]+\]\([^)]+\)/],
  },
  json: {
    id: 'json',
    extensions: ['json', 'jsonc', 'json5'],
    declaration: [],
    export: [],
    import: [],
  },
  yaml: {
    id: 'yaml',
    extensions: ['yaml', 'yml'],
    declaration: [],
    export: [],
    import: [],
  },
  generic: {
    id: 'generic',
    extensions: [],
    declaration: [/\b(function|def|fn|func|class|struct|interface|type)\b/],
    export: [/\b(export|public|pub)\b/],
    import: [/\b(import|require|use|include)\b/],
  },
};

const HEADING_LINE = /^\s*#{1,6}\s/;

const LOW_SIGNAL_PATH =
  /(^|\/)(dist|build|out|coverage|node_modules|vendor|\.next|__snapshots__)(\/|$)/;
const GENERATED_PATH = /(\.min\.|\.bundle\.|\.generated\.|\.d\.ts$|-lock\.)/;
const TEST_PATH = /(^|\/)(tests?|__tests__|spec|e2e)(\/|$)|\.(test|spec)\./;
const FIXTURE_PATH = /(^|\/)(fixtures?|__fixtures__|snapshots?)(\/|$)/;
const DOCS_PATH = /(^|\/)(docs?|examples?)(\/|$)|\.(md|markdown|mdx|rst|txt)$/i;
const CONFIG_PATH =
  /(^|\/)(config|\.github)(\/|$)|\.(json|ya?ml|toml|ini|cfg|conf|env)$|(^|\/)[^/]*\.config\.[jt]s$/i;
// Source-root conventions across ecosystems (not JS-only): JS/TS (src, lib,
// app, packages), Go (cmd, pkg, internal), Rust (crates), C/C++ (include),
// generic (source/sources). This is a boost hint, not the sole source signal —
// classifyPathRole also treats any recognized code file as source (see below),
// so a Go/Rust/Python file outside these dirs is not penalized.
const SOURCE_DIR =
  /(^|\/)(src|lib|app|packages|internal|pkg|cmd|crates|include|sources?)(\/|$)/;

/** Profiles that denote actual source code (vs markup/data/docs). */
const CODE_PROFILE_IDS: ReadonlySet<RankingProfileId> = new Set([
  'typescript',
  'javascript',
  'rust',
  'python',
  'go',
  'java',
  'scala',
]);

const EXT_RE = /\.([a-z0-9]+)$/i;

function fileExtension(path: string): string {
  const m = EXT_RE.exec(path);
  return m?.[1]?.toLowerCase() ?? '';
}

/** Whether the file's extension maps to a real code language (any ecosystem). */
function isCodeFile(path: string): boolean {
  const ext = fileExtension(path);
  if (!ext) return false;
  for (const id of CODE_PROFILE_IDS) {
    if (PROFILES[id].extensions.includes(ext)) return true;
  }
  return false;
}

/** Deterministic profile selection from query langType then file extension. */
export function selectProfile(
  path: string,
  langType?: string,
  override?: RankingProfileId | 'auto'
): RankingProfile {
  if (override && override !== 'auto') return PROFILES[override];
  const ext = fileExtension(path);
  const hint = (langType ?? '').toLowerCase();
  for (const id of RANKING_PROFILE_IDS) {
    const p = PROFILES[id];
    if (id === 'generic') continue;
    if (hint && (id === hint || p.extensions.includes(hint))) return p;
  }
  for (const id of RANKING_PROFILE_IDS) {
    const p = PROFILES[id];
    if (id === 'generic') continue;
    if (p.extensions.includes(ext)) return p;
  }
  return PROFILES.generic;
}

export function classifyPathRole(path: string): PathRole {
  if (GENERATED_PATH.test(path)) return 'generated';
  if (FIXTURE_PATH.test(path)) return 'fixture';
  if (TEST_PATH.test(path)) return 'test';
  // config before docs so foo.config.ts beats the generic docs check
  if (CONFIG_PATH.test(path)) return 'config';
  if (DOCS_PATH.test(path)) return 'docs';
  // Source by exclusion: a recognized code file (any ecosystem) that isn't
  // test/docs/config/generated/fixture/build-output is source — not just files
  // under src/. This removes the JS-centric "only src/ counts" bias while still
  // leaving dist/node_modules/etc. to the low-signal penalty.
  if (
    !LOW_SIGNAL_PATH.test(path) &&
    (SOURCE_DIR.test(path) || isCodeFile(path))
  ) {
    return 'source';
  }
  return 'unknown';
}

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

function buildCandidateTermRarity(
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

function scoreFileWithRarity(
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

function matchesAny(patterns: readonly RegExp[], line: string): boolean {
  for (const re of patterns) if (re.test(line)) return true;
  return false;
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** A search whose keyword is a single bare identifier — the only case where
 * whole-word/substring line-exactness is meaningful (regex/multi-term patterns
 * make the extracted token an arbitrary fragment). */
function isPlainIdentifierKeyword(keyword?: string): boolean {
  return !!keyword && /^[A-Za-z_$][\w$]*$/.test(keyword.trim());
}

/** Pick the line within a (possibly multi-line context) snippet that actually
 * contains the query token; fall back to the first non-empty line. */
function matchedLineOf(
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
function lexicalCategoryOf(
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
function isIndexInsideString(line: string, idx: number): boolean {
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

/** True when the query path explicitly targets a low-signal area, so the
 * scorer should not penalize generated/test/docs/fixture roles. Uses anchored
 * path-segment matching (not substring) so `latest/`, `contest/`, `manifest/`
 * do NOT count as low-signal. */
export function isLowSignalQueryPath(path: string | undefined): boolean {
  if (!path) return false;
  if (LOW_SIGNAL_PATH.test(path)) return true;
  const role = classifyPathRole(path);
  return (
    role === 'test' ||
    role === 'docs' ||
    role === 'fixture' ||
    role === 'generated'
  );
}
