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

const queryOverrides = {
  // --- mode discriminator ---
  // "prs" (default): PR search / detail. "commits": file or repo commit history.
  type: z
    .enum(['prs', 'commits'])
    .optional()
    .default('prs')
    .describe(
      'Query mode. "prs" (default): search/read pull requests. "commits": commit history for a file or repo.'
    ),
  // --- commits-mode fields ---
  path: z
    .string()
    .optional()
    .describe(
      'File path or directory prefix for commits mode. Omit for whole-repo history.'
    ),
  branch: z
    .string()
    .optional()
    .describe('Branch name or SHA to start history from (commits mode).'),
  since: z
    .string()
    .optional()
    .describe(
      'ISO 8601 date lower bound — only commits/PRs after this date (commits mode: since; PRs mode: use created field).'
    ),
  until: z
    .string()
    .optional()
    .describe('ISO 8601 date upper bound — only commits before this date (commits mode).'),
  perPage: clampedInt(1, 100)
    .optional()
    .default(30)
    .describe('Commits per page (commits mode only, 1-100).'),
  includeDiff: z
    .boolean()
    .optional()
    .default(false)
    .describe(
      'Include per-commit file diffs (additions, deletions, patch) in commits mode. Increases response size.'
    ),
  // --- PR-mode search fields ---
  milestone: z.string().optional().describe('Filter PRs by milestone title (exact match). Example: "v2.0".'),
  language: z.string().optional().describe('Filter PRs by repository language. Example: "rust", "typescript".'),
  checks: z
    .enum(['pending', 'success', 'failure'])
    .optional()
    .describe('Filter by CI check status: "success" (all checks passed), "failure" (any check failed), "pending" (checks still running).'),
  review: z
    .enum(['none', 'required', 'approved', 'changes_requested'])
    .optional()
    .describe('Filter by review state: "approved" (approved by a reviewer), "changes_requested" (reviewer requested changes), "required" (review is required but not submitted), "none" (no reviews yet).'),
  locked: z.boolean().optional().describe('Filter by conversation lock state. true = locked, false = unlocked.'),
  visibility: z.enum(['public', 'private']).optional().describe('Filter by repository visibility.'),
  'team-mentions': z.string().optional().describe('Filter PRs that mention a team. Format: "org/team-slug".'),
  project: z.string().optional().describe('Filter PRs linked to a project board. Format: "owner/project-number" or just a project number.'),
  // --- PR-mode fields ---
  prNumber: clampedInt(1, 1_000_000_000).optional().describe('Direct PR lookup by number. Required for body/patches/comments/reviews/commits. owner+repo required.'),
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT)
    .optional()
    .default(GITHUB_SEARCH_DEFAULT_LIMIT)
    .describe('Max results for list-mode search. Default 10, max 100.'),
  page: relaxedPageNumberField.default(1).describe('Result page for list-mode search (1-based). Use only when hasMore:true.'),
  filePage: relaxedPageNumberField.optional().describe('Page through changed-files list. Use when contentPagination.changedFiles.hasMore:true.'),
  commentPage: relaxedPageNumberField.optional().describe('Page through PR comments. Use when contentPagination.comments.hasMore:true.'),
  commitPage: relaxedPageNumberField.optional().describe('Page through PR-bound commits. Use when contentPagination.commits.hasMore:true.'),
  itemsPerPage: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE)
    .optional()
    .default(PR_CONTENT_DEFAULT_ITEMS_PER_PAGE)
    .describe('Items per page for files, comments, and commits lists. Default 20.'),
  charOffset: clampedInt(0, 100_000_000).optional().describe('Char window start for body or patch text. Advance to nextQuery.charOffset when contentPagination.body/patches.hasMore:true.'),
  commentBodyOffset: clampedInt(0, 100_000_000).optional().describe('Char window start for comment bodies. Advance to nextQuery.commentBodyOffset when contentPagination.commentBody.hasMore:true.'),
  charLength: clampedInt(1, MAX_CHAR_LENGTH).optional().describe('Char window size for body, patches, and comment bodies. Controls how much text is returned per page.'),
  minify: z.enum(['none', 'standard']).default('standard').describe('"standard" (default): strips blank lines and comment-only lines from patches — smaller responses. "none": raw exact diff text — use when quoting or matching whitespace.'),
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
