import { z } from 'zod';
import { FetchContentQuerySchema as CoreFetchContentQuerySchema } from '@octocodeai/octocode-core/schemas';
import { MAX_CHAR_LENGTH } from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  lineNumberField,
  type MinifyMode,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';

const minifyField = z
  .enum(['none', 'standard', 'symbols'])
  .optional()
  .default('standard');

const queryOverrides = {
  startLine: lineNumberField,
  endLine: lineNumberField,
  contextLines: contextLinesField.default(5),
  charOffset: clampedInt(0, 100_000_000).optional(),
  charLength: clampedInt(1, MAX_CHAR_LENGTH).optional(),
  minify: minifyField,
} as const;

const FetchContentQueryShape = createQueryShapeSchema(
  CoreFetchContentQuerySchema,
  queryOverrides
);

export const LocalFetchContentQuerySchema = describeQuerySchema(
  CoreFetchContentQuerySchema,
  queryOverrides
);

export type FetchContentQuery = z.infer<typeof LocalFetchContentQuerySchema> & {
  minify?: MinifyMode;
};

export const LocalFetchContentBulkQuerySchema = createRelaxedBulkQuerySchema(
  FetchContentQueryShape,
  { maxQueries: 5 }
);
