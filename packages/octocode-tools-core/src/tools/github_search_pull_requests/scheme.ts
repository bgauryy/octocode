import { z } from 'zod';
import { GitHubPullRequestSearchQuerySchema as CoreGitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubSearchPullRequestsOutputSchema as UpstreamPRsOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  GITHUB_SEARCH_MAX_LIMIT,
  MAX_CHAR_LENGTH,
  PR_CONTENT_MAX_ITEMS_PER_PAGE,
} from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';

const minifyField = z.enum(['none', 'standard']).optional().default('standard');
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';

const queryOverrides = {
  prNumber: clampedInt(1, 1_000_000_000).optional(),
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT).optional(),
  page: relaxedPageNumberField.default(1),
  filePage: relaxedPageNumberField.optional(),
  commentPage: relaxedPageNumberField.optional(),
  commitPage: relaxedPageNumberField.optional(),
  itemsPerPage: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE).optional(),
  charOffset: clampedInt(0, 100_000_000).optional(),
  charLength: clampedInt(1, MAX_CHAR_LENGTH).optional(),
  minify: minifyField,
} as const;

const GitHubPullRequestSearchQueryShape = createQueryShapeSchema(
  CoreGitHubPullRequestSearchQuerySchema,
  queryOverrides
);

export const GitHubPullRequestSearchQueryLocalSchema = describeQuerySchema(
  CoreGitHubPullRequestSearchQuerySchema,
  queryOverrides
);

export const GitHubPullRequestSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(GitHubPullRequestSearchQueryShape);

export const GitHubSearchPullRequestsOutputLocalSchema =
  UpstreamPRsOutput.extend({
    // executeBulkOperation wraps per-query data in a results array; extend the
    // upstream passthrough schema to advertise this envelope to clients.
    results: z
      .array(
        z
          .object({
            id: z.string().optional(),
            status: z.string().optional(),
            // PR data shape varies with requested content fields; passthrough
            // avoids falsely rejecting any combination. pull_requests is the
            // primary output key (snake_case, matches GitHub API naming).
            data: z
              .object({
                pull_requests: z.array(z.object({}).passthrough()).optional(),
              })
              .passthrough()
              .optional(),
          })
          .passthrough()
      )
      .optional(),
    ...responseEnvelopeFields,
  });
