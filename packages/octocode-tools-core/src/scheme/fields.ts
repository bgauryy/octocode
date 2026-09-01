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

// Offsets are cursors, not resource budgets. Keeping an arbitrary maximum here
// can make a continuation emitted for a large resource fail its own schema.
export const offsetField = z.preprocess(
  value =>
    typeof value === 'number' && Number.isFinite(value)
      ? Math.max(value, 0)
      : value,
  z
    .number()
    .nonnegative()
    .refine(Number.isSafeInteger, 'Expected a safe non-negative integer')
);

const responsePaginationFields = {
  responseCharOffset: offsetField
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
  return z
    .object({
      queries: z
        .array(querySchema)
        .min(1)
        .max(maxQueries)
        .describe(
          'Parallel queries; response rows use matching zero-based indexes.'
        ),
      ...responsePaginationFields,
    })
    .strict();
}
