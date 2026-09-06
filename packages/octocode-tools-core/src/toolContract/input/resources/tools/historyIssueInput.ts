import { z } from 'zod';

import {
  buildObject,
  charLength,
  charOffset,
  DEFAULT_GITHUB_SEARCH_LIMIT,
  intRange,
  MAX_GITHUB_SEARCH_LIMIT,
  MAX_LINE_NUMBER,
  metaFields,
  optionalPageNumber,
} from './_toolkit.js';

const prose = {
  keywords: 'Issue search terms.',
  match: 'Keyword fields: title, body, comments.',
  issueNumber: 'Issue detail mode; requires owner+repo.',
  concise: 'Issue list triage; list mode only.',
  state: 'open or closed.',
  assignee: 'Assigned GitHub login.',
  author: 'Issue author login.',
  commenter: 'Commenter login.',
  mentions: 'Mentioned login.',
  archived: 'Include issues from archived repos.',
  comments: 'Comment-count range filter (">5"); not comment content.',
  reactions: 'Reaction-count range filter (">10").',
  created: 'Created date/range, e.g. ">2024-01-01" or "a..b".',
  updated: 'Updated-date filter (same format as created).',
  closed: 'Closed-date filter (same format as created).',
  label: 'Label name(s); multiple are ANDed.',
  sort: 'created, updated, comments, reactions, best-match.',
  order: 'asc/desc; asc+created helps archaeology.',
  commentPage: 'Comments page from contentPagination.',
  content: 'Detail selector for issueNumber.',
  'content.body': 'Include the issue description body.',
  'content.comments': 'Comment-thread selector.',
  'content.comments.discussion': 'Include the comment thread.',
  'content.comments.includeBots': 'Include bot/CI comments.',
  matchString: 'Substring filter for body/comment windows.',
  charOffset:
    'Character offset in the selected issue body or comment bodies; follow next calls.',
  charLength:
    'Characters per selected body window (default 12000); commentPage selects comment items independently.',
  commentBodyOffset: 'Comment-body continuation offset from nextQuery.',
  minify:
    'Search results contain metadata only; this setting does not alter metadata. Exact issue reads do not accept minify.',
  pageSize: 'Issues or detail comments returned per page.',
};

export const SearchIssuesQuerySchema = buildObject(prose, {
  ...metaFields,
  owner: z.string(),
  repo: z.string(),
  keywords: z.array(z.string()).optional(),
  issueNumber: intRange(1, MAX_LINE_NUMBER).optional(),
  concise: z.boolean().optional(),
  state: z.enum(['open', 'closed']).optional(),
  assignee: z.string().optional(),
  author: z.string().optional(),
  commenter: z.string().optional(),
  mentions: z.string().optional(),
  label: z.array(z.string()).optional(),
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
  pageSize: intRange(1, MAX_GITHUB_SEARCH_LIMIT).default(
    DEFAULT_GITHUB_SEARCH_LIMIT
  ),
  page: optionalPageNumber(),
  archived: z.boolean().optional(),
  commentPage: optionalPageNumber(),
  content: buildObject(
    prose,
    {
      body: z.literal(true).optional(),
      comments: buildObject(
        prose,
        {
          discussion: z.literal(true).optional(),
          includeBots: z.literal(true).optional(),
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
}).superRefine((query, ctx) => {
  const detailOnly = [
    'content',
    'commentPage',
    'matchString',
    'commentBodyOffset',
    'charOffset',
    'charLength',
  ] as const;
  const listOnly = [
    'keywords',
    'concise',
    'state',
    'assignee',
    'author',
    'commenter',
    'mentions',
    'label',
    'created',
    'updated',
    'closed',
    'comments',
    'reactions',
    'match',
    'sort',
    'order',
    'page',
    'archived',
  ] as const;

  if (query.issueNumber === undefined) {
    for (const field of detailOnly) {
      if (query[field] !== undefined) {
        ctx.addIssue({
          code: 'custom',
          message: `${field} requires issueNumber detail mode.`,
          path: [field],
        });
      }
    }
  } else {
    if (!query.owner?.trim()) {
      ctx.addIssue({
        code: 'custom',
        message: 'owner is required with issueNumber.',
        path: ['owner'],
      });
    }
    if (!query.repo?.trim()) {
      ctx.addIssue({
        code: 'custom',
        message: 'repo is required with issueNumber.',
        path: ['repo'],
      });
    }
    for (const field of listOnly) {
      if (query[field] !== undefined) {
        ctx.addIssue({
          code: 'custom',
          message: `${field} is only available in list mode.`,
          path: [field],
        });
      }
    }
  }

  if (query.content && Object.keys(query.content).length === 0) {
    ctx.addIssue({
      code: 'custom',
      message: 'content needs at least one selector.',
      path: ['content'],
    });
  }
});
