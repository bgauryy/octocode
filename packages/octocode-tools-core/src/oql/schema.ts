/**
 * OQL Zod schemas.
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

// Item-count caps mirror ContentSanitizer's array bound (100) so a scope can't
// carry an unbounded list; matches the bounded-input convention elsewhere.
const stringOrArray = z.union([z.string(), z.array(z.string()).max(100)]);

export const QueryScopeSchema = z
  .strictObject({
    path: stringOrArray.optional(),
    language: stringOrArray.optional(),
    include: z.array(z.string()).max(100).optional(),
    exclude: z.array(z.string()).max(100).optional(),
    excludeDir: z.array(z.string()).max(100).optional(),
    hidden: z.boolean().optional(),
    noIgnore: z.boolean().optional(),
    minDepth: z.number().int().min(0).max(64).optional(),
    maxDepth: z.number().int().min(0).max(64).optional(),
  })
  .optional();

/* ----------------------------- predicates ------------------------------- */

const caseEnum = z.enum(['smart', 'sensitive', 'insensitive']);

// Term length cap mirrors ContentSanitizer's 10K string bound.
const predicateValue = z.string().max(10_000);

const TextPredicateSchema = z.strictObject({
  id: z.string().optional(),
  kind: z.literal('text'),
  value: predicateValue,
  case: caseEnum.optional(),
  wholeWord: z.boolean().optional(),
});

