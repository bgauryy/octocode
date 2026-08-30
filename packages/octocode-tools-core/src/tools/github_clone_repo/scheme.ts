import { CloneRepoQuerySchema } from '../../toolContract/schemas.js';
import { createRelaxedBulkQuerySchema } from '../../scheme/fields.js';
import { describeQuerySchema } from '../../scheme/coreSchemas.js';
import type { ToolContinuation } from '../../scheme/pagination.js';

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
interface GitHubCloneRepoLocation {
  kind: 'repo' | 'tree';
  localPath: string;
  source: 'clone';
  cached: boolean;
  complete: boolean;
  resolvedBranch: string;
  requestedPath?: string;
}

interface GitHubCloneRepoData {
  owner: string;
  repo: string;
  totalSize: number;
  location: GitHubCloneRepoLocation;
  next: {
    viewStructure: ToolContinuation;
  };
}

export type GitHubCloneRepoOutputLocal = {
  results: Array<{
    index: number;
    status?: 'empty' | 'error';
    meta?: Record<string, unknown>;
    data: GitHubCloneRepoData | Record<string, unknown>;
  }>;
  base?: string;
  shared?: Record<string, string | number | boolean>;
} & Record<string, unknown>;
