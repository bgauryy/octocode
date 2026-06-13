import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { GITHUB_SEARCH_MAX_LIMIT } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES]
    ?.schema,
} as Record<string, string>;

const GitHubReposSearchQuerySchema = z.object({
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
  keywordsToSearch: z
    .array(z.string())
    .optional()
    .describe(QUERY_DESCRIPTIONS.keywordsToSearch!),
  topicsToSearch: z
    .array(z.string())
    .optional()
    .describe(QUERY_DESCRIPTIONS.topicsToSearch!),
  language: z.string().optional().describe(QUERY_DESCRIPTIONS.language!),
  owner: z.string().optional().describe(QUERY_DESCRIPTIONS.owner!),
  stars: z.string().optional().describe(QUERY_DESCRIPTIONS.stars!),
  size: z.string().optional().describe(QUERY_DESCRIPTIONS.size!),
  created: z.string().optional().describe(QUERY_DESCRIPTIONS.created!),
  updated: z.string().optional().describe(QUERY_DESCRIPTIONS.updated!),
  match: z
    .array(z.enum(['name', 'description', 'readme']))
    .optional()
    .describe(QUERY_DESCRIPTIONS.match!),
  sort: z
    .enum(['stars', 'forks', 'help-wanted-issues', 'updated', 'best-match'])
    .optional()
    .describe(QUERY_DESCRIPTIONS.sort!),
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT)
    .optional()
    .describe(QUERY_DESCRIPTIONS.limit!),
  page: relaxedPageNumberField.default(1).describe(QUERY_DESCRIPTIONS.page!),
  archived: z.boolean().optional().describe(QUERY_DESCRIPTIONS.archived!),
  visibility: z
    .enum(['public', 'private'])
    .optional()
    .describe(QUERY_DESCRIPTIONS.visibility!),
  forks: z.string().optional().describe(QUERY_DESCRIPTIONS.forks!),
  license: z.string().optional().describe(QUERY_DESCRIPTIONS.license!),
  goodFirstIssues: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.goodFirstIssues!),
  verbose: z.boolean().optional().describe(QUERY_DESCRIPTIONS.verbose!),
});

export const GitHubReposSearchSingleQueryLocalSchema =
  GitHubReposSearchQuerySchema;

export const GitHubReposSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(GitHubReposSearchQuerySchema);

const LocalRepositoryDetailSchema = z.object({
  owner: z.string(),
  repo: z.string(),
  stars: z.number().optional(),
  forks: z.number().optional(),
  openIssuesCount: z.number().optional(),
  language: z.string().optional(),
  license: z.string().optional(),
  description: z.string().optional(),
  homepage: z.string().optional(),
  pushedAt: z.string().optional(),
  createdAt: z.string().optional(),
  defaultBranch: z.string().optional(),
  topics: z.array(z.string()).optional(),
  visibility: z.string().optional(),
  url: z.string().optional(),
  updatedAt: z.string().optional(),
});

export const GitHubSearchRepositoriesOutputLocalSchema = z
  .object({
    data: z
      .object({
        // Lean mode returns compact strings ("owner/repo ★stars …");
        // verbose=true returns full structured objects.
        repositories: z.array(
          z.union([z.string(), LocalRepositoryDetailSchema])
        ),
        pagination: z
          .object({
            currentPage: z.number(),
            totalPages: z.number(),
            hasMore: z.boolean(),
            perPage: z.number().optional(),
            totalMatches: z.number().optional(),
            reportedTotalMatches: z.number().optional(),
            reachableTotalMatches: z.number().optional(),
            totalMatchesKind: z
              .enum(['exact', 'reported', 'lowerBound'])
              .optional(),
            totalMatchesCapped: z.boolean().optional(),
          })
          .optional(),
      })
      .optional(),
  })
  .extend(responseEnvelopeFields);
