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

export const ghSearchPullRequests: ToolSpec = defineTool({
  name: 'ghSearchPullRequests',
  type: 'Github',
  shortDescription:
    "Search GitHub pull requests, or read one PR's files, diffs, and reviews.",
  instructions: `Use for PR archaeology — how/why a change landed, review discussion, diffs — not current code (ghSearchCode/ghGetFileContent), commits (ghSearchCommits), or issues (ghSearchIssues).
Comments, review replies, and requested changes are claims — confirm what actually landed against content.patches (mode:"selected") or the file at the merge SHA, not the conversation.
Param relations: list mode uses keywordsToSearch+filters+sort/page; detail mode needs owner+repo+prNumber and content selectors. content.patches.mode:"selected" requires files or ranges. Body/patch/comment windows continue via returned charOffset/commentBodyOffset; file/comment/commit lists use their page fields. For code identity, clone and use local/LSP.`,
  schema: {
    keywordsToSearch: 'PR search terms.',
    match: 'Keyword fields: title, body, comments.',
    prNumber: 'Switches to PR detail mode; needs owner+repo.',
    concise: 'PR list triage; ignored with prNumber.',
    state: 'open, closed, or merged.',
    assignee: 'Filter by assigned GitHub login.',
    author: 'Filter by PR author login.',
    commenter: 'Filter by a login that commented.',
    mentions: 'Filter by a mentioned login.',
    'review-requested': 'PRs where this login is a requested reviewer.',
    'reviewed-by': 'PRs this login has reviewed.',
    checks: 'success, failure, or pending.',
    review: 'approved, changes_requested, required, or none.',
    comments: 'Comment-count range filter (">5"); not comment content.',
    reactions: 'Reaction-count range filter (">10").',
    created:
      'Created-date filter, e.g. ">2024-01-01" or "2024-01-01..2024-06-30".',
    updated: 'Updated-date filter (same format as created).',
    closed: 'Closed-date filter (same format as created).',
    'merged-at': 'Merged-date filter (same format as created).',
    head: 'Head branch name filter.',
    base: 'Base branch name filter.',
    label: 'Label name(s); multiple are ANDed.',
    draft: 'true = only drafts; false = exclude drafts.',
    archived: 'Include PRs from archived repos.',
    sort: 'created, updated, comments, reactions, best-match.',
    order: 'asc/desc; asc+created helps archaeology.',
    filePage: 'Changed-files page from contentPagination.',
    commentPage: 'Comments page from contentPagination.',
    commitPage: 'PR commits page from contentPagination.',
    reviewMode: '"full" fetches all PR detail surfaces.',
    content: 'Detail selector for prNumber.',
    'content.metadata': 'Include PR metadata (state, refs, counts).',
    'content.body': 'Include the PR description body.',
    'content.changedFiles': 'Include the changed-file list with +/- counts.',
    'content.patches.mode':
      '"none", "selected", or "all"; selected is cheapest.',
    'content.patches.files':
      'Restrict selected patches to these files (mode:"selected").',
    'content.patches.ranges': 'Line ranges for selected patch hunks.',
    'content.patches.ranges.file': 'File the line range applies to.',
    'content.patches.ranges.additions': 'Added-side line numbers to include.',
    'content.patches.ranges.deletions': 'Deleted-side line numbers to include.',
    'content.comments.discussion': 'Include the PR conversation timeline.',
    'content.comments.reviewInline':
      'Include inline review comments (threads via in_reply_to_id).',
    'content.comments.includeBots': 'Include bot/CI comments.',
    'content.comments.file': 'Filter comments to one file path.',
    'content.reviews': 'Include review verdicts per reviewer.',
    'content.commits': 'PR-bound commits selector.',
    'content.commits.list': "Include the PR's commit list.",
    'content.commits.includeFiles': 'Attach per-commit file changes.',
    matchString: 'Substring filter for body/patch/comment windows.',
    charOffset: 'Body/patch continuation offset from nextQuery.',
    commentBodyOffset: 'Comment-body continuation offset from nextQuery.',
    minify:
      '"standard" compact patches; "none" exact diff; "symbols" not available.',
    limit: 'Max PRs in the search list (list-result cap).',
    itemsPerPage:
      "Items per page when reading one PR's comments/files/commits (with page).",
  },
});

const prose = ghSearchPullRequests.schema;

export const SearchPullRequestsQuerySchema = buildObject(prose, {
  ...metaFields,
  keywordsToSearch: z.array(z.string()).optional(),
  prNumber: intRange(1, MAX_LINE_NUMBER).optional(),
  owner: z.string().optional(),
  repo: z.string().optional(),
  concise: z.boolean().optional(),
  state: z.enum(['open', 'closed', 'merged']).optional(),
  assignee: z.string().optional(),
  author: z.string().optional(),
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
