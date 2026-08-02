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
import type {
  LocalItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

const queryOverrides = {
  excludeDir: z
    .array(z.string())
    .optional()
    .describe(
      'Directory names to prune from recursive walks (default: common generated/vendor dirs such as node_modules, dist, build, out, coverage, target). Pass [] to inspect everything.'
    ),
  // Description flows from @octocodeai/octocode-core prose (provenance:
  // resources-only) — only the bounds are tightened here.
  maxDepth: clampedInt(0, LOCAL_MAX_DEPTH).optional(),
  limit: clampedInt(1, LOCAL_MAX_LIMIT).optional(),
  page: relaxedPageNumberField.default(1),
  itemsPerPage: clampedInt(1, LOCAL_MAX_FILES_PER_PAGE).optional(),
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
// Output TYPES — describes what localViewStructure returns per query result.
// No zod: the MCP server registers no outputSchema, so the output is a plain
// type. Shared envelope lives in types/toolOutput.ts.
// ---------------------------------------------------------------------------

export interface LocalViewStructureEntry {
  name?: string;
  type: 'file' | 'dir' | 'directory' | 'link' | 'symlink';
  path?: string;
  absolutePath?: string;
  uri?: string;
  depth?: number;
  size?: number | string;
  sizeBytes?: number;
  modified?: string;
  permissions?: string;
}

export interface LocalViewStructureData {
  path?: string;
  entries?: LocalViewStructureEntry[];
  // grouped list variants
  files?: string[];
  folders?: string[];
  links?: string[];
  summary?: string | Record<string, unknown>;
  pagination?: LocalItemPagination;
  next?: Record<string, ToolContinuation>;
}

export type LocalViewStructureOutput = BulkToolOutput<LocalViewStructureData>;
