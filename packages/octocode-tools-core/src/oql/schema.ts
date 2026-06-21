/**
 * OQL V1 Zod schemas.
 *
 * Two layers:
 *  - canonical (`OqlQuerySchema` / `OqlBatchSchema`): STRICT. Unknown fields
 *    fail with `unknownField`. This is what the planner/executor consume after
 *    normalization, and what `--explain` echoes back.
 *  - raw input (`OqlInputQuerySchema` / `OqlInputBatchSchema`): lenient. Accepts
 *    documented sugar and passes unknown keys through so the normalizer can
 *    decide (accept / `ambiguousSugar` / `unknownField`).
 */
import { z } from 'zod';
import { ACTIVE_TARGETS, RESERVED_TARGETS } from './types.js';

/* ------------------------------ source ---------------------------------- */

export const QuerySourceSchema: z.ZodType = z.lazy(() =>
  z.union([
    z.strictObject({ kind: z.literal('local'), path: z.string().min(1) }),
    z.strictObject({
      kind: z.literal('github'),
      repo: z.string().min(1).optional(),
      owner: z.string().min(1).optional(),
      ref: z.string().min(1).optional(),
    }),
    z.strictObject({
      kind: z.literal('materialized'),
      localPath: z.string().min(1),
      source: QuerySourceSchema.optional(),
    }),
    z.strictObject({ kind: z.literal('npm') }),
  ])
);

/* ------------------------------- scope ---------------------------------- */

const stringOrArray = z.union([z.string(), z.array(z.string())]);

export const QueryScopeSchema = z
  .strictObject({
    path: stringOrArray.optional(),
    language: stringOrArray.optional(),
    include: z.array(z.string()).optional(),
    exclude: z.array(z.string()).optional(),
    excludeDir: z.array(z.string()).optional(),
    hidden: z.boolean().optional(),
    noIgnore: z.boolean().optional(),
    maxDepth: z.number().int().min(0).max(64).optional(),
  })
  .optional();

/* ----------------------------- predicates ------------------------------- */

const caseEnum = z.enum(['smart', 'sensitive', 'insensitive']);

const TextPredicateSchema = z.strictObject({
  id: z.string().optional(),
  kind: z.literal('text'),
  value: z.string(),
  case: caseEnum.optional(),
  wholeWord: z.boolean().optional(),
});

const RegexPredicateSchema = z.strictObject({
  id: z.string().optional(),
  kind: z.literal('regex'),
  value: z.string(),
  dialect: z.enum(['rust', 'pcre2', 'provider']).optional(),
  case: caseEnum.optional(),
  wholeWord: z.boolean().optional(),
  multiline: z.boolean().optional(),
  dotAll: z.boolean().optional(),
});

export const StructuralRuleSchema: z.ZodType = z.lazy(() =>
  z.strictObject({
    pattern: z.string().optional(),
    kind: z.string().optional(),
    inside: StructuralRuleSchema.optional(),
    has: StructuralRuleSchema.optional(),
    not: StructuralRuleSchema.optional(),
    all: z.array(StructuralRuleSchema).optional(),
    any: z.array(StructuralRuleSchema).optional(),
    stopBy: z.literal('end').optional(),
  })
);

const StructuralPredicateSchema = z.strictObject({
  id: z.string().optional(),
  kind: z.literal('structural'),
  lang: z.string().min(1),
  pattern: z.string().optional(),
  rule: StructuralRuleSchema.optional(),
});

const FieldPredicateSchema = z.strictObject({
  id: z.string().optional(),
  kind: z.literal('field'),
  field: z.enum([
    'path',
    'basename',
    'extension',
    'size',
    'modified',
    'entryType',
  ]),
  op: z.enum([
    '=',
    '!=',
    'in',
    'exists',
    'glob',
    'regex',
    '>',
    '>=',
    '<',
    '<=',
    'within',
  ]),
  value: z.unknown().optional(),
});

export const PredicateSchema: z.ZodType = z.lazy(() =>
  z.discriminatedUnion('kind', [
    z.strictObject({
      kind: z.literal('all'),
      id: z.string().optional(),
      of: z.array(PredicateSchema).min(1),
    }),
    z.strictObject({
      kind: z.literal('any'),
      id: z.string().optional(),
      of: z.array(PredicateSchema).min(1),
    }),
    z.strictObject({
      kind: z.literal('not'),
      id: z.string().optional(),
      predicate: PredicateSchema,
    }),
    TextPredicateSchema,
    RegexPredicateSchema,
    StructuralPredicateSchema,
    FieldPredicateSchema,
  ])
);

/* --------------------------- materialize -------------------------------- */

export const MaterializePolicySchema = z.strictObject({
  mode: z.enum(['never', 'auto', 'required']),
  strategy: z.enum(['file', 'tree', 'subtree', 'repo']).optional(),
  allowFullRepo: z.boolean().optional(),
  forceRefresh: z.boolean().optional(),
});

