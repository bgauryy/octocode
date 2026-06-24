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
  mode: z
    .enum(LOCAL_SEARCH_MODES)
    .optional()
    .default('paginated')
    .describe(
      '"paginated" snippets; "discovery" paths only; "detailed" snippets plus context; "structural" AST/code-shape search with pattern or rule. Structural matches return line/capture anchors that can feed lspGetSemantics when symbol identity matters.'
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
