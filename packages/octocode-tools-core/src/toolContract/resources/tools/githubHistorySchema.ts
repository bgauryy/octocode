import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import {
  buildObject,
  charLength,
  charOffset,
  DEFAULT_GITHUB_SEARCH_LIMIT,
  DEFAULT_PR_ITEMS_PER_PAGE,
  defineTool,
  intRange,
  MAX_GITHUB_SEARCH_LIMIT,
  MAX_LINE_NUMBER,
  MAX_PR_ITEMS_PER_PAGE,
  metaFields,
  optionalPageNumber,
  pageNumber,
  StringArray,
} from './_toolkit.js';

// Internal-only ToolSpec: NOT registered as an agent-facing tool (absent from
// resources/tools/index.ts and the /mcp tool registry). It is the prose source
// for GitHubPullRequestSearchQuerySchema — a standalone, combined PR + commit
// history schema (type:"prs" | "commits") exported from /schemas for consumers
// that want the unified validator. The agent-facing ghSearchPullRequests /
// ghSearchIssues / ghSearchCommits / ghListReleases tools were split out and
// each parse through their OWN dedicated schema, NOT this one.
const githubHistorySchemaSpec: ToolSpec = defineTool({
  name: 'githubHistory',
  type: 'Github',
  shortDescription: 'Search and read GitHub pull requests and commit history.',
  instructions: `Use for PR/commit archaeology — not current code (ghSearchCode/ghGetFileContent). type:"prs" lists/reads PRs; type:"commits" walks history.
Param relations: list mode uses filters+sort/page; detail mode needs owner+repo+prNumber and content selectors. patches.mode:"selected" requires files or ranges. Body/patch/comment windows continue via returned charOffset/commentBodyOffset; file/comment/commit lists use their page fields. For code identity, clone and use local/LSP.`,
  schema: {
    type: '"prs" for pull requests; "commits" for commit history.',
    keywordsToSearch: 'PR search terms; ignored in commits mode.',
    match: 'Which text fields keywords search.',
    prNumber: 'Switches to PR detail mode; needs owner+repo.',
    concise: 'PR list triage; ignored with prNumber.',
    path: 'Commit-mode file/dir prefix; trailing / scopes subtree.',
    includeDiff: 'Commit-mode diffs; costly.',
    order: 'asc+created helps archaeology.',
    comments: 'Comment-count range filter (">5"); not comment content.',
    reactions: 'Reaction-count range filter (">10").',
    interactions: 'Combined comment+reaction count range (">20").',
    project: 'Project board: "owner/number".',
    'team-mentions': 'Team mention: "org/team-slug".',
    filePage: 'Changed-files page from contentPagination.',
    commentPage: 'Comments page from contentPagination.',
    commitPage: 'PR commits page from contentPagination.',
    reviewMode: '"full" fetches all PR detail surfaces.',
    content: 'Detail selector for prNumber.',
    'content.body': 'Full PR description; windowed by charOffset.',
    'content.changedFiles': 'File list with status and +/- counts.',
    'content.patches.mode':
      '"none", "selected" (cheapest; needs files or ranges), or "all".',
    'content.patches.ranges': 'Line ranges for selected patch hunks.',
    'content.comments.reviewInline':
      'Inline review comments; in_reply_to_id marks threads.',
    'content.comments.includeBots': 'Include bot/CI comments.',
    'content.reviews': 'Review verdicts per reviewer.',
    'content.commits': 'PR-bound commits selector.',
    matchString: 'Substring filter for body/patch/comment windows.',
    charOffset: 'Body/patch continuation offset from nextQuery.',
    commentBodyOffset: 'Comment-body continuation offset from nextQuery.',
    minify:
      '"standard" compact patches; "none" exact diff; "symbols" not available.',
  },
});

const prose = githubHistorySchemaSpec.schema;