const RegexPredicateSchema = z.strictObject({
  id: z.string().optional(),
  kind: z.literal('regex'),
  value: predicateValue,
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

export const StructuralRuleInputSchema = z.union([
  StructuralRuleSchema,
  z.string().min(1),
]);

const StructuralPredicateSchema = z.strictObject({
  id: z.string().optional(),
  kind: z.literal('structural'),
  lang: z.string().min(1),
  pattern: z.string().optional(),
  rule: StructuralRuleInputSchema.optional(),
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
    'accessed',
    'empty',
    'permissions',
    'executable',
    'readable',
    'writable',
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
    'before',
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
      // Bounds mirror the shared response clamps (scheme/fields.ts) so OQL
      // content paging can't request an out-of-range window.
      charOffset: z.number().int().min(0).max(100_000_000).optional(),
      charLength: z.number().int().min(1).max(50_000).optional(),
      fullContent: z.boolean().optional(),
    })
    .optional(),
  tree: z
    .strictObject({
      maxDepth: z.number().int().min(0).max(64).optional(),
      pattern: z.string().optional(),
      includeSizes: z.boolean().optional(),
      extensions: z.array(z.string()).optional(),
      filesOnly: z.boolean().optional(),
      directoriesOnly: z.boolean().optional(),
      sortBy: z.enum(['name', 'size', 'time', 'extension']).optional(),
      reverse: z.boolean().optional(),
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
      contextLines: z.number().int().min(0).max(100).optional(),
      invertMatch: z.boolean().optional(),
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
          // files-target sorts, lowered to localFindFiles sortBy
          'size',
          'name',
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
  schema: z.literal('oql'),
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
  schema: z.literal('oql'),
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

const ACTIVE_TARGET_ENUM = ACTIVE_TARGETS as unknown as [string, ...string[]];

/**
 * Raw input is intentionally permissive: it carries documented sugar fields
 * AND passes unknown keys through (`.catchall`) so the normalizer — not Zod —
 * decides whether an unknown key is sugar, `ambiguousSugar`, or `unknownField`.
 * The reserved-target enum is allowed for internal normalization so callers
 * using the OQL library get `unsupportedTarget`; the public tool schema below
 * advertises only executable active targets.
 */
const ALL_TARGETS = [...ACTIVE_TARGETS, ...RESERVED_TARGETS] as unknown as [
  string,
  ...string[],
];

const OqlInputMetaShape = {
  schema: z.literal('oql').optional(),
  id: z.string().optional(),
  mainResearchGoal: z.string().optional(),
  researchGoal: z.string().optional(),
  reasoning: z.string().optional(),
} as const;

const OqlInputQueryShape = {
  ...OqlInputMetaShape,
  // target is optional on raw input — the normalizer infers it from sugar
  // (e.g. pattern/text -> "code", fetch.content -> "content").
  target: z
    .enum(ALL_TARGETS)
    .optional()
    .describe(
      'REQUIRED unless inferable from sugar (text/regex/pattern/rule/boolean → code, fetch.content → content, fetch.tree → structure). One of the active targets — run `search --scheme` for the full list and recipes.'
    ),
  from: QuerySourceSchema.optional().describe(
    'Source. Defaults to local cwd when omitted; use {kind:"github",owner,repo} for remote or {kind:"materialized",localPath} after a fetch/clone.'
  ),
  where: PredicateSchema.optional().describe(
    'Canonical predicate tree (kind: text | regex | structural | field | all | any | not). Mutually exclusive with the flat shorthand fields (text/regex/pattern/and/or/...): use ONE shape, not both.'
  ),
  materialize: z
    .union([MaterializePolicySchema, z.enum(['never', 'auto', 'required'])])
    .optional(),
  fetch: FetchInstructionsSchema.optional(),
  select: z.array(z.string()).optional(),
  view: viewEnum.optional(),
  controls: QueryControlsSchema.optional(),
  limit: z.number().int().min(1).optional(),
  page: z.number().int().min(1).optional(),
  itemsPerPage: z.number().int().min(1).optional(),
  params: z.record(z.string(), z.unknown()).optional(),
  explain: z.boolean().optional(),
  // Sugar fields consumed by normalize.ts. Shorthand for `from`/`where` —
  // mutually exclusive with the canonical `where` predicate tree.
  repo: z.string().optional(),
  owner: z.string().optional(),
  ref: z.string().optional(),
  path: stringOrArray.optional(),
  text: z
    .string()
    .optional()
    .describe(
      'Shorthand text search (→ where.text, target code). Do not combine with a canonical `where`.'
    ),
  regex: z.string().optional(),
  pattern: z
    .string()
    .optional()
    .describe(
      'Shorthand AST/structural pattern (→ structural where, target code). A function pattern must match a COMPLETE node — include return type (e.g. `function $N($$$A): $R { $$$B }`) or use a `rule` for partial/relational matches.'
    ),
  rule: StructuralRuleInputSchema.optional(),
  lang: z.string().optional(),
  and: z.array(z.unknown()).optional(),
  or: z.array(z.unknown()).optional(),
  xor: z.array(z.unknown()).optional(),
  noneOf: z.array(z.unknown()).optional(),
  oneOf: z.array(z.unknown()).optional(),
  invert: z.unknown().optional(),
  filesOnly: z.boolean().optional(),
  filesWithoutMatch: z.boolean().optional(),
  verbose: z.boolean().optional(),
} as const;

const OqlExecutableInputQueryShape = {
  ...OqlInputQueryShape,
  target: z
    .enum(ACTIVE_TARGET_ENUM)
    .optional()
    .describe(
      'REQUIRED unless inferable from sugar (text/regex/pattern/rule/boolean → code, fetch.content → content, fetch.tree → structure). One of the active targets — run `search --scheme` for the full list and recipes.'
    ),
} as const;

export const OqlDisplayQuerySchema = z
  .object(OqlExecutableInputQueryShape)
  .catchall(z.unknown());

export const OqlInputQuerySchema = z
  .object(OqlInputQueryShape)
  .catchall(z.unknown());

export const OqlInputBatchSchema = z
  .object({
    ...OqlInputMetaShape,
    queries: z.array(z.unknown()).min(1),
  })
  .catchall(z.unknown());

export const OqlSearchInputSchema = z
  .object({
    ...OqlExecutableInputQueryShape,
    // Single-query fields are accepted alongside an optional batch envelope.
    // Normalization decides whether the object is a single query or a batch.
    queries: z.array(z.unknown()).min(1).max(5).optional(),
    combine: z.enum(['independent', 'merge']).optional(),
  })
  .catchall(z.unknown());
