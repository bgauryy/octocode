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
import { ToolContinuationSchema } from '../../scheme/pagination.js';

// Field set, enums, defaults and descriptions all come from octocode-core
// (GitHubPullRequestSearchQuerySchema). The runtime only overrides the numeric /
// pagination fields to apply *relaxed* validation (clamp instead of reject) — and
// omits .describe() so the description is inherited from core (see copyDescription
// in ../../scheme/coreSchemas.ts). One source of truth; no duplicated prose.
const queryOverrides = {
  // Extends core's enum (prs|commits) with 'releases' and 'issues'. Carries its
  // own description until core ships the new values.
  type: z
    .enum(['prs', 'commits', 'releases', 'issues'])
    .optional()
    .describe(
      'Research mode: "prs" (default) searches pull requests; "commits" walks commit history for a repo or path; "releases" lists the repository releases (tagName, publishedAt, prerelease flag) and surfaces the latest stable release; "issues" searches or reads GitHub issues (body/discussion comments — not PRs).'
    ),
  perPage: clampedInt(1, 100).optional().default(30),
  prNumber: clampedInt(1, 1_000_000_000).optional(),
  issueNumber: clampedInt(1, 1_000_000_000)
    .optional()
    .describe(
      'Issue number for type:"issues" detail mode — reads that specific issue (body/discussion comments). Requires owner+repo. Falls back to prNumber if omitted.'
    ),
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT)
    .optional()
    .default(GITHUB_SEARCH_DEFAULT_LIMIT),
  // `match` here selects WHICH text fields keywords are matched against — a
  // different concept from ghSearchCode's `match` (file contents vs paths).
  // Don't carry intuition across tools.
  match: z
    .array(z.enum(['title', 'body', 'comments']))
    .optional()
    .describe(
      'Fields to match keywords against: "title", "body", "comments". Default searches all three. Use ["title"] for the most precise and fastest match. (Unlike ghSearchCode, where `match` instead selects file-contents vs file-paths — a different concept sharing this name.)'
    ),
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

// concise:true returns flat "#N title" strings; full mode returns objects.
const ConciseOrDetailRowSchema = z.union([
  z.string(),
  z.object({}).passthrough(),
]);

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
                pull_requests: z.array(ConciseOrDetailRowSchema).optional(),
                // type:"issues" reuses this tool; same concise/object shapes.
                issues: z.array(ConciseOrDetailRowSchema).optional(),
                // Continuations (readIssue / searchCode / …) — declare so MCP
                // JSON Schema does not reject under additionalProperties:false
                // when upstream/passthrough compilation is strict.
                next: z.record(z.string(), ToolContinuationSchema).optional(),
              })
              .passthrough()
              .optional(),
          })
          .passthrough()
      )
      .optional(),
    ...responseEnvelopeFields,
  });
