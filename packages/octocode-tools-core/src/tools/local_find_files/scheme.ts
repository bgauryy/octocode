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
import type {
  LocalItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

const queryOverrides = {
  maxDepth: clampedInt(0, 100).optional(),
  minDepth: clampedInt(0, 100).optional(),
  limit: clampedInt(1, LOCAL_MAX_LIMIT)
    .optional()
    .describe(
      'Discovery cap applied after sort, before pagination — total results are capped here; itemsPerPage/page page within that cap.'
    ),
  page: relaxedPageNumberField.default(1),
  itemsPerPage: clampedInt(1, LOCAL_MAX_FILES_PER_PAGE).optional(),
  // Core's description can lag runtime behavior. The executor prunes common
  // generated/vendor dirs by default but honors excludeDir: [] to prune nothing.
  excludeDir: z
    .array(z.string())
    .optional()
    .describe(
      'Directory names to prune from the walk. Common generated/vendor dirs are pruned by default (node_modules, .git, dist, build, out, coverage, target, .next, .cache); pass [] to prune nothing, or pass an explicit list to choose pruned dirs.'
    ),
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
// Output TYPES — localFindFiles result shape. No zod: the output was never
// validated at runtime (MCP registers no outputSchema), so it is a plain type.
// Shared envelope lives in types/toolOutput.ts.
// ---------------------------------------------------------------------------

export interface LocalFindFilesEntryOutput {
  name?: string;
  path?: string;
  type?: 'file' | 'dir' | 'directory' | 'link' | 'symlink';
  size?: number | string;
  sizeFormatted?: string;
  modified?: string;
  permissions?: string;
}

export interface LocalFindFilesData {
  path?: string;
  files?: LocalFindFilesEntryOutput[];
  summary?: string;
  pagination?: LocalItemPagination;
  next?: Record<string, ToolContinuation>;
}

export type LocalFindFilesOutput = BulkToolOutput<LocalFindFilesData>;
