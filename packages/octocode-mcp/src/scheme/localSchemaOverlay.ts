import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';

export function withCoreSchemaDescriptions<
  T extends z.ZodObject<z.ZodRawShape>,
>(toolName: string, schema: T): T {
  const tool = completeMetadata.tools[toolName];
  const descriptions = {
    ...completeMetadata.baseSchema,
    ...(tool?.schema ?? {}),
  } as Record<string, unknown>;
  const describedShape = Object.fromEntries(
    Object.entries(schema.shape).map(([fieldName, fieldSchema]) => {
      const fs = fieldSchema as z.ZodTypeAny;
      const alreadyDescribed =
        typeof (fs as { description?: string }).description === 'string';
      if (alreadyDescribed) return [fieldName, fs];
      const description = descriptions[fieldName];
      return [
        fieldName,
        typeof description === 'string' ? fs.describe(description) : fs,
      ];
    })
  ) as z.ZodRawShape;
  return schema.extend(describedShape) as unknown as T;
}

export function clampedInt(min: number, max: number) {
  return z.preprocess(
    v =>
      typeof v === 'number' && Number.isFinite(v)
        ? Math.min(Math.max(v, min), max)
        : v,
    z.number().int().min(min).max(max)
  );
}

export const LOCAL_OVERLAY_MAX_LIMIT = 10_000;
export const LOCAL_OVERLAY_MAX_DEPTH = 20;

const LOCAL_OVERLAY_MAX_LINE = 1_000_000_000;
const LOCAL_OVERLAY_MAX_ORDER_HINT = 100_000;
const LOCAL_OVERLAY_MAX_PAGINATION_LIMIT = 1_000;
const LOCAL_OVERLAY_MAX_CONTEXT_LINES = 100;

export const lineNumberField = clampedInt(1, LOCAL_OVERLAY_MAX_LINE).optional();
export const requiredLineHintField = clampedInt(1, LOCAL_OVERLAY_MAX_LINE);
export const orderHintField = clampedInt(
  0,
  LOCAL_OVERLAY_MAX_ORDER_HINT
).optional();

export const contextLinesField = clampedInt(
  0,
  LOCAL_OVERLAY_MAX_CONTEXT_LINES
).optional();

export const relaxedPageNumberField = clampedInt(
  1,
  LOCAL_OVERLAY_MAX_PAGINATION_LIMIT
)
  .optional()
  .default(1);

export const DEFAULT_PAGE_SIZE = 20;
export const STRUCTURE_PAGE_SIZE = 100;

const responsePaginationFields = {
  responseCharOffset: clampedInt(0, 100_000_000)
    .optional()
    .describe(
      'Top-level response character offset. Use when the entire formatted tool response is larger than the responseCharLength window.'
    ),
  responseCharLength: clampedInt(1, 50_000)
    .optional()
    .describe(
      'Top-level response character page size. Works for every tool as a final formatted-response window; content-specific tools also expose per-query charOffset/charLength.'
    ),
} as const;

export const depthField = clampedInt(0, LOCAL_OVERLAY_MAX_DEPTH).optional();

/** View level for content-returning tools. */
export type MinifyMode = 'none' | 'standard' | 'symbols';

// Legacy boolean inputs (pre-enum `minify`) are still accepted but
// undocumented: true → "standard", false → "none".
const legacyMinifyBoolean = (value: unknown): unknown =>
  value === true ? 'standard' : value === false ? 'none' : value;

/** minify enum for the fetch-content tools: none (raw) | standard | symbols. */
export const minifyFieldWithSymbols = z.preprocess(
  legacyMinifyBoolean,
  z.enum(['none', 'standard', 'symbols']).optional()
);

/** minify enum for PR patches: none (raw diffs) | standard. */
export const minifyFieldStandard = z.preprocess(
  legacyMinifyBoolean,
  z.enum(['none', 'standard']).optional()
);

export type WithQueryMeta<T> = T & {
  id?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
};

export type WithLocalOverlay<T> = WithQueryMeta<T>;

export function createRelaxedBulkQuerySchema(
  toolName: string,
  querySchema: z.ZodTypeAny,
  options: { maxQueries?: number } = {}
) {
  const { maxQueries = 5 } = options;
  return z
    .object({
      queries: z
        .array(querySchema)
        .min(1)
        .max(maxQueries)
        .describe(
          `Array of queries for ${toolName}. Maximum is ${maxQueries} queries per call. ` +
            'Multiple queries run in parallel. Use the per-query `page` field to navigate through result lists and responseCharOffset/responseCharLength to page the final formatted response.'
        ),
      ...responsePaginationFields,
    })
    .superRefine((data, ctx) => {
      const ids = new Set<string>();
      data.queries.forEach((q: unknown, idx) => {
        if (
          q &&
          typeof q === 'object' &&
          'id' in q &&
          typeof q.id === 'string'
        ) {
          if (ids.has(q.id)) {
            ctx.addIssue({
              code: 'custom',
              message: `Duplicate query id "${q.id}" at index ${idx}`,
              path: ['queries', idx, 'id'],
            });
          }
          ids.add(q.id);
        }
      });
    });
}

export const optionalMetaFields = {
  id: z.string().optional(),
  mainResearchGoal: z.string().optional(),
  researchGoal: z.string().optional(),
  reasoning: z.string().optional(),
} as const;
