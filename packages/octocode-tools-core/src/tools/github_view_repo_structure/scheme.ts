import { z } from 'zod';
import { GitHubViewRepoStructureQuerySchema as CoreGitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE } from '../../config.js';
import { LOCAL_MAX_DEPTH } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import {
  ItemPaginationSchema,
  ToolContinuationSchema,
} from '../../scheme/pagination.js';

// Field set + descriptions (incl. includeSizes) come from octocode-core; the
// runtime only relaxes the numeric/pagination bounds (clamp instead of reject).
const queryOverrides = {
  maxDepth: clampedInt(0, LOCAL_MAX_DEPTH).optional(),
  page: relaxedPageNumberField.default(1),
  itemsPerPage: clampedInt(1, GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE).optional(),
} as const;

export const GitHubViewRepoStructureQueryLocalSchema = describeQuerySchema(
  CoreGitHubViewRepoStructureQuerySchema,
  queryOverrides
);

export const GitHubViewRepoStructureBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    createQueryShapeSchema(
      CoreGitHubViewRepoStructureQuerySchema,
      queryOverrides
    )
  );

// The upstream octocode-core output schema describes a different envelope
// (data.entries[]) than this tool actually emits (results[].data.structure[]);
// extending it declared almost nothing the runtime sends, so schema validation
// could not catch drift and clients reading outputSchema learned nothing.
// Declare the real local envelope explicitly; keep passthrough for additive
// runtime fields.
const StructureDirEntrySchema = z
  .object({
    dir: z.string().optional(),
    files: z.array(z.string()).optional(),
    folders: z.array(z.string()).optional(),
  })
  .passthrough();

const RepoStructureResultDataSchema = z
  .object({
    structure: z.array(StructureDirEntrySchema).optional(),
    // Keyed by repo-relative file path; values are byte sizes (includeSizes).
    fileSizes: z.record(z.string(), z.number()).optional(),
    summary: z
      .object({
        totalFiles: z.number().optional(),
        totalFolders: z.number().optional(),
        truncated: z.boolean().optional(),
      })
      .passthrough()
      .optional(),
    resolvedBranch: z.string().optional(),
    pagination: ItemPaginationSchema.optional(),
    next: z.record(z.string(), ToolContinuationSchema).optional(),
    warnings: z.array(z.string()).optional(),
    // status:"error" rows carry the query identity plus the failure details.
    owner: z.string().optional(),
    repo: z.string().optional(),
    path: z.string().optional(),
    error: z.string().optional(),
    statusCode: z.number().optional(),
    errorType: z.string().optional(),
  })
  .passthrough();

export const GitHubViewRepoStructureOutputLocalSchema = z
  .object({
    results: z
      .array(
        z
          .object({
            id: z.string().optional(),
            status: z.string().optional(),
            data: RepoStructureResultDataSchema.optional(),
          })
          .passthrough()
      )
      .optional(),
  })
  .extend(responseEnvelopeFields);
