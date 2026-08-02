// Focused per-tool schemes for the 4 focused GitHub history tools
// (ghSearchPullRequests / ghSearchIssues / ghSearchCommits / ghListReleases).
// Field set / enums / defaults / prose come from octocode-core
// (SearchPullRequestsQuerySchema / SearchIssuesQuerySchema /
// SearchCommitsQuerySchema / ListReleasesQuerySchema). The runtime only relaxes
// numeric / pagination validation (clamp instead of reject), mirroring
// github_search_pull_requests/scheme.ts. One source of truth; no duplicated prose.
import {
  SearchPullRequestsQuerySchema as CoreSearchPullRequestsQuerySchema,
  SearchIssuesQuerySchema as CoreSearchIssuesQuerySchema,
  SearchCommitsQuerySchema as CoreSearchCommitsQuerySchema,
  ListReleasesQuerySchema as CoreListReleasesQuerySchema,
} from '@octocodeai/octocode-core/schemas';
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
} as const;
export const SearchCommitsLocalSchema = describeQuerySchema(
  CoreSearchCommitsQuerySchema,
  commitsOverrides
);
export const SearchCommitsBulkLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(CoreSearchCommitsQuerySchema, commitsOverrides)
);

// ghListReleases — releases have no discovery-search, so no `limit`; the core
// schema supplies page + itemsPerPage (only `page` needs the relaxed form).
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
