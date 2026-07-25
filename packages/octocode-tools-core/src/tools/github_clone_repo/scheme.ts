import type { z } from 'zod';
import { CloneRepoQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubCloneRepoOutputSchema as UpstreamCloneRepoOutput } from '@octocodeai/octocode-core/schemas/outputs';
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
// registers no outputSchema. Faithfully mirrors the deleted
// `UpstreamCloneRepoOutput.extend(responseEnvelopeFields)`: the upstream
// output's inferred type plus the shared response-envelope fields.
// ---------------------------------------------------------------------------
export type GitHubCloneRepoOutputLocal = z.infer<
  typeof UpstreamCloneRepoOutput
> & {
  base?: string;
  shared?: Record<string, string | number | boolean>;
  responsePagination?: ResponsePaginationInfo;
  next?: Record<string, ToolContinuation>;
};
