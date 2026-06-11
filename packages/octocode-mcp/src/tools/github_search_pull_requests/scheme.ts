import { z } from 'zod';
import { GitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubSearchPullRequestsOutputSchema as UpstreamPRsOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  minifyFieldStandard,
  optionalMetaFields,
  relaxedPageNumberField,
  withCoreSchemaDescriptions,
} from '../../scheme/localSchemaOverlay.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const PrPartialContentMetadataLocalSchema = z.object({
  file: z
    .string()
    .describe(
      'File path relative to repo root, exactly as returned in metadata/changedFiles output (e.g. "src/utils/foo.ts").'
    ),
  additions: z
    .array(clampedInt(1, 1_000_000_000))
    .optional()
    .describe('New-file line numbers to keep from the patch.'),
  deletions: z
    .array(clampedInt(1, 1_000_000_000))
    .optional()
    .describe('Original-file line numbers to keep from the patch.'),
});

const PrContentSelectorLocalSchema = z
  .object({
    metadata: z.boolean().optional(),
    body: z.boolean().optional(),
    changedFiles: z.boolean().optional(),
    patches: z
      .object({
        mode: z.enum(['none', 'selected', 'all']).optional(),
        files: z.array(z.string()).optional(),
        ranges: z.array(PrPartialContentMetadataLocalSchema).optional(),
      })
      .optional(),
    comments: z
      .object({
        discussion: z.boolean().optional(),
        reviewInline: z.boolean().optional(),
        includeBots: z.boolean().optional(),
        file: z.string().optional(),
      })
      .optional(),
    reviews: z.boolean().optional(),
    commits: z
      .object({
        list: z.boolean().optional(),
        includeFiles: z.boolean().optional(),
      })
      .optional(),
  })
  .optional();

export const GitHubPullRequestSearchQueryLocalSchema =
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQuerySchema.omit({
      limit: true,
      merged: true,
    }).extend({
      ...optionalMetaFields,
      prNumber: clampedInt(1, 1_000_000_000).optional(),
      state: z.enum(['open', 'closed', 'merged']).optional(),
      matchScope: z.array(z.enum(['title', 'body', 'comments'])).optional(),
      sort: z
        .enum(['created', 'updated', 'best-match', 'comments', 'reactions'])
        .optional(),
      order: z.enum(['asc', 'desc']).optional(),
      archived: z.boolean().optional(),
      page: relaxedPageNumberField.default(1),
      filePage: relaxedPageNumberField.optional(),
      commentPage: relaxedPageNumberField.optional(),
      commitPage: relaxedPageNumberField.optional(),
      itemsPerPage: clampedInt(1, 100).optional(),
      reviewMode: z.enum(['summary', 'full']).optional(),
      content: PrContentSelectorLocalSchema,
      charOffset: clampedInt(0, 100_000_000).optional(),
      charLength: clampedInt(1, 50_000).optional(),
      label: z.union([z.string(), z.array(z.string())]).optional(),
      includeBots: z.boolean().optional(),
      minify: minifyFieldStandard,
    })
  );

export const GitHubPullRequestSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQueryLocalSchema
  );

export const GitHubSearchPullRequestsOutputLocalSchema =
  UpstreamPRsOutput.extend(responseEnvelopeFields);
