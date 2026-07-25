import { z } from 'zod';
import { RipgrepQuerySchema as CoreRipgrepQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  LOCAL_MAX_DEPTH,
  MAX_MATCH_CONTENT_LENGTH,
  MAX_PAGE_NUMBER,
} from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import type {
  LocalItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

const LOCAL_SEARCH_MODES = [
  'paginated',
  'discovery',
  'detailed',
  'structural',
] as const;

// sort ('relevance'|'matchCount'|'path'|'modified'|'accessed'|'created'),
// rankingProfile, and debugRanking are defined canonically in
// @octocodeai/octocode-core (src/resources/tools/localSearchCode.ts) and flow in
// through CoreRipgrepQuerySchema. Engine-incompatible sort values are translated
// to a deterministic filesystem walk in ripgrepExecutor; the relevance scorer
// runs in ripgrepResultBuilder. Keep tools-core overrides to tightening bounds
// only, not redefining ranking fields.
const REMOVED_CORE_FIELDS = ['semanticRanking'] as const;

const queryOverrides = {
  // This `mode` selects the SEARCH ALGORITHM (paginated/discovery/detailed/
  // structural). It's unrelated to the nested `patches.mode` on
  // ghHistoryResearch (diff detail level) — different concepts sharing this
  // field name across tools.
  mode: z
    .enum(LOCAL_SEARCH_MODES)
    .optional()
    .default('paginated')
    .describe(
      '"paginated" snippets; "discovery" paths only; "detailed" snippets plus context; "structural" AST/code-shape search with pattern or rule. Structural matches return line/capture anchors that can feed lspGetSemantics when symbol identity matters. (Unrelated to ghHistoryResearch\'s `patches.mode` — different concepts sharing this name.)'
    ),
  // A single text/regex pattern (unlike ghSearchCode/ghSearchRepos, where
  // `keywords` is an ARRAY of ANDed terms) — passing an array here fails
  // validation.
  keywords: z
    .string()
    .optional()
    .describe(
      'The search pattern (text or regex). Set fixedString:true for a literal match, or perlRegex:true for advanced regex features (lookaheads, backreferences). (Unlike ghSearchCode/ghSearchRepos, where `keywords` is an array of ANDed terms — this is a single string.)'
    ),
  // Filters SEARCH RESULTS down to matching file paths (drops line content).
  // Unrelated to localViewStructure's `filesOnly`, which instead filters a
  // directory LISTING down to file entries (excluding subdirectories).
  filesOnly: z
    .boolean()
    .optional()
    .describe(
      "Returns matching file paths without line content. Mutually exclusive with filesWithoutMatch. (Unlike localViewStructure's `filesOnly`, which filters a directory listing to file entries only — a different concept sharing this name.)"
    ),
  pattern: z
    .string()
    .optional()
    .describe(
      'Structural only: code-shaped AST pattern with $X (one node) or $$$ARGS (node list). Modifiers are part of the node — `function $NAME` does not match `async function` or `export function`; include the modifiers or use a YAML `kind` rule for modifier-agnostic matches. Use this to find syntax shape, then use lspGetSemantics for semantic proof.'
    ),
  // Engine walker supports maxDepth on the structural lane
  // (StructuralSearchFilesOptions.maxDepth); previously only reachable by
  // direct napi callers.
  maxDepth: clampedInt(0, LOCAL_MAX_DEPTH)
    .optional()
    .describe(
      'Structural mode only: keep files at most this many directory levels below the search root (0 = files directly in the root). Ignored by text/regex modes.'
    ),
  rule: z
    .string()
    .optional()
    .describe(
      'Structural only: YAML ast-grep rule for not/inside/has/all/any. Use for partial or relational AST queries before escalating matched anchors to lspGetSemantics.'
    ),
  contextLines: contextLinesField,
  matchContentLength: clampedInt(1, MAX_MATCH_CONTENT_LENGTH)
    .optional()
    .default(500),
  maxMatchesPerFile: clampedInt(1, MAX_MATCH_CONTENT_LENGTH).optional(),
  maxFiles: clampedInt(1, MAX_MATCH_CONTENT_LENGTH).optional(),
  matchPage: relaxedPageNumberField.optional(),
  itemsPerPage: clampedInt(1, MAX_PAGE_NUMBER).optional(),
  page: relaxedPageNumberField.default(1),
  unique: z
    .boolean()
    .optional()
    .describe('With onlyMatching, return each matched value once per file.'),
  countUnique: z
    .boolean()
    .optional()
    .describe(
      'With onlyMatching, return each matched value once per file with its frequency.'
    ),
} as const;

const bulkQueryOverrides = {
  ...queryOverrides,
  // Disabled feature: reject loudly instead of z.never()'s opaque type error —
  // the field is still visible in older clients/docs, so the rejection must
  // say WHY and what to do.
  semanticRanking: z
    .unknown()
    .optional()
    .superRefine((v, ctx) => {
      if (v === undefined) return;
      ctx.addIssue({
        code: 'custom',
        message:
          'semanticRanking is disabled in this build — sort:"relevance" already includes declaration/export/AST signals; remove this field.',
      });
    })
    .describe(
      'DISABLED in this build — do not pass. sort:"relevance" already includes declaration/export/AST signals.'
    ),
} as const;

const RipgrepQueryShape = createQueryShapeSchema(
  CoreRipgrepQuerySchema,
  bulkQueryOverrides
);

// Structural-mode validation (exactly one of pattern/rule, reject ripgrep-only
// fields, require keywords otherwise) is enforced by the core RipgrepQuerySchema
// superRefine, which describeQuerySchema preserves through to this schema.
const LocalRipgrepBaseQuerySchema = describeQuerySchema(
  CoreRipgrepQuerySchema,
  queryOverrides,
  { strict: true, omit: REMOVED_CORE_FIELDS }
);

export const LocalRipgrepQuerySchema = LocalRipgrepBaseQuerySchema.superRefine(
  (query, ctx) => {
    const ripgrepQuery = query as typeof query & {
      unique?: boolean;
      countUnique?: boolean;
    };
    if (ripgrepQuery.caseSensitive && ripgrepQuery.caseInsensitive) {
      ctx.addIssue({
        code: 'custom',
        message: 'caseSensitive and caseInsensitive are mutually exclusive.',
        path: ['caseSensitive'],
      });
    }
    if (ripgrepQuery.fixedString && ripgrepQuery.perlRegex) {
      ctx.addIssue({
        code: 'custom',
        message: 'fixedString and perlRegex are mutually exclusive.',
        path: ['fixedString'],
      });
    }
    if (ripgrepQuery.filesOnly && ripgrepQuery.filesWithoutMatch) {
      ctx.addIssue({
        code: 'custom',
        message: 'filesOnly and filesWithoutMatch are mutually exclusive.',
        path: ['filesOnly'],
      });
    }
    if (ripgrepQuery.countLinesPerFile && ripgrepQuery.countMatchesPerFile) {
      ctx.addIssue({
        code: 'custom',
        message:
          'countLinesPerFile and countMatchesPerFile are mutually exclusive.',
        path: ['countLinesPerFile'],
      });
    }
    if (ripgrepQuery.multilineDotall && !ripgrepQuery.multiline) {
      ctx.addIssue({
        code: 'custom',
        message: 'multilineDotall requires multiline=true.',
        path: ['multilineDotall'],
      });
    }
    if (ripgrepQuery.mode === 'structural') {
      for (const field of ['unique', 'countUnique'] as const) {
        if (ripgrepQuery[field]) {
          ctx.addIssue({
            code: 'custom',
            message: `\`${field}\` is not valid with mode:"structural".`,
            path: [field],
          });
        }
      }
      return;
    }

    if (ripgrepQuery.unique && !ripgrepQuery.onlyMatching) {
      ctx.addIssue({
        code: 'custom',
        message: 'unique requires onlyMatching:true.',
        path: ['unique'],
      });
    }
    if (ripgrepQuery.countUnique && !ripgrepQuery.onlyMatching) {
      ctx.addIssue({
        code: 'custom',
        message: 'countUnique requires onlyMatching:true.',
        path: ['countUnique'],
      });
    }
  }
);

export type RipgrepQuery = z.infer<typeof LocalRipgrepQuerySchema> & {
  unique?: boolean;
  countUnique?: boolean;
};

export const LocalRipgrepBulkQuerySchema = createRelaxedBulkQuerySchema(
  RipgrepQueryShape,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output TYPES — describes what localSearchCode returns per query result row.
// No zod: the MCP server registers no outputSchema, so the output is a plain
// type. Shared envelope lives in types/toolOutput.ts.
// ---------------------------------------------------------------------------

export interface LocalSearchMatch {
  line: number;
  endLine?: number;
  value?: string;
  column?: number;
  endColumn?: number;
  count?: number;
  /** AST node-kind label when classifyMatches ran (declaration|callsite|…). */
  kind?: string;
  /** Deterministic hint derived from kind (0.0..1.0); not a ranker score. */
  scoreHint?: number;
  metavars?: Record<string, string[]>;
  metavarRanges?: Record<
    string,
    Array<{
      text: string;
      line: number;
      column: number;
      endLine: number;
      endColumn: number;
    }>
  >;
}

export interface LocalSearchFile {
  path: string;
  absolutePath?: string;
  uri?: string;
  matches?: LocalSearchMatch[];
  totalOccurrences?: number;
  totalMatchedLines?: number;
  totalMatchRows?: number;
  returnedMatchRows?: number;
  ranking?: {
    score: number;
    profile?: string;
    pathRole?: string;
    reasons?: string[];
  };
  matchPagination?: LocalItemPagination;
  pagination?: LocalItemPagination;
  next?: Record<string, ToolContinuation>;
}

export interface LocalSearchCodeData {
  files?: LocalSearchFile[];
  summary?: string;
  searchEngine?: string;
  stats?: {
    totalOccurrences?: number;
    matchedLines?: number;
    filesMatched?: number;
    filesSearched?: number;
    bytesSearched?: number;
    searchTime?: string;
    [key: string]: unknown;
  };
  pagination?: LocalItemPagination;
  next?: Record<string, ToolContinuation>;
  warnings?: string[];
}

export type LocalSearchCodeOutput = BulkToolOutput<LocalSearchCodeData>;
