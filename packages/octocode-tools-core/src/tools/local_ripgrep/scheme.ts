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
  matchPage: relaxedPageNumberField.optional(),
  itemsPerPage: clampedInt(1, MAX_PAGE_NUMBER).optional(),
  page: relaxedPageNumberField.default(1),
} as const;

const RipgrepQueryShape = createQueryShapeSchema(
  CoreRipgrepQuerySchema,
  queryOverrides
);

// Structural-mode validation (exactly one of pattern/rule, reject ripgrep-only
// fields, require keywords otherwise) is enforced by the core RipgrepQuerySchema
// superRefine, which describeQuerySchema preserves through to this schema.
export const LocalRipgrepQuerySchema = describeQuerySchema(
  CoreRipgrepQuerySchema,
  queryOverrides,
  { strict: true }
);

export type RipgrepQuery = z.infer<typeof LocalRipgrepQuerySchema>;

export const LocalRipgrepBulkQuerySchema = createRelaxedBulkQuerySchema(
  RipgrepQueryShape,
  { maxQueries: 5 }
);
