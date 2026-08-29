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
} from './_toolkit.js';

export const ghSearchIssues: ToolSpec = defineTool({
  name: 'ghSearchIssues',
  type: 'Github',
  shortDescription:
    "Search GitHub issues, or read one issue's body and comments.",
  instructions: `Issue triage/archaeology — bug reports, feature threads — not PRs (ghSearchPullRequests) or code (ghSearchCode).
List mode: keywordsToSearch+filters+sort/page. Detail mode: owner+repo+issueNumber + content selectors; body/comment windows continue via charOffset/commentBodyOffset, comments page via commentPage.
totalCount appears only when one page is the full set (the endpoint also returns PRs, filtered out — so per-page counts aren't totals); an empty page with hasMore:true was PR-only — follow nextPage.`,
  schema: {
    keywordsToSearch: 'Issue search terms.',
    match: 'Keyword fields: title, body, comments.',
    issueNumber: 'Switches to issue detail mode; needs owner+repo.',
    concise: 'Issue list triage; ignored with issueNumber.',
    state: 'open or closed.',
    assignee: 'Filter by assigned GitHub login.',
    author: 'Filter by issue author login.',
    commenter: 'Filter by a login that commented.',
    mentions: 'Filter by a mentioned login.',
    archived: 'Include issues from archived repos.',
    comments: 'Comment-count range filter (">5"); not comment content.',
    reactions: 'Reaction-count range filter (">10").',
    created:
      'Created-date filter, e.g. ">2024-01-01" or "2024-01-01..2024-06-30".',
    updated: 'Updated-date filter (same format as created).',
    closed: 'Closed-date filter (same format as created).',
    label: 'Label name(s); multiple are ANDed.',
    sort: 'created, updated, comments, reactions, best-match.',
    order: 'asc/desc; asc+created helps archaeology.',
    commentPage: 'Comments page from contentPagination.',
    content: 'Detail selector for issueNumber.',
    'content.metadata': 'Include issue metadata (state, labels, counts).',
    'content.body': 'Include the issue description body.',
    'content.comments.discussion': 'Include the comment thread.',
    'content.comments.includeBots': 'Include bot/CI comments.',
    matchString: 'Substring filter for body/comment windows.',
    charOffset: 'Body continuation offset from nextQuery.',
    commentBodyOffset: 'Comment-body continuation offset from nextQuery.',
    minify:
      '"standard" (default) compacts body/comment text; "none" keeps it exact.',
    limit: 'Max issues in the search list (list-result cap).',
    itemsPerPage:
      'Comments per page when reading one issue (with commentPage).',
  },
});

const prose = ghSearchIssues.schema;

export const SearchIssuesQuerySchema = buildObject(prose, {
  ...metaFields,
  owner: z.string().optional(),
  repo: z.string().optional(),
  keywordsToSearch: z.array(z.string()).optional(),
  issueNumber: intRange(1, MAX_LINE_NUMBER).optional(),
  concise: z.boolean().optional(),
  state: z.enum(['open', 'closed']).optional(),
  assignee: z.string().optional(),
  author: z.string().optional(),
  commenter: z.string().optional(),
  mentions: z.string().optional(),
  label: z.union([z.string(), z.array(z.string())]).optional(),
  created: z.string().optional(),
  updated: z.string().optional(),
  closed: z.string().optional(),
  comments: z.string().optional(),
  reactions: z.string().optional(),
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
  commentPage: optionalPageNumber(),
  itemsPerPage: intRange(1, MAX_PR_ITEMS_PER_PAGE).default(
    DEFAULT_PR_ITEMS_PER_PAGE
  ),
  content: buildObject(
    prose,
    {
      metadata: z.boolean().optional(),
      body: z.boolean().optional(),
      comments: buildObject(
        prose,
        {
          discussion: z.boolean().optional(),
          includeBots: z.boolean().default(false),
        },
        'content.comments'
      ).optional(),
    },
    'content'
  ).optional(),
  matchString: z.string().optional(),
  commentBodyOffset: charOffset(),
  charOffset: charOffset(),
  charLength: charLength(),
  minify: z.enum(['none', 'standard']).default('standard'),
});
