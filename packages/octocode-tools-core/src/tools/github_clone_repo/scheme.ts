import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { GitHubCloneRepoOutputSchema as UpstreamCloneRepoOutput } from '@octocodeai/octocode-core/schemas/outputs';
import { createRelaxedBulkQuerySchema } from '../../scheme/fields.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.GITHUB_CLONE_REPO]?.schema,
} as Record<string, string>;

const CloneRepoQuerySchema = z.object({
  id: z.string().optional().describe(QUERY_DESCRIPTIONS.id!),
  mainResearchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.mainResearchGoal!),
  researchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.researchGoal!),
  reasoning: z.string().optional().describe(QUERY_DESCRIPTIONS.reasoning!),
  owner: z.string().describe(QUERY_DESCRIPTIONS.owner!),
  repo: z.string().describe(QUERY_DESCRIPTIONS.repo!),
  branch: z.string().optional().describe(QUERY_DESCRIPTIONS.branch!),
  forceRefresh: z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS.forceRefresh!),
  sparsePath: z.string().optional().describe(QUERY_DESCRIPTIONS.sparsePath!),
});

export const CloneRepoQueryLocalSchema = CloneRepoQuerySchema;

export const BulkCloneRepoLocalSchema =
  createRelaxedBulkQuerySchema(CloneRepoQuerySchema);

export const GitHubCloneRepoOutputLocalSchema = UpstreamCloneRepoOutput.extend(
  responseEnvelopeFields
);
