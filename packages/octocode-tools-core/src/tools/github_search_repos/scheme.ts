import { z } from 'zod';
import { GitHubReposSearchSingleQuerySchema as CoreGitHubReposSearchSingleQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GITHUB_SEARCH_MAX_LIMIT } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { ItemPaginationSchema } from '../../scheme/pagination.js';

const queryOverrides = {
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT).optional(),
  page: relaxedPageNumberField.default(1),
} as const;

export const GitHubReposSearchSingleQueryLocalSchema = describeQuerySchema(
  CoreGitHubReposSearchSingleQuerySchema,
  queryOverrides
);

export const GitHubReposSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    createQueryShapeSchema(
      CoreGitHubReposSearchSingleQuerySchema,
      queryOverrides
    )
  );

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

// Repo-search-specific pagination: canonical base + search-confidence fields.
const RepoSearchPaginationSchema = ItemPaginationSchema.extend({
  totalMatchesKind: z.enum(['exact', 'reported', 'lowerBound']).optional(),
  totalMatchesCapped: z.boolean().optional(),
}).optional();

const RepositoryResultDataSchema = z
  .object({
    repositories: z
      .array(z.union([z.string(), LocalRepositoryDetailSchema]))
      .optional(),
    pagination: RepoSearchPaginationSchema,
  })
  .passthrough();

export const GitHubSearchRepositoriesOutputLocalSchema = z
  .object({
    results: z
      .array(
        z
          .object({
            id: z.string().optional(),
            status: z.string().optional(),
            data: RepositoryResultDataSchema.optional(),
          })
          .passthrough()
      )
      .optional(),
  })
  .extend(responseEnvelopeFields);
