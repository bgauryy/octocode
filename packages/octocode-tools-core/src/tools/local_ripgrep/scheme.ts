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

const queryOverrides = {
  contextLines: contextLinesField,
  matchContentLength: clampedInt(1, MAX_MATCH_CONTENT_LENGTH)
    .optional()
    .default(500),
  maxMatchesPerFile: clampedInt(1, MAX_MATCH_CONTENT_LENGTH).optional(),
  maxFiles: clampedInt(1, MAX_MATCH_CONTENT_LENGTH).optional(),
  sort: z
    .enum(['path', 'modified', 'accessed', 'created'])
    .optional()
    .default('path'),
  matchPage: relaxedPageNumberField.optional(),
  itemsPerPage: clampedInt(1, MAX_PAGE_NUMBER).optional(),
  page: relaxedPageNumberField.default(1),
} as const;

const RipgrepQueryShape = createQueryShapeSchema(
  CoreRipgrepQuerySchema,
  queryOverrides
);

export const LocalRipgrepQuerySchema = describeQuerySchema(
  CoreRipgrepQuerySchema,
  queryOverrides
);

export type RipgrepQuery = z.infer<typeof LocalRipgrepQuerySchema>;

// Bulk uses the base shape (no mutex superRefine) so one invalid query
// does not reject the whole batch. Per-query mutex checks run at execution.
export const LocalRipgrepBulkQuerySchema = createRelaxedBulkQuerySchema(
  RipgrepQueryShape,
  { maxQueries: 5 }
);
