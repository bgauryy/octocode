import { z } from 'zod';
import {
  LOCAL_MAX_DEPTH,
  LOCAL_MAX_LIMIT,
  MAX_CONTEXT_LINES,
  MAX_PAGE_NUMBER,
} from '../config.js';

export function clampedInt(min: number, max: number) {
  return z.preprocess(
    v =>
      typeof v === 'number' && Number.isFinite(v)
        ? Math.min(Math.max(v, min), max)
        : v,
    z.number().int().min(min).max(max)
  );
}

export const LOCAL_OVERLAY_MAX_LIMIT = LOCAL_MAX_LIMIT;
export const LOCAL_OVERLAY_MAX_DEPTH = LOCAL_MAX_DEPTH;

const LOCAL_OVERLAY_MAX_LINE = 1_000_000_000;
const LOCAL_OVERLAY_MAX_ORDER_HINT = 100_000;
const LOCAL_OVERLAY_MAX_PAGINATION_LIMIT = MAX_PAGE_NUMBER;
const LOCAL_OVERLAY_MAX_CONTEXT_LINES = MAX_CONTEXT_LINES;

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

/** minify enum for the fetch-content tools: none (raw) | standard | symbols. Defaults to 'standard'. */
export const minifyFieldWithSymbols = z
  .enum(['none', 'standard', 'symbols'])
  .optional()
  .default('standard');

/** minify enum for PR patches: none (raw exact diffs) | standard (token-saving view). Defaults to 'standard'. */
export const minifyFieldStandard = z
  .enum(['none', 'standard'])
  .optional()
  .default('standard');

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
