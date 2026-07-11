/**
 * Ranking profile data/config — language profile tables, path-role
 * classification, and the tunable weight policy. See
 * `.octocode/RANKING-ARCHITECTURE.md` and the module doc in
 * `../rankingProfile.ts` (the public barrel) for the full contract.
 *
 * This module owns *what* a profile is and *where* a path lives (its role);
 * `rankingScoring.ts` owns *how much* each signal is worth in a score.
 */

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
  'relevance' | 'matchCount' | 'path' | 'created' | 'modified' | 'accessed';

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

export type PathRole =
  'source' | 'test' | 'docs' | 'config' | 'generated' | 'fixture' | 'unknown';

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

export interface RankingProfile {
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
export const COMMENT_LINE = /^\s*(\/\/|#|\*|\/\*|<!--|--)/;

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

export const HEADING_LINE = /^\s*#{1,6}\s/;

export const LOW_SIGNAL_PATH =
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

export const EXT_RE = /\.([a-z0-9]+)$/i;

export function fileExtension(path: string): string {
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
