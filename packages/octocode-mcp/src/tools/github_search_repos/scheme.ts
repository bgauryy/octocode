import { z } from 'zod';
import { GitHubReposSearchSingleQuerySchema as UpstreamGitHubReposSearchSingleQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubSearchRepositoriesOutputSchema as UpstreamReposOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  createRelaxedBulkQuerySchema,
  optionalMetaFields,
  relaxedPageNumberField,
  withCoreSchemaDescriptions,
} from '../../scheme/localSchemaOverlay.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

export const GitHubReposSearchSingleQueryLocalSchema =
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    UpstreamGitHubReposSearchSingleQuerySchema.omit({ limit: true }).extend({
      ...optionalMetaFields,
      language: z.string().optional(),
      archived: z.boolean().optional(),
      sort: z
        .enum(['stars', 'forks', 'help-wanted-issues', 'updated', 'best-match'])
        .optional(),
      page: relaxedPageNumberField.default(1),
    })
  );

export const GitHubReposSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    GitHubReposSearchSingleQueryLocalSchema
  );

const LocalRepositoryDetailSchema = z.object({
  owner: z.string(),
  repo: z.string(),
  stars: z.number().optional(),
  forks: z.number().optional(),
  language: z.string().optional(),
  description: z.string().optional(),
  pushedAt: z.string().optional(),
  defaultBranch: z.string().optional(),
  topics: z.array(z.string()).optional(),
  visibility: z.string().optional(),
  // verbose-mode extras (query.verbose=true)
  url: z.string().optional(),
  createdAt: z.string().optional(),
  updatedAt: z.string().optional(),
  openIssuesCount: z.number().optional(),
});

export const GitHubSearchRepositoriesOutputLocalSchema =
  UpstreamReposOutput.extend({
    ...responseEnvelopeFields,
    data: z
      .object({
        repositories: z.array(LocalRepositoryDetailSchema),
        pagination: z
          .object({
            currentPage: z.number(),
            totalPages: z.number(),
            hasMore: z.boolean(),
            perPage: z.number().optional(),
            totalMatches: z.number().optional(),
          })
          .optional(),
      })
      .optional(),
  });
