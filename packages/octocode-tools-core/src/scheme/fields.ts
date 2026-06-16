import { z } from 'zod';
import { MAX_CONTEXT_LINES, MAX_PAGE_NUMBER } from '../config.js';

export function clampedInt(min: number, max: number) {
  return z.preprocess(
    v =>
      typeof v === 'number' && Number.isFinite(v)
        ? Math.min(Math.max(v, min), max)
        : v,
    z.number().int().min(min).max(max)
  );
}

export type MinifyMode = 'none' | 'standard' | 'symbols';

export const relaxedPageNumberField = clampedInt(1, MAX_PAGE_NUMBER)
  .optional()
  .default(1);

export const contextLinesField = clampedInt(0, MAX_CONTEXT_LINES).optional();

export const lineNumberField = clampedInt(1, 1_000_000_000).optional();

const responsePaginationFields = {
  responseCharOffset: clampedInt(0, 100_000_000)
    .optional()
    .describe(
      'Full-response char offset; re-call with returned value when hasMore.'
    ),
  responseCharLength: clampedInt(1, 50_000)
    .optional()
    .describe('Full-response char window.'),
} as const;

export function createRelaxedBulkQuerySchema(
  querySchema: z.ZodTypeAny,
  options: { maxQueries?: number } = {}
) {
  const { maxQueries = 5 } = options;
  // Strip unknown envelope keys instead of rejecting them, so a stray/legacy
  // top-level field never hard-fails the whole bulk call with a schema mismatch.
  return z
    .object({
      queries: z
        .array(querySchema)
        .min(1)
        .max(maxQueries)
        .describe('Parallel queries.'),
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
