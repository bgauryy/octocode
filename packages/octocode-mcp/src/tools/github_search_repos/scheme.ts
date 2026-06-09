import { z } from 'zod';
import { GitHubReposSearchSingleQuerySchema as UpstreamGitHubReposSearchSingleQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubSearchRepositoriesOutputSchema as UpstreamReposOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  createRelaxedBulkQuerySchema,
  DEFAULT_PAGE_SIZE,
  describeField,
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
      keywordsToSearch: describeField(
        UpstreamGitHubReposSearchSingleQuerySchema.shape.keywordsToSearch,
        'Repository name/description keywords — each array element is a separate AND term. Do NOT use multi-word phrases in one element (["react","hooks"] not ["react hooks"]). Prefer fewer, distinctive terms.'
      ),
      topicsToSearch: describeField(
        UpstreamGitHubReposSearchSingleQuerySchema.shape.topicsToSearch,
        'Self-reported GitHub topics. Useful but sparse; language is more reliable for language filtering.'
      ),
      owner: describeField(
        UpstreamGitHubReposSearchSingleQuerySchema.shape.owner,
        'Optional owner/org scope for repository discovery. Supply owner without keywordsToSearch to enumerate ALL repositories in an org or user account (uses the listing endpoint, bypasses the 1 000-result search cap). Supply owner WITH keywords to scope a keyword search to that org.'
      ),
      language: z
        .string()
        .optional()
        .describe(
          'Primary repository language qualifier, based on GitHub language detection. Prefer this over topicsToSearch for language filters.'
        ),
      archived: z
        .boolean()
        .optional()
        .describe(
          'Include archived repositories. Default (omitted/false) excludes them. Set true to find archived/deprecated projects.'
        ),
      sort: z
        .enum(['stars', 'forks', 'help-wanted-issues', 'updated', 'best-match'])
        .optional()
        .describe(
          'Sort field for repository results. Omit (or "best-match") for relevance ranking.'
        ),
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} repositories.`
        ),
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
