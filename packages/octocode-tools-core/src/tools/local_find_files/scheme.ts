import { z } from 'zod';
import { FindFilesQuerySchema as CoreFindFilesQuerySchema } from '@octocodeai/octocode-core/schemas';
import { LOCAL_MAX_FILES_PER_PAGE, LOCAL_MAX_LIMIT } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import { bulkOutputEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { ItemPaginationSchema } from '../../scheme/pagination.js';

const queryOverrides = {
  maxDepth: clampedInt(0, 100).optional(),
  minDepth: clampedInt(0, 100).optional(),
  limit: clampedInt(1, LOCAL_MAX_LIMIT).optional(),
  page: relaxedPageNumberField.default(1),
  itemsPerPage: clampedInt(1, LOCAL_MAX_FILES_PER_PAGE).optional(),
} as const;

// Strip unknown keys (legacy/removed fields like regexType, or typos) instead
// of rejecting them — an unknown field must never hard-fail the whole call.
const CoreFindFilesBulkShapeSchema = z.object(
  Object.fromEntries(
    Object.entries(CoreFindFilesQuerySchema.shape).filter(
      ([field]) => field !== 'regexType'
    )
  ) as z.ZodRawShape
);

function validateDepthRange(
  data: { minDepth?: number; maxDepth?: number },
  ctx: z.RefinementCtx
): void {
  if (
    data.minDepth !== undefined &&
    data.maxDepth !== undefined &&
    data.minDepth > data.maxDepth
  ) {
    ctx.addIssue({
      code: 'custom',
      message: 'minDepth must be less than or equal to maxDepth.',
      path: ['minDepth'],
    });
  }
}

const FindFilesQueryShape = createQueryShapeSchema(
  CoreFindFilesBulkShapeSchema,
  queryOverrides
);

export type FindFilesQuery = Omit<
  z.infer<typeof CoreFindFilesQuerySchema>,
  'regexType'
>;

export const LocalFindFilesQuerySchema = describeQuerySchema(
  CoreFindFilesBulkShapeSchema,
  queryOverrides
).superRefine(validateDepthRange) as unknown as z.ZodType<FindFilesQuery>;

export const LocalFindFilesBulkQuerySchema = createRelaxedBulkQuerySchema(
  FindFilesQueryShape,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output schema — describes what localFindFiles returns per query result.
// ---------------------------------------------------------------------------

const FindFilesEntrySchema = z.object({
  name: z.string().optional(),
  path: z.string().optional(),
  type: z.enum(['file', 'dir', 'directory', 'link', 'symlink']).optional(),
  size: z.union([z.number(), z.string()]).optional(),
  modified: z.string().optional(),
  accessed: z.string().optional(),
  created: z.string().optional(),
  permissions: z.string().optional(),
});

const LocalFindFilesDataSchema = z.object({
  files: z.array(FindFilesEntrySchema).optional(),
  summary: z.string().optional(),
  pagination: ItemPaginationSchema.optional(),
  warnings: z.array(z.string()).optional(),
});

export const LocalFindFilesOutputSchema = z
  .object({
    results: z.array(
      z.object({
        id: z.string(),
        status: z.enum(['empty', 'error']).optional(),
        data: LocalFindFilesDataSchema,
      })
    ),
  })
  .extend(bulkOutputEnvelopeFields);

export type LocalFindFilesOutput = z.infer<typeof LocalFindFilesOutputSchema>;
