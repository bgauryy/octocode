/**
 * OQL predicate-tree Zod schemas.
 *
 * Split out of `../schema.ts` (kept as a barrel) so that file stays under the
 * repo's `max-lines: 400` ESLint rule. This group is self-contained: the
 * predicate tree (`PredicateSchema`) and its leaf kinds (text/regex/structural
 * /field) don't depend on anything else in the OQL schema module.
 */
import { z } from 'zod';

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
