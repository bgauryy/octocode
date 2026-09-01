// Internal branch schemes for GitHub history and release execution.
// Field set / enums / defaults / prose come from octocode-core
// (SearchPullRequestsQuerySchema / SearchIssuesQuerySchema /
// SearchCommitsQuerySchema / ListReleasesQuerySchema). The runtime only relaxes
// numeric / pagination validation (clamp instead of reject), mirroring
// github_search_pull_requests/scheme.ts. One source of truth; no duplicated prose.
import { z } from 'zod';

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
  offsetField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import {
  getRequiredSchemaField,
  getSchemaField,
} from '../../scheme/conditionalSchemas.js';

function copyCanonicalIssues(
  schema: z.ZodTypeAny,
  query: unknown,
  ctx: z.RefinementCtx
): void {
  const result = schema.safeParse(query);
  if (result.success) return;
  for (const issue of result.error.issues) {
    ctx.addIssue({
      code: 'custom',
      message: issue.message,
      path: issue.path,
    });
  }
}

function unwrapObject(schema: z.ZodTypeAny): z.ZodObject<z.ZodRawShape> {
  let current = schema;
  while (current instanceof z.ZodOptional || current instanceof z.ZodDefault) {
    current = current.unwrap() as z.ZodTypeAny;
  }
  if (!(current instanceof z.ZodObject)) {
    throw new TypeError('Expected an object schema');
  }
  return current;
}

function unwrapArray(schema: z.ZodTypeAny): z.ZodArray {
  let current = schema;
  while (current instanceof z.ZodOptional || current instanceof z.ZodDefault) {
    current = current.unwrap() as z.ZodTypeAny;
  }
  if (!(current instanceof z.ZodArray)) {
    throw new TypeError('Expected an array schema');
  }
  return current;
}

function describeLike<T extends z.ZodTypeAny>(
  source: z.ZodTypeAny,
  target: T
): T {
  return source.description
    ? (target.describe(source.description) as T)
    : target;
}

// Shared relaxed pagination overrides applied to every split tool.
const paginationOverrides = {
  pageSize: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT)
    .optional()
    .default(GITHUB_SEARCH_DEFAULT_LIMIT),
  page: clampedInt(1, 1_000).optional(),
} as const;

// PR/issue tools also relax the char/page continuation fields.
const readOverrides = {
  ...paginationOverrides,
  filePage: clampedInt(1, 1_000).optional(),
  commentPage: clampedInt(1, 1_000).optional(),
  commitPage: clampedInt(1, 1_000).optional(),
  charOffset: offsetField.optional(),
  commentBodyOffset: offsetField.optional(),
  charLength: clampedInt(1, 100_000).optional(),
} as const;

// Pull requests
export const SearchPullRequestsLocalSchema = describeQuerySchema(
  CoreSearchPullRequestsQuerySchema,
  readOverrides
).superRefine((query, ctx) =>
  copyCanonicalIssues(CoreSearchPullRequestsQuerySchema, query, ctx)
);

const SearchPullRequestsQueryShape = createQueryShapeSchema(
  CoreSearchPullRequestsQuerySchema,
  readOverrides
);
const prContentField = getSchemaField(
  SearchPullRequestsQueryShape.shape,
  'content'
);
const prContentObject = unwrapObject(prContentField);
const prPatchesField = getSchemaField(prContentObject.shape, 'patches');
const prPatchesObject = unwrapObject(prPatchesField);
const selectedPatchMode = describeLike(
  getSchemaField(prPatchesObject.shape, 'mode'),
  z.literal('selected')
);
const allPatchMode = describeLike(
  getSchemaField(prPatchesObject.shape, 'mode'),
  z.literal('all')
);
const selectedFiles = describeLike(
  getSchemaField(prPatchesObject.shape, 'files'),
  unwrapArray(getSchemaField(prPatchesObject.shape, 'files')).min(1)
);
const selectedRanges = describeLike(
  getSchemaField(prPatchesObject.shape, 'ranges'),
  unwrapArray(getSchemaField(prPatchesObject.shape, 'ranges')).min(1)
);
const selectedFilesPatchSchema = z
  .object({
    mode: selectedPatchMode,
    files: selectedFiles,
    ranges: getSchemaField(prPatchesObject.shape, 'ranges'),
  })
  .strict();
const selectedRangesPatchSchema = z
  .object({
    mode: selectedPatchMode,
    files: getSchemaField(prPatchesObject.shape, 'files'),
    ranges: selectedRanges,
  })
  .strict();
