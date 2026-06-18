import { z } from 'zod';
import { GitHubPullRequestSearchQuerySchema as CoreGitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubSearchPullRequestsOutputSchema as UpstreamPRsOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
  MAX_CHAR_LENGTH,
  PR_CONTENT_DEFAULT_ITEMS_PER_PAGE,
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

import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';

// Field set, enums, defaults and descriptions all come from octocode-core
// (GitHubPullRequestSearchQuerySchema). The runtime only overrides the numeric /
// pagination fields to apply *relaxed* validation (clamp instead of reject) — and
// omits .describe() so the description is inherited from core (see copyDescription
// in ../../scheme/coreSchemas.ts). One source of truth; no duplicated prose.
const queryOverrides = {
  perPage: clampedInt(1, 100).optional().default(30),
  prNumber: clampedInt(1, 1_000_000_000).optional(),
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT)
    .optional()
    .default(GITHUB_SEARCH_DEFAULT_LIMIT),
  page: relaxedPageNumberField.default(1),
  filePage: relaxedPageNumberField.optional(),
  commentPage: relaxedPageNumberField.optional(),
  commitPage: relaxedPageNumberField.optional(),
  itemsPerPage: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE)
    .optional()
    .default(PR_CONTENT_DEFAULT_ITEMS_PER_PAGE),
  charOffset: clampedInt(0, 100_000_000).optional(),
  commentBodyOffset: clampedInt(0, 100_000_000).optional(),
  charLength: clampedInt(1, MAX_CHAR_LENGTH).optional(),
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
    results: z
      .array(
        z
          .object({
            id: z.string().optional(),
            status: z.string().optional(),
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