/* ------------------------------- fetch ---------------------------------- */

export const FetchInstructionsSchema = z.strictObject({
  content: z
    .strictObject({
      range: z
        .strictObject({
          startLine: z.number().int().min(1).optional(),
          endLine: z.number().int().min(1).optional(),
          contextLines: z.number().int().min(0).max(100).optional(),
        })
        .optional(),
      match: z
        .strictObject({
          text: z.string(),
          regex: z.boolean().optional(),
          caseSensitive: z.boolean().optional(),
        })
        .optional(),
      contentView: z.enum(['exact', 'compact', 'symbols']).optional(),
      charOffset: z.number().int().min(0).optional(),
      charLength: z.number().int().min(1).optional(),
      fullContent: z.boolean().optional(),
    })
    .optional(),
  tree: z
    .strictObject({
      maxDepth: z.number().int().min(0).max(64).optional(),
      includeSizes: z.boolean().optional(),
    })
    .optional(),
});

/* ------------------------------ controls -------------------------------- */

export const QueryControlsSchema = z.strictObject({
  search: z
    .strictObject({
      countLinesPerFile: z.boolean().optional(),
      countMatchesPerFile: z.boolean().optional(),
      onlyMatching: z.boolean().optional(),
      unique: z.boolean().optional(),
      countUnique: z.boolean().optional(),
      matchWindow: z.number().int().min(0).optional(),
      matchContentLength: z.number().int().min(1).optional(),
      maxMatchesPerFile: z.number().int().min(1).optional(),
      matchPage: z.number().int().min(1).optional(),
      sort: z
        .enum([
          'relevance',
          'matchCount',
          'path',
          'modified',
          'accessed',
          'created',
        ])
        .optional(),
      sortReverse: z.boolean().optional(),
      rankingProfile: z.string().optional(),
      debugRanking: z.boolean().optional(),
    })
    .optional(),
  budget: z
    .strictObject({
      maxFiles: z.number().int().min(1).optional(),
      maxCandidates: z.number().int().min(1).optional(),
      maxBytes: z.number().int().min(1).optional(),
      maxMaterializedBytes: z.number().int().min(1).optional(),
      maxPlanNodes: z.number().int().min(1).optional(),
      maxBooleanExpansion: z.number().int().min(1).optional(),
      timeoutMs: z.number().int().min(1).optional(),
    })
    .optional(),
});

/* ---------------------------- canonical query --------------------------- */

const viewEnum = z.enum(['discovery', 'paginated', 'detailed']);

export const OqlQuerySchema = z.strictObject({
  schema: z.literal('oql/v1'),
  id: z.string().optional(),
  target: z.enum(ACTIVE_TARGETS as unknown as [string, ...string[]]),
  from: QuerySourceSchema.optional(),
  scope: QueryScopeSchema,
  where: PredicateSchema.optional(),
  materialize: MaterializePolicySchema.optional(),
  fetch: FetchInstructionsSchema.optional(),
  select: z.array(z.string()).optional(),
  view: viewEnum.optional(),
  controls: QueryControlsSchema.optional(),
  limit: z.number().int().min(1).optional(),
  page: z.number().int().min(1).optional(),
  itemsPerPage: z.number().int().min(1).optional(),
  params: z.record(z.string(), z.unknown()).optional(),
  explain: z.boolean().optional(),
});

export const OqlBatchSchema = z.strictObject({
  schema: z.literal('oql/v1'),
  id: z.string().optional(),
  queries: z.array(OqlQuerySchema).min(1).max(5),
  combine: z.enum(['independent', 'merge']).optional(),
  limit: z.number().int().min(1).optional(),
  page: z.number().int().min(1).optional(),
  itemsPerPage: z.number().int().min(1).optional(),
  explain: z.boolean().optional(),
});

export const OqlCanonicalInputSchema = z.union([
  OqlQuerySchema,
  OqlBatchSchema,
]);

/**
 * Raw input is intentionally permissive: it carries documented sugar fields
 * AND passes unknown keys through (`.catchall`) so the normalizer — not Zod —
 * decides whether an unknown key is sugar, `ambiguousSugar`, or `unknownField`.
 * The reserved-target enum is allowed here so the normalizer can emit
 * `unsupportedTarget` rather than a generic schema error.
 */
const ALL_TARGETS = [...ACTIVE_TARGETS, ...RESERVED_TARGETS] as unknown as [
  string,
  ...string[],
];

export const OqlInputQuerySchema = z
  .object({
    schema: z.literal('oql/v1').optional(),
    // target is optional on raw input — the normalizer infers it from sugar
    // (e.g. pattern/text -> "code", fetch.content -> "content").
    target: z.enum(ALL_TARGETS).optional(),
  })
  .catchall(z.unknown());

export const OqlInputBatchSchema = z
  .object({
    schema: z.literal('oql/v1').optional(),
    queries: z.array(z.unknown()).min(1),
  })
  .catchall(z.unknown());
