import { z } from 'zod';
import { GitHubPullRequestSearchQuerySchema as CoreGitHubPullRequestSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
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

import type { ToolContinuation } from '../../scheme/pagination.js';
import type { ResponsePaginationInfo } from '../../types/toolOutput.js';

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

// ---------------------------------------------------------------------------
// Output TYPES — describes what ghSearchPullRequests returns. No zod: the MCP
// server registers no outputSchema. Index signatures mirror the original
// .passthrough() (upstream + local) for additive runtime fields.
// ---------------------------------------------------------------------------

export interface PRChangedFile {
  path?: string;
  status?: string;
  additions?: number;
  deletions?: number;
  [key: string]: unknown;
}

// Detail-mode PR/issue row. All fields optional (list mode returns a subset);
// the index signature keeps additive runtime fields valid.
export interface PRDetailRow {
  number?: number;
  title?: string;
  url?: string;
  state?: string;
  author?: string;
  targetBranch?: string;
  sourceBranch?: string;
  sourceSha?: string;
  createdAt?: string;
  updatedAt?: string;
  closedAt?: string;
  mergedAt?: string;
  changedFilesCount?: number;
  additions?: number;
  deletions?: number;
  changedFiles?: PRChangedFile[];
  // Row-level continuations include non-ToolContinuation shapes (e.g.
  // `target` carries bare owner/repo/prNumber) — keep values open.
  next?: Record<string, unknown>;
  contentPagination?: Record<string, unknown>;
  [key: string]: unknown;
}

// concise:true returns flat "#N title" strings; full mode returns objects.
export type ConciseOrDetailRow = string | PRDetailRow;

export interface HistoryCommit {
  sha?: string;
  date?: string;
  message?: string;
  messageHeadline?: string;
  url?: string;
  author?: {
    name?: string;
    email?: string;
    login?: string;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

export interface HistoryRelease {
  tagName?: string;
  name?: string;
  publishedAt?: string;
  prerelease?: boolean;
  latest?: boolean;
  [key: string]: unknown;
}

// Commits-mode pagination omits totalPages (unbounded history walk), so the
// canonical ItemPagination (which requires it) does not fit here.
export interface HistoryPagination {
  currentPage?: number;
  totalPages?: number;
  perPage?: number;
  hasMore?: boolean;
  nextPage?: number;
  [key: string]: unknown;
}

export interface PullRequestsResultData {
  pullRequests?: ConciseOrDetailRow[];
  // type:"issues" reuses this tool; same concise/object shapes.
  issues?: ConciseOrDetailRow[];
  // Mode identity + scope echoed by commits/releases/issues modes.
  type?: string;
  owner?: string;
  repo?: string;
  path?: string;
  totalCount?: number;
  effectiveQuery?: string;
  commits?: HistoryCommit[];
  releases?: HistoryRelease[];
  latest?: {
    tagName?: string;
    publishedAt?: string;
    [key: string]: unknown;
  };
  pagination?: HistoryPagination;
  // Mode-irrelevant-field notices and other in-band guidance.
  // Continuations (readIssue / searchCode / …).
  next?: Record<string, ToolContinuation>;
  [key: string]: unknown;
}

export interface GitHubSearchPullRequestsOutputLocal {
  base?: string;
  shared?: Record<string, string | number | boolean>;
  responsePagination?: ResponsePaginationInfo;
  next?: Record<string, ToolContinuation>;
  results?: Array<{
    id?: string;
    status?: string;
    data?: PullRequestsResultData;
    [key: string]: unknown;
  }>;
  // Upstream output schema is passthrough — allow additive top-level fields.
  [key: string]: unknown;
}
