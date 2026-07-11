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
import { bulkOutputEnvelopeFields } from '../../scheme/responseEnvelope.js';
import {
  CharPaginationSchema,
  ItemPaginationSchema,
  ToolContinuationSchema,
} from '../../scheme/pagination.js';

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

// ---------------------------------------------------------------------------
// Output schema — describes what localGetFileContent returns per query result.
//
// A single query can return either:
//   - a char-paginated content window (startLine/endLine / matchString / full)
//   - a line-range extraction result
// Both modes share the same result row shape; pagination discriminates.
// ---------------------------------------------------------------------------

const FileContentMatchRangeSchema = z.object({
  start: z.number(),
  end: z.number(),
});

const LocalGetFileContentDataSchema = z.object({
  path: z.string().optional(),
  content: z.string().optional(),
  // isSkeleton was dropped — always equal to contentView==='symbols', so it
  // carried no information a consumer couldn't already derive from contentView.
  contentView: z.enum(['none', 'standard', 'symbols']).optional(),
  totalLines: z.number().optional(),
  sourceChars: z.number().optional(),
  sourceBytes: z.number().optional(),
  startLine: z.number().optional(),
  endLine: z.number().optional(),
  isPartial: z.boolean().optional(),
  matchRanges: z.array(FileContentMatchRangeSchema).optional(),
  // Char pagination for content windows
  pagination: z
    .union([
      CharPaginationSchema.extend({
        nextBlockChar: z.number().optional(),
      }),
      ItemPaginationSchema,
    ])
    .optional(),
  next: z.record(z.string(), ToolContinuationSchema).optional(),
  modified: z.string().optional(),
  lastModified: z.string().optional(),
  lastModifiedBy: z.string().optional(),
  warnings: z.array(z.string()).optional(),
  matchNotFound: z.boolean().optional(),
  searchedFor: z.string().optional(),
});

export const LocalGetFileContentOutputSchema = z
  .object({
    results: z.array(
      z.object({
        id: z.string(),
        status: z.enum(['empty', 'error']).optional(),
        data: LocalGetFileContentDataSchema,
      })
    ),
  })
  .extend(bulkOutputEnvelopeFields);

export type LocalGetFileContentOutput = z.infer<
  typeof LocalGetFileContentOutputSchema
>;
