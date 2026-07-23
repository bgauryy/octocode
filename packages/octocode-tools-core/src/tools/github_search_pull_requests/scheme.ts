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

const PRChangedFileSchema = z
  .object({
    path: z.string().optional(),
    status: z.string().optional(),
    additions: z.number().optional(),
    deletions: z.number().optional(),
  })
  .passthrough();

// Detail-mode PR/issue row. All fields optional (list mode returns a subset);
// passthrough keeps additive runtime fields valid, but the known surface is
// declared so schema validation can catch drift and clients can read it.
const PRDetailRowSchema = z
  .object({
    number: z.number().optional(),
    title: z.string().optional(),
    url: z.string().optional(),
    state: z.string().optional(),
    author: z.string().optional(),
    targetBranch: z.string().optional(),
    sourceBranch: z.string().optional(),
    sourceSha: z.string().optional(),
    createdAt: z.string().optional(),
    updatedAt: z.string().optional(),
    closedAt: z.string().optional(),
    mergedAt: z.string().optional(),
    changedFilesCount: z.number().optional(),
    additions: z.number().optional(),
    deletions: z.number().optional(),
    changedFiles: z.array(PRChangedFileSchema).optional(),
    // Row-level continuations include non-ToolContinuation shapes (e.g.
    // `target` carries bare owner/repo/prNumber) — keep values open.
    next: z.record(z.string(), z.unknown()).optional(),
    contentPagination: z.record(z.string(), z.unknown()).optional(),
  })
  .passthrough();

// concise:true returns flat "#N title" strings; full mode returns objects.
const ConciseOrDetailRowSchema = z.union([z.string(), PRDetailRowSchema]);

const HistoryCommitSchema = z
  .object({
    sha: z.string().optional(),
    date: z.string().optional(),
    message: z.string().optional(),
    messageHeadline: z.string().optional(),
    url: z.string().optional(),
    author: z
      .object({
        name: z.string().optional(),
        email: z.string().optional(),
        login: z.string().optional(),
      })
      .passthrough()
      .optional(),
  })
  .passthrough();

const HistoryReleaseSchema = z
  .object({
    tagName: z.string().optional(),
    name: z.string().optional(),
    publishedAt: z.string().optional(),
    prerelease: z.boolean().optional(),
    latest: z.boolean().optional(),
    url: z.string().optional(),
  })
  .passthrough();

// Commits-mode pagination omits totalPages (unbounded history walk), so the
// canonical ItemPaginationSchema (which requires it) does not fit here.
const HistoryPaginationSchema = z
  .object({
    currentPage: z.number().optional(),
    totalPages: z.number().optional(),
    perPage: z.number().optional(),
    hasMore: z.boolean().optional(),
    nextPage: z.number().optional(),
  })
  .passthrough();

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
                // Mode identity + scope echoed by commits/releases/issues modes.
                type: z.string().optional(),
                owner: z.string().optional(),
                repo: z.string().optional(),
                path: z.string().optional(),
                total_count: z.number().optional(),
                effectiveQuery: z.string().optional(),
                commits: z.array(HistoryCommitSchema).optional(),
                releases: z.array(HistoryReleaseSchema).optional(),
                latest: z
                  .object({
                    tagName: z.string().optional(),
                    publishedAt: z.string().optional(),
                  })
                  .passthrough()
                  .optional(),
                pagination: HistoryPaginationSchema.optional(),
                // Mode-irrelevant-field notices and other in-band guidance.
                warnings: z.array(z.string()).optional(),
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
