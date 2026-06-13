import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { GitHubSearchPullRequestsOutputSchema as UpstreamPRsOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  GITHUB_SEARCH_MAX_LIMIT,
  MAX_CHAR_LENGTH,
  PR_CONTENT_MAX_ITEMS_PER_PAGE,
} from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  minifyFieldStandard,
  relaxedPageNumberField,
} from '../../scheme/localSchemaOverlay.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS]
    ?.schema,
} as Record<string, string>;

const PrPartialContentMetadataSchema = z.object({
  file: z.string(),
  additions: z.array(clampedInt(1, 1_000_000_000)).optional(),
  deletions: z.array(clampedInt(1, 1_000_000_000)).optional(),
});

const PrContentSelectorSchema = z
  .object({
    body: z.boolean().optional().describe(QUERY_DESCRIPTIONS['content.body']!),
    changedFiles: z
      .boolean()
      .optional()
      .describe(QUERY_DESCRIPTIONS['content.changedFiles']!),
    patches: z
      .object({
        mode: z
          .enum(['none', 'selected', 'all'])
          .optional()
          .describe(QUERY_DESCRIPTIONS['content.patches.mode']!),
        files: z
          .array(z.string())
          .optional()
          .describe(QUERY_DESCRIPTIONS['content.patches.files']!),
        ranges: z
          .array(PrPartialContentMetadataSchema)
          .optional()
          .describe(QUERY_DESCRIPTIONS['content.patches.ranges']!),
      })
      .optional()
      .describe(QUERY_DESCRIPTIONS['content.patches']!),
    comments: z
      .object({
        discussion: z
          .boolean()
          .optional()
          .describe(QUERY_DESCRIPTIONS['content.comments.discussion']!),
        reviewInline: z
          .boolean()
          .optional()
          .describe(QUERY_DESCRIPTIONS['content.comments.reviewInline']!),
        includeBots: z
          .boolean()
          .optional()
          .describe(QUERY_DESCRIPTIONS['content.comments.includeBots']!),
        file: z
          .string()
          .optional()
          .describe(QUERY_DESCRIPTIONS['content.comments.file']!),
      })
      .optional()
      .describe(QUERY_DESCRIPTIONS['content.comments']!),
    reviews: z
      .boolean()
      .optional()
      .describe(QUERY_DESCRIPTIONS['content.reviews']!),
    commits: z
      .object({
        list: z
          .boolean()
          .optional()
          .describe(QUERY_DESCRIPTIONS['content.commits.list']!),
        includeFiles: z
          .boolean()
          .optional()
          .describe(QUERY_DESCRIPTIONS['content.commits.includeFiles']!),
      })
      .optional()
      .describe(QUERY_DESCRIPTIONS['content.commits']!),
  })
  .optional()
  .describe(QUERY_DESCRIPTIONS.content!);

const GitHubPullRequestSearchQuerySchema = z.object({
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
  query: z.string().optional().describe(QUERY_DESCRIPTIONS.query!),
  prNumber: clampedInt(1, 1_000_000_000)
    .optional()
    .describe(QUERY_DESCRIPTIONS.prNumber!),
  owner: z.string().optional().describe(QUERY_DESCRIPTIONS.owner!),
  repo: z.string().optional().describe(QUERY_DESCRIPTIONS.repo!),
  verbose: z.boolean().optional().describe(QUERY_DESCRIPTIONS.verbose!),
  state: z
    .enum(['open', 'closed', 'merged'])
    .optional()
    .describe(QUERY_DESCRIPTIONS.state!),
  assignee: z.string().optional().describe(QUERY_DESCRIPTIONS.assignee!),
  author: z.string().optional().describe(QUERY_DESCRIPTIONS.author!),
  commenter: z.string().optional().describe(QUERY_DESCRIPTIONS.commenter!),
  involves: z.string().optional().describe(QUERY_DESCRIPTIONS.involves!),
  mentions: z.string().optional().describe(QUERY_DESCRIPTIONS.mentions!),
  'review-requested': z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS['review-requested']!),
  'reviewed-by': z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS['reviewed-by']!),
  label: z
    .union([z.string(), z.array(z.string())])
    .optional()
    .describe(QUERY_DESCRIPTIONS.label!),
  'no-label': z.boolean().optional().describe(QUERY_DESCRIPTIONS['no-label']!),
  'no-milestone': z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS['no-milestone']!),
  'no-project': z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS['no-project']!),
  'no-assignee': z
    .boolean()
    .optional()
    .describe(QUERY_DESCRIPTIONS['no-assignee']!),
  head: z.string().optional().describe(QUERY_DESCRIPTIONS.head!),
  base: z.string().optional().describe(QUERY_DESCRIPTIONS.base!),
  created: z.string().optional().describe(QUERY_DESCRIPTIONS.created!),
  updated: z.string().optional().describe(QUERY_DESCRIPTIONS.updated!),
  closed: z.string().optional().describe(QUERY_DESCRIPTIONS.closed!),
  'merged-at': z.string().optional().describe(QUERY_DESCRIPTIONS['merged-at']!),
  comments: z.string().optional().describe(QUERY_DESCRIPTIONS.comments!),
  reactions: z.string().optional().describe(QUERY_DESCRIPTIONS.reactions!),
  interactions: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.interactions!),
  draft: z.boolean().optional().describe(QUERY_DESCRIPTIONS.draft!),
  match: z
    .array(z.enum(['title', 'body', 'comments']))
    .optional()
    .describe(QUERY_DESCRIPTIONS.match!),
  sort: z
    .enum(['created', 'updated', 'best-match', 'comments', 'reactions'])
    .optional()
    .describe(QUERY_DESCRIPTIONS.sort!),
  order: z.enum(['asc', 'desc']).optional().describe(QUERY_DESCRIPTIONS.order!),
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT)
    .optional()
    .describe(QUERY_DESCRIPTIONS.limit!),
  page: relaxedPageNumberField.default(1).describe(QUERY_DESCRIPTIONS.page!),
  archived: z.boolean().optional().describe(QUERY_DESCRIPTIONS.archived!),
  filePage: relaxedPageNumberField
    .optional()
    .describe(QUERY_DESCRIPTIONS.filePage!),
  commentPage: relaxedPageNumberField
    .optional()
    .describe(QUERY_DESCRIPTIONS.commentPage!),
  commitPage: relaxedPageNumberField
    .optional()
    .describe(QUERY_DESCRIPTIONS.commitPage!),
  itemsPerPage: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE)
    .optional()
    .describe(QUERY_DESCRIPTIONS.itemsPerPage!),
  reviewMode: z
    .literal('full')
    .optional()
    .describe(QUERY_DESCRIPTIONS.reviewMode!),
  content: PrContentSelectorSchema.describe(QUERY_DESCRIPTIONS.content!),
  matchString: z.string().optional().describe(QUERY_DESCRIPTIONS.matchString!),
  charOffset: clampedInt(0, 100_000_000)
    .optional()
    .describe(QUERY_DESCRIPTIONS.charOffset!),
  charLength: clampedInt(1, MAX_CHAR_LENGTH)
    .optional()
    .describe(QUERY_DESCRIPTIONS.charLength!),
  minify: minifyFieldStandard.describe(QUERY_DESCRIPTIONS.minify!),
});

export const GitHubPullRequestSearchQueryLocalSchema =
  GitHubPullRequestSearchQuerySchema;

export const GitHubPullRequestSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQuerySchema
  );

export const GitHubSearchPullRequestsOutputLocalSchema =
  UpstreamPRsOutput.extend(responseEnvelopeFields);
