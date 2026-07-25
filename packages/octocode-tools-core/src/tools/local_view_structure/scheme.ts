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
  maxDepth: clampedInt(0, LOCAL_MAX_DEPTH)
    .optional()
    .describe(
      `Maximum recursion depth: 1 = the target directory's immediate children, 2 = children + grandchildren, and so on (max ${LOCAL_MAX_DEPTH}). Setting maxDepth on its own enables recursion to that depth — recursive:true is not required. Effective depth when OMITTED: 1 (immediate children only) if recursive is unset, or 5 if recursive:true. maxDepth:0 is treated as unset and falls back to those defaults.`
    ),
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
  warnings?: string[];
}

export type LocalViewStructureOutput = BulkToolOutput<LocalViewStructureData>;
