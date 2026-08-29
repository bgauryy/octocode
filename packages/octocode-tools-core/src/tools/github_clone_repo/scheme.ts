import { CloneRepoQuerySchema } from '../../toolContract/schemas.js';
import { createRelaxedBulkQuerySchema } from '../../scheme/fields.js';
import { describeQuerySchema } from '../../scheme/coreSchemas.js';
import type { ToolContinuation } from '../../scheme/pagination.js';
import type { ResponsePaginationInfo } from '../../types/toolOutput.js';

export const CloneRepoQueryLocalSchema =
  describeQuerySchema(CloneRepoQuerySchema);

export const BulkCloneRepoLocalSchema = createRelaxedBulkQuerySchema(
  CloneRepoQueryLocalSchema
);

// ---------------------------------------------------------------------------
// Output TYPE — describes what ghCloneRepo returns. No zod: the MCP server
// registers no outputSchema, so this is a plain structural type (the source of
// truth for the shape is the clone execution, not a schema).
// ---------------------------------------------------------------------------
interface GitHubCloneRepoData {
  localPath: string;
  resolvedBranch?: string;
  cached?: boolean;
  sparsePath?: string;
  cloneTimeMs?: number;
  totalSize?: number;
  fileCount?: number;
  location?: Record<string, unknown>;
}

export type GitHubCloneRepoOutputLocal = {
  status?: string;
  data?: GitHubCloneRepoData;
  localPath?: string;
  resolvedBranch?: string;
  cached?: boolean;
  sparsePath?: string;
  location?: Record<string, unknown>;
  warnings?: string[];
  error?: string;
  base?: string;
  shared?: Record<string, string | number | boolean>;
  responsePagination?: ResponsePaginationInfo;
  next?: Record<string, ToolContinuation>;
} & Record<string, unknown>;
