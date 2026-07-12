import { z } from 'zod';
import { RipgrepQuerySchema as CoreRipgrepQuerySchema } from '@octocodeai/octocode-core/schemas';
import { MAX_MATCH_CONTENT_LENGTH, MAX_PAGE_NUMBER } from '../../config.js';
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
import { bulkOutputEnvelopeFields } from '../../scheme/responseEnvelope.js';
import {
  LocalItemPaginationSchema,
  ToolContinuationSchema,
} from '../../scheme/pagination.js';

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
      'Structural only: code-shaped AST pattern with $X (one node) or $$$ARGS (node list). Use this to find syntax shape, then use lspGetSemantics for semantic proof.'
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
  semanticRanking: z.never().optional(),
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
// Output schema — describes what localSearchCode returns per query result row.
// ---------------------------------------------------------------------------

const SearchMatchSchema = z.object({
  line: z.number(),
  endLine: z.number().optional(),
  value: z.string().optional(),
  column: z.number().optional(),
  endColumn: z.number().optional(),
  count: z.number().optional(),
  /** AST node-kind label when classifyMatches ran (declaration|callsite|…). */
  kind: z.string().optional(),
  /** Deterministic hint derived from kind (0.0..1.0); not a ranker score. */
  scoreHint: z.number().optional(),
  metavars: z.record(z.string(), z.array(z.string())).optional(),
  metavarRanges: z
    .record(
      z.string(),
      z.array(
        z.object({
          text: z.string(),
          line: z.number(),
          column: z.number(),
          endLine: z.number(),
          endColumn: z.number(),
        })
      )
    )
    .optional(),
});

const SearchFileSchema = z.object({
  path: z.string(),
  matches: z.array(SearchMatchSchema).optional(),
  totalOccurrences: z.number().optional(),
  totalMatchedLines: z.number().optional(),
  totalMatchRows: z.number().optional(),
  returnedMatchRows: z.number().optional(),
  ranking: z
    .object({
      score: z.number(),
      profile: z.string().optional(),
      pathRole: z.string().optional(),
      reasons: z.array(z.string()).optional(),
    })
    .optional(),
  matchPagination: LocalItemPaginationSchema.optional(),
  pagination: LocalItemPaginationSchema.optional(),
  next: z.record(z.string(), ToolContinuationSchema).optional(),
});

const LocalSearchCodeDataSchema = z.object({
  files: z.array(SearchFileSchema).optional(),
  summary: z.string().optional(),
  searchEngine: z.string().optional(),
  stats: z
    .object({
      totalOccurrences: z.number().optional(),
      matchedLines: z.number().optional(),
      filesMatched: z.number().optional(),
      filesSearched: z.number().optional(),
      bytesSearched: z.number().optional(),
      searchTime: z.string().optional(),
    })
    .passthrough()
    .optional(),
  pagination: LocalItemPaginationSchema.optional(),
  next: z.record(z.string(), ToolContinuationSchema).optional(),
  warnings: z.array(z.string()).optional(),
});

export const LocalSearchCodeOutputSchema = z
  .object({
    results: z.array(
      z.object({
        id: z.string(),
        status: z.enum(['empty', 'error']).optional(),
        data: LocalSearchCodeDataSchema,
      })
    ),
  })
  .extend(bulkOutputEnvelopeFields);

export type LocalSearchCodeOutput = z.infer<typeof LocalSearchCodeOutputSchema>;
