import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { GitHubViewRepoStructureOutputSchema as UpstreamStructureOutput } from '@octocodeai/octocode-core/schemas/outputs';
import { GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE } from '../../config.js';
import { LOCAL_MAX_DEPTH } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE]
    ?.schema,
} as Record<string, string>;

const GitHubViewRepoStructureQuerySchema = z.object({
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
  path: z.string().optional().describe(QUERY_DESCRIPTIONS.path!),
  depth: clampedInt(0, LOCAL_MAX_DEPTH)
    .optional()
    .describe(QUERY_DESCRIPTIONS.depth!),
  page: relaxedPageNumberField.default(1).describe(QUERY_DESCRIPTIONS.page!),
  itemsPerPage: clampedInt(1, GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE)
    .optional()
    .describe(QUERY_DESCRIPTIONS.itemsPerPage!),
  verbose: z.boolean().optional().describe(QUERY_DESCRIPTIONS.verbose!),
});

export const GitHubViewRepoStructureQueryLocalSchema =
  GitHubViewRepoStructureQuerySchema;

export const GitHubViewRepoStructureBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(GitHubViewRepoStructureQuerySchema);

export const GitHubViewRepoStructureOutputLocalSchema =
  UpstreamStructureOutput.extend(responseEnvelopeFields);
