import { z } from 'zod';
import { GitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubSearchPullRequestsOutputSchema as UpstreamPRsOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  DEFAULT_PAGE_SIZE,
  describeField,
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
  .optional()
  .describe(
    'Explicit PR content selector. Use for smart, paginated PR review: request body, changedFiles, selected/all patches, comments, reviews, or commits by need. Broad searches still return metadata only; re-call with prNumber for content.'
  );

export const GitHubPullRequestSearchQueryLocalSchema =
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQuerySchema.omit({
      limit: true,
      type: true,
      withComments: true,
      withCommits: true,
      partialContentMetadata: true,
      merged: true,
    }).extend({
      ...optionalMetaFields,
      query: describeField(
        GitHubPullRequestSearchQuerySchema.shape.query,
        'Free-text PR search query. For PR archaeology, start with title keywords and matchScope=["title"].'
      ),
      prNumber: clampedInt(1, 1_000_000_000)
        .optional()
        .describe(
          'Direct PR number lookup. Cheapest and most precise when known.'
        ),
      owner: describeField(
        GitHubPullRequestSearchQuerySchema.shape.owner,
        'GitHub repository owner or organization.'
      ),
      repo: describeField(
        GitHubPullRequestSearchQuerySchema.shape.repo,
        'GitHub repository name without the owner.'
      ),
      state: z
        .enum(['open', 'closed', 'merged'])
        .optional()
        .describe(
          'PR state filter. "merged" emits is:merged in GitHub search (merged PRs only). "closed" returns all closed PRs (merged + unmerged). "open" for active PRs. Omit to search across all states.'
        ),
      matchScope: z
        .array(z.enum(['title', 'body', 'comments']))
        .optional()
        .describe(
          'Text fields searched by query. Use ["title"] first for PR archaeology; comments are slower/noisier.'
        ),
      sort: z
        .enum(['created', 'updated', 'best-match', 'comments', 'reactions'])
        .optional(),
      order: z
        .enum(['asc', 'desc'])
        .optional()
        .describe(
          'Sort direction: "asc" for ascending, "desc" (default) for descending.'
        ),
      archived: z
        .boolean()
        .optional()
        .describe(
          'Include PRs from archived repositories. Default (omitted/false) excludes them.'
        ),
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `PR search result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} pull requests.`
        ),
      filePage: relaxedPageNumberField
        .optional()
        .describe(
          'Changed-file / patch file page for direct prNumber content requests.'
        ),
      commentPage: relaxedPageNumberField
        .optional()
        .describe('Comment page for direct prNumber content requests.'),
      commitPage: relaxedPageNumberField
        .optional()
        .describe('Commit page for direct prNumber commit requests.'),
      itemsPerPage: clampedInt(1, 100)
        .optional()
        .describe('Items per page for changed files, comments, or commits.'),
      reviewMode: z
        .enum(['summary', 'full'])
        .optional()
        .describe(
          'Convenience mode for PR review. "full" requests metadata, body, changed files, patches, comments, reviews, and commits as a paginated review packet.'
        ),
      content: PrContentSelectorLocalSchema,
      charOffset: clampedInt(0, 100_000_000)
        .optional()
        .describe(
          'Character offset for paginated PR bodies/comment bodies in search results. Use the returned nextCharOffset to continue without losing data.'
        ),
      charLength: clampedInt(1, 50_000)
        .optional()
        .describe(
          'Character page size for PR bodies/comment bodies in search results. Broad PR searches default to compact body/comment windows; direct prNumber lookups return full details.'
        ),
      label: z
        .union([z.string(), z.array(z.string())])
        .optional()
        .describe(
          'Filter PRs by GitHub label name(s). String for one label, array for multiple. Labels with spaces are quoted automatically — e.g. label: "Pages Router" or label: ["bug", "enhancement"]. Use the exact label name as it appears on GitHub.'
        ),
      includeBots: z
        .boolean()
        .optional()
        .describe(
          'Include bot-authored comments (e.g. CI bots, Vercel, CodeRabbit). Default false — bot comments are filtered to reduce noise.'
        ),
      minify: z
        .boolean()
        .optional()
        .describe(
          'Control minification of PR content (patches, body, comments). Default true — comments and redundant whitespace are stripped from code patches for token efficiency. Pass false to get raw unprocessed diffs.'
        ),
    })
  );

export const GitHubPullRequestSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQueryLocalSchema
  );

export const GitHubSearchPullRequestsOutputLocalSchema =
  UpstreamPRsOutput.extend(responseEnvelopeFields);
