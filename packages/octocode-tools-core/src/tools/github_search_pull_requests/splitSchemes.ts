// Focused per-tool schemes for the 4 focused GitHub history tools
// (ghSearchPullRequests / ghSearchIssues / ghSearchCommits / ghListReleases).
// Field set / enums / defaults / prose come from octocode-core
// (SearchPullRequestsQuerySchema / SearchIssuesQuerySchema /
// SearchCommitsQuerySchema / ListReleasesQuerySchema). The runtime only relaxes
// numeric / pagination validation (clamp instead of reject), mirroring
// github_search_pull_requests/scheme.ts. One source of truth; no duplicated prose.
import type { z } from 'zod';

import {
  SearchPullRequestsQuerySchema as CoreSearchPullRequestsQuerySchema,
  SearchIssuesQuerySchema as CoreSearchIssuesQuerySchema,
  SearchCommitsQuerySchema as CoreSearchCommitsQuerySchema,
  ListReleasesQuerySchema as CoreListReleasesQuerySchema,
} from '../../toolContract/schemas.js';
import {
  GITHUB_SEARCH_DEFAULT_LIMIT,
  GITHUB_SEARCH_MAX_LIMIT,
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

// Shared relaxed pagination overrides applied to every split tool.
const paginationOverrides = {
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT)
    .optional()
    .default(GITHUB_SEARCH_DEFAULT_LIMIT),
  page: relaxedPageNumberField.default(1),
  itemsPerPage: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE)
    .optional()
    .default(PR_CONTENT_DEFAULT_ITEMS_PER_PAGE),
} as const;

// PR/issue tools also relax the char/page continuation fields.
const readOverrides = {
  ...paginationOverrides,
  filePage: relaxedPageNumberField.optional(),
  commentPage: relaxedPageNumberField.optional(),
  commitPage: relaxedPageNumberField.optional(),
  charOffset: clampedInt(0, 100_000_000).optional(),
  commentBodyOffset: clampedInt(0, 100_000_000).optional(),
  charLength: clampedInt(1, 100_000).optional(),
} as const;

// ghSearchPullRequests
export const SearchPullRequestsLocalSchema = describeQuerySchema(
  CoreSearchPullRequestsQuerySchema,
  readOverrides
);
export const SearchPullRequestsBulkLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(CoreSearchPullRequestsQuerySchema, readOverrides)
);

// ghSearchIssues
const issuesReadOverrides = {
  ...paginationOverrides,
  commentPage: relaxedPageNumberField.optional(),
  charOffset: clampedInt(0, 100_000_000).optional(),
  commentBodyOffset: clampedInt(0, 100_000_000).optional(),
  charLength: clampedInt(1, 100_000).optional(),
} as const;
export const SearchIssuesLocalSchema = describeQuerySchema(
  CoreSearchIssuesQuerySchema,
  issuesReadOverrides
);
export const SearchIssuesBulkLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(CoreSearchIssuesQuerySchema, issuesReadOverrides)
);

// ghSearchCommits
const commitsOverrides = {
  page: relaxedPageNumberField.default(1),
  // `limit` accepted as an alias for `itemsPerPage` (flow consistency with the
  // discovery-search tools); execution prefers it when explicitly provided.
  limit: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE).optional(),
  // includeDiff pagination: page the changed-file list and window each patch.
  // Execution already reads these (history walk + compare mode); relax the
  // numeric validation so they are accepted instead of stripped.
  filePage: relaxedPageNumberField.optional(),
  charOffset: clampedInt(0, 100_000_000).optional(),
  charLength: clampedInt(1, 100_000).optional(),
} as const;
function requireCommitComparePair(
  query: { base?: string; head?: string },
  ctx: z.RefinementCtx
): void {
  if ((query.base && !query.head) || (query.head && !query.base)) {
    ctx.addIssue({
      code: 'custom',
      path: [query.base ? 'head' : 'base'],
      message: 'compare mode requires base and head together',
    });
  }
}

export const SearchCommitsLocalSchema = describeQuerySchema(
  CoreSearchCommitsQuerySchema,
  commitsOverrides
).superRefine(requireCommitComparePair);
export const SearchCommitsBulkLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(
    CoreSearchCommitsQuerySchema,
    commitsOverrides
  ).superRefine(requireCommitComparePair)
);

// ghListReleases — the core supplies page + itemsPerPage + limit; only numeric
// validation is relaxed here.
const releasesOverrides = {
  page: relaxedPageNumberField.default(1),
  // `limit` accepted as an alias for `itemsPerPage` (flow consistency); execution
  // prefers it when explicitly provided.
  limit: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE).optional(),
} as const;
export const ListReleasesLocalSchema = describeQuerySchema(
  CoreListReleasesQuerySchema,
  releasesOverrides
);
export const ListReleasesBulkLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(CoreListReleasesQuerySchema, releasesOverrides)
);