export const GitHubPullRequestSearchQuerySchema = buildObject(prose, {
  ...metaFields,
  // mode + commits-mode fields (documented in the resource, so the scheme
  // accepts them instead of silently stripping them)
  type: z.enum(['prs', 'commits']).default('prs'),
  since: z.string().optional(),
  until: z.string().optional(),
  path: z.string().optional(),
  branch: z.string().optional(),
  includeDiff: z.boolean().optional().default(false),
  // releases mode
  includeAssets: z.boolean().optional(),
  // PR search
  keywordsToSearch: z.array(z.string()).optional(),
  prNumber: intRange(1, MAX_LINE_NUMBER).optional(),
  owner: z.string().optional(),
  repo: z.string().optional(),
  concise: z.boolean().optional(),
  state: z.enum(['open', 'closed', 'merged']).optional(),
  assignee: z.string().optional(),
  author: z.string().optional(),
  committer: z.string().optional(),
  commenter: z.string().optional(),
  mentions: z.string().optional(),
  'review-requested': z.string().optional(),
  'reviewed-by': z.string().optional(),
  label: z.union([z.string(), z.array(z.string())]).optional(),
  checks: z.enum(['success', 'failure', 'pending']).optional(),
  review: z
    .enum(['approved', 'changes_requested', 'required', 'none'])
    .optional(),
  head: z.string().optional(),
  base: z.string().optional(),
  created: z.string().optional(),
  updated: z.string().optional(),
  closed: z.string().optional(),
  'merged-at': z.string().optional(),
  comments: z.string().optional(),
  reactions: z.string().optional(),
  draft: z.boolean().optional(),
  match: z.array(z.enum(['title', 'body', 'comments'])).optional(),
  sort: z
    .enum(['created', 'updated', 'best-match', 'comments', 'reactions'])
    .optional(),
  order: z.enum(['asc', 'desc']).optional(),
  limit: intRange(1, MAX_GITHUB_SEARCH_LIMIT).default(
    DEFAULT_GITHUB_SEARCH_LIMIT
  ),
  page: pageNumber(),
  archived: z.boolean().optional(),
  filePage: optionalPageNumber(),
  commentPage: optionalPageNumber(),
  commitPage: optionalPageNumber(),
  itemsPerPage: intRange(1, MAX_PR_ITEMS_PER_PAGE).default(
    DEFAULT_PR_ITEMS_PER_PAGE
  ),
  reviewMode: z.literal('full').optional(),
  content: buildObject(
    prose,
    {
      metadata: z.boolean().optional(),
      body: z.boolean().optional(),
      changedFiles: z.boolean().optional(),
      patches: buildObject(
        prose,
        {
          mode: z.enum(['none', 'selected', 'all']).default('none'),
          files: StringArray,
          ranges: z
            .array(
              buildObject(
                prose,
                {
                  file: z.string(),
                  additions: z.array(intRange(1, MAX_LINE_NUMBER)).optional(),
                  deletions: z.array(intRange(1, MAX_LINE_NUMBER)).optional(),
                },
                'content.patches.ranges'
              )
            )
            .optional(),
        },
        'content.patches'
      ).optional(),
      comments: buildObject(
        prose,
        {
          discussion: z.boolean().optional(),
          reviewInline: z.boolean().optional(),
          includeBots: z.boolean().default(false),
          file: z.string().optional(),
        },
        'content.comments'
      ).optional(),
      reviews: z.boolean().optional(),
      commits: buildObject(
        prose,
        {
          list: z.boolean().optional(),
          includeFiles: z.boolean().optional(),
        },
        'content.commits'
      ).optional(),
    },
    'content'
  ).optional(),
  matchString: z.string().optional(),
  commentBodyOffset: charOffset(),
  charOffset: charOffset(),
  charLength: charLength(),
  minify: z.enum(['none', 'standard']).default('standard'),
}).superRefine((query, ctx) => {
  const patches = query.content?.patches;
  if (!patches) return;

  const hasFiles = (patches.files?.length ?? 0) > 0;
  const hasRanges = (patches.ranges?.length ?? 0) > 0;
  const hasSelection = hasFiles || hasRanges;

  if (patches.mode === 'selected' && !hasSelection) {
    ctx.addIssue({
      code: 'custom',
      message:
        'content.patches.mode="selected" requires non-empty files or ranges.',
      path: ['content', 'patches', 'files'],
    });
  }

  if (patches.mode !== 'selected' && hasSelection) {
    ctx.addIssue({
      code: 'custom',
      message:
        'content.patches.files and content.patches.ranges require mode="selected".',
      path: ['content', 'patches', 'mode'],
    });
  }
});