const allPatchesSchema = z.object({ mode: allPatchMode }).strict();
const prContentWithPatchModes = prContentObject.extend({
  patches: describeLike(
    prPatchesField,
    z
      .union([
        selectedFilesPatchSchema,
        selectedRangesPatchSchema,
        allPatchesSchema,
      ])
      .optional()
  ),
});
export const PullRequestListQueryShape = SearchPullRequestsQueryShape.omit({
  prNumber: true,
  filePage: true,
  commentPage: true,
  commitPage: true,
  content: true,
  matchString: true,
  commentBodyOffset: true,
  charOffset: true,
  charLength: true,
});
const prListSchema = PullRequestListQueryShape.superRefine((query, ctx) =>
  copyCanonicalIssues(CoreSearchPullRequestsQuerySchema, query, ctx)
);
export const PullRequestDetailQueryShape = SearchPullRequestsQueryShape.omit({
  keywords: true,
  concise: true,
  state: true,
  assignee: true,
  author: true,
  commenter: true,
  mentions: true,
  'review-requested': true,
  'reviewed-by': true,
  label: true,
  checks: true,
  review: true,
  head: true,
  base: true,
  created: true,
  updated: true,
  closed: true,
  'merged-at': true,
  comments: true,
  reactions: true,
  draft: true,
  match: true,
  sort: true,
  order: true,
  page: true,
  archived: true,
}).extend({
  owner: getRequiredSchemaField(SearchPullRequestsQueryShape.shape, 'owner'),
  repo: getRequiredSchemaField(SearchPullRequestsQueryShape.shape, 'repo'),
  prNumber: getRequiredSchemaField(
    SearchPullRequestsQueryShape.shape,
    'prNumber'
  ),
  content: describeLike(prContentField, prContentWithPatchModes.optional()),
});
const prDetailSchema = PullRequestDetailQueryShape.superRefine((query, ctx) =>
  copyCanonicalIssues(CoreSearchPullRequestsQuerySchema, query, ctx)
);
export const SearchPullRequestsBulkLocalSchema = createRelaxedBulkQuerySchema(
  z.union([prListSchema, prDetailSchema])
);

// Issues
const issuesReadOverrides = {
  ...paginationOverrides,
  commentPage: clampedInt(1, 1_000).optional(),
  charOffset: offsetField.optional(),
  commentBodyOffset: offsetField.optional(),
  charLength: clampedInt(1, 100_000).optional(),
} as const;
export const SearchIssuesLocalSchema = describeQuerySchema(
  CoreSearchIssuesQuerySchema,
  issuesReadOverrides
).superRefine((query, ctx) =>
  copyCanonicalIssues(CoreSearchIssuesQuerySchema, query, ctx)
);

const SearchIssuesQueryShape = createQueryShapeSchema(
  CoreSearchIssuesQuerySchema,
  issuesReadOverrides
);
export const IssueListQueryShape = SearchIssuesQueryShape.omit({
  issueNumber: true,
  commentPage: true,
  content: true,
  matchString: true,
  commentBodyOffset: true,
  charOffset: true,
  charLength: true,
});
const issueListSchema = IssueListQueryShape.superRefine((query, ctx) =>
  copyCanonicalIssues(CoreSearchIssuesQuerySchema, query, ctx)
);
export const IssueDetailQueryShape = SearchIssuesQueryShape.omit({
  keywords: true,
  concise: true,
  state: true,
  assignee: true,
  author: true,
  commenter: true,
  mentions: true,
  label: true,
  created: true,
  updated: true,
  closed: true,
  comments: true,
  reactions: true,
  match: true,
  sort: true,
  order: true,
  page: true,
  archived: true,
}).extend({
  issueNumber: getRequiredSchemaField(
    SearchIssuesQueryShape.shape,
    'issueNumber'
  ),
});
const issueDetailSchema = IssueDetailQueryShape.superRefine((query, ctx) =>
  copyCanonicalIssues(CoreSearchIssuesQuerySchema, query, ctx)
);
export const SearchIssuesBulkLocalSchema = createRelaxedBulkQuerySchema(
  z.union([issueListSchema, issueDetailSchema])
);

// Commits
const commitsOverrides = {
  page: relaxedPageNumberField.default(1),
  pageSize: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE)
    .optional()
    .default(GITHUB_SEARCH_DEFAULT_LIMIT),
  // includeDiff pagination: page the changed-file list and window each patch.
  // Execution already reads these (history walk + compare mode); relax the
  // numeric validation so they are accepted instead of stripped.
  filePage: relaxedPageNumberField.optional(),
  charOffset: offsetField.optional(),
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
const SearchCommitsQueryShape = createQueryShapeSchema(
  CoreSearchCommitsQuerySchema,
  commitsOverrides
);
export const CommitHistoryQueryShape = SearchCommitsQueryShape.omit({
  base: true,
  head: true,
});
const commitHistorySchema = CommitHistoryQueryShape;
export const CommitCompareQueryShape = SearchCommitsQueryShape.omit({
  branch: true,
  since: true,
  until: true,
  author: true,
  committer: true,
  page: true,
}).extend({
  base: getRequiredSchemaField(SearchCommitsQueryShape.shape, 'base'),
  head: getRequiredSchemaField(SearchCommitsQueryShape.shape, 'head'),
});
const commitCompareSchema = CommitCompareQueryShape;
export const SearchCommitsBulkLocalSchema = createRelaxedBulkQuerySchema(
  z.union([commitHistorySchema, commitCompareSchema])
);

// ghListReleases — relax numeric validation while preserving its page default.
const releasesOverrides = {
  page: relaxedPageNumberField.default(1),
  pageSize: clampedInt(1, PR_CONTENT_MAX_ITEMS_PER_PAGE)
    .optional()
    .default(PR_CONTENT_DEFAULT_ITEMS_PER_PAGE),
} as const;
export const ListReleasesLocalSchema = describeQuerySchema(
  CoreListReleasesQuerySchema,
  releasesOverrides
);
export const ListReleasesBulkLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(CoreListReleasesQuerySchema, releasesOverrides)
);
