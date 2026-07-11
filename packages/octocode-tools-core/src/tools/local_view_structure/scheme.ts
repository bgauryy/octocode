import { z } from 'zod';
import { ViewStructureQuerySchema as CoreViewStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  LOCAL_MAX_DEPTH,
  LOCAL_MAX_FILES_PER_PAGE,
  LOCAL_MAX_LIMIT,
} from '../../config.js';
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
import {
  LocalItemPaginationSchema,
  ToolContinuationSchema,
} from '../../scheme/pagination.js';

const queryOverrides = {
  maxDepth: clampedInt(0, LOCAL_MAX_DEPTH).optional(),
  limit: clampedInt(1, LOCAL_MAX_LIMIT).optional(),
  page: relaxedPageNumberField.default(1),
  itemsPerPage: clampedInt(1, LOCAL_MAX_FILES_PER_PAGE).optional(),
  // Filters a directory LISTING down to file entries (excludes
  // subdirectories). Unrelated to localSearchCode's `filesOnly`, which
  // instead filters search results down to matching file paths.
  filesOnly: z
    .boolean()
    .optional()
    .describe(
      "Returns files only. Mutually exclusive with directoriesOnly. (Unlike localSearchCode's `filesOnly`, which filters search results to matching file paths — a different concept sharing this name.)"
    ),
} as const;

const ViewStructureQueryShape = createQueryShapeSchema(
  CoreViewStructureQuerySchema,
  queryOverrides
);

export const LocalViewStructureQuerySchema = describeQuerySchema(
  CoreViewStructureQuerySchema,
  queryOverrides
);
export type ViewStructureQuery = z.infer<typeof LocalViewStructureQuerySchema>;

export const LocalViewStructureBulkQuerySchema = createRelaxedBulkQuerySchema(
  ViewStructureQueryShape,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output schema — describes what localViewStructure returns per query result.
// ---------------------------------------------------------------------------

const ViewStructureEntrySchema = z.object({
  name: z.string().optional(),
  type: z.enum(['file', 'dir', 'directory', 'link', 'symlink']),
  path: z.string().optional(),
  depth: z.number().optional(),
  size: z.union([z.number(), z.string()]).optional(),
  sizeBytes: z.number().optional(),
  modified: z.string().optional(),
  permissions: z.string().optional(),
});

const LocalViewStructureDataSchema = z.object({
  path: z.string().optional(),
  entries: z.array(ViewStructureEntrySchema).optional(),
  // grouped list variants
  files: z.array(z.string()).optional(),
  folders: z.array(z.string()).optional(),
  links: z.array(z.string()).optional(),
  summary: z.union([z.string(), z.record(z.string(), z.unknown())]).optional(),
  pagination: LocalItemPaginationSchema.optional(),
  next: z.record(z.string(), ToolContinuationSchema).optional(),
  warnings: z.array(z.string()).optional(),
});

export const LocalViewStructureOutputSchema = z
  .object({
    results: z.array(
      z.object({
        id: z.string(),
        status: z.enum(['empty', 'error']).optional(),
        data: LocalViewStructureDataSchema,
      })
    ),
  })
  .extend(bulkOutputEnvelopeFields);

export type LocalViewStructureOutput = z.infer<
  typeof LocalViewStructureOutputSchema
>;
