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
import type {
  ItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

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

// ---------------------------------------------------------------------------
// Output TYPES — describes what ghViewRepoStructure returns. No zod: the MCP
// server registers no outputSchema. The upstream octocode-core output schema
// described a different envelope (data.entries[]) than this tool actually emits
// (results[].data.structure[]); this declares the real local envelope. Index
// signatures mirror the original .passthrough() for additive runtime fields.
// Shared envelope lives in types/toolOutput.ts.
// ---------------------------------------------------------------------------

export interface StructureDirEntry {
  dir?: string;
  files?: string[];
  folders?: string[];
  [key: string]: unknown;
}

export interface RepoStructureResultData {
  structure?: StructureDirEntry[];
  // Keyed by repo-relative file path; values are byte sizes (includeSizes).
  fileSizes?: Record<string, number>;
  summary?: {
    totalFiles?: number;
    totalFolders?: number;
    truncated?: boolean;
    [key: string]: unknown;
  };
  resolvedBranch?: string;
  pagination?: ItemPagination;
  next?: Record<string, ToolContinuation>;
  warnings?: string[];
  // status:"error" rows carry the query identity plus the failure details.
  owner?: string;
  repo?: string;
  path?: string;
  error?: string;
  statusCode?: number;
  errorType?: string;
  [key: string]: unknown;
}

export type GitHubViewRepoStructureOutputLocal =
  BulkToolOutput<RepoStructureResultData>;
