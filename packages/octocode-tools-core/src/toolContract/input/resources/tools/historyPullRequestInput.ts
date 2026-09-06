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
  StringArray,
} from './_toolkit.js';

const prose = {
  keywords: 'PR search terms.',
  match:
    'PR text fields searched by keywords: title, body, comments. ghSearch operation:"code" instead uses match to select file content or paths.',
  prNumber: 'PR detail mode; requires owner+repo.',
  concise: 'PR list triage; list mode only.',
  state: 'open, closed, or merged.',
  assignee: 'Assigned GitHub login.',
  author: 'PR author login.',
  commenter: 'Commenter login.',
  mentions: 'Mentioned login.',
  'review-requested': 'Requested reviewer login.',
  'reviewed-by': 'Reviewer login.',
  checks: 'success, failure, or pending.',
  review: 'approved, changes_requested, required, or none.',
  comments: 'Comment-count range filter (">5"); not comment content.',
  reactions: 'Reaction-count range filter (">10").',
  created: 'Created date/range, e.g. ">2024-01-01" or "a..b".',
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
  commitPage: 'PR commits page within the current provider batch.',
  reviewPage:
    'Reviews page within the current provider batch; copy next.nextReviewsPage.',
  collectionPages:
    'Provider batch positions from next.*; copy unchanged. Zero marks an exhausted source. Each call fetches at most one page per requested source.',
  content: 'Detail selector for prNumber.',
  'content.body': 'Include the PR description body.',
  'content.changedFiles': 'Include changed files and +/- counts.',
  'content.patches': 'Patch selector for changed files.',
  'content.patches.mode': '"selected" or "all"; selected is cheapest.',
  'content.patches.files':
    'Restrict selected patches to these files (mode:"selected").',
  'content.patches.ranges': 'Line ranges for selected patch hunks.',
  'content.patches.ranges.file': 'File the line range applies to.',
  'content.patches.ranges.additions': 'Added-side line numbers to include.',
  'content.patches.ranges.deletions': 'Deleted-side line numbers to include.',
  'content.comments':
    'Select discussion and/or reviewInline explicitly; omitted surfaces are not fetched.',
  'content.comments.discussion': 'Include the PR conversation timeline.',
  'content.comments.reviewInline': 'Include inline review threads.',
  'content.comments.includeBots': 'Include bot/CI comments.',
  'content.comments.file': 'Filter comments to one file path.',
  'content.reviews': 'Include review verdicts per reviewer.',
  'content.commits': 'PR-bound commits selector.',
  'content.commits.includeFiles': 'Attach per-commit file changes.',
  matchString: 'Substring filter for body/patch/comment windows.',
  charOffset: 'Body/patch continuation offset from nextQuery.',
  commentBodyOffset: 'Comment-body continuation offset from nextQuery.',
  minify:
    '"standard" compacts PR body/comments/reviews and trims diff context; "none" preserves selected text after redaction. Match reads retain source text. No symbols mode.',
  pageSize: 'PRs or detail-list items returned per page.',
};

export const SearchPullRequestsQuerySchema = buildObject(prose, {
  ...metaFields,
  keywords: z.array(z.string()).optional(),
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
  label: z.array(z.string()).optional(),
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
  pageSize: intRange(1, MAX_GITHUB_SEARCH_LIMIT).default(
    DEFAULT_GITHUB_SEARCH_LIMIT
  ),
  page: optionalPageNumber(),
  archived: z.boolean().optional(),
  filePage: optionalPageNumber(),
  commentPage: optionalPageNumber(),
  commitPage: optionalPageNumber(),
  reviewPage: optionalPageNumber(),
  collectionPages: z
    .object({
      changedFiles: z.number().int().min(0).max(30).optional(),
      discussion: z
        .number()
        .int()
        .min(0)
        .max(Number.MAX_SAFE_INTEGER)
        .optional(),
      inline: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
      reviews: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER).optional(),
      commits: z.number().int().min(0).max(5).optional(),
    })
    .strict()
    .optional(),
  content: buildObject(
    prose,
    {
      body: z.literal(true).optional(),
      changedFiles: z.literal(true).optional(),
      patches: buildObject(
        prose,
        {
          mode: z.enum(['selected', 'all']),
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
          discussion: z.literal(true).optional(),
          reviewInline: z.literal(true).optional(),
          includeBots: z.literal(true).optional(),
          file: z.string().optional(),
        },
        'content.comments'
      )
        .refine(
          comments =>
            comments.discussion === true || comments.reviewInline === true,
          {
            message:
              'content.comments needs discussion:true and/or reviewInline:true.',
          }
        )
        .optional(),
      reviews: z.literal(true).optional(),
      commits: buildObject(
        prose,
        {
          includeFiles: z.literal(true).optional(),
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
  const detailOnly = [
    'content',
    'filePage',
    'commentPage',
    'commitPage',
    'reviewPage',
    'collectionPages',
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
    'review-requested',
    'reviewed-by',
    'label',
    'checks',
    'review',
    'head',
    'base',
    'created',
    'updated',
    'closed',
    'merged-at',
    'comments',
    'reactions',
    'draft',
    'match',
    'sort',
    'order',
    'page',
    'archived',
  ] as const;

  if (query.prNumber === undefined) {
    for (const field of detailOnly) {
      if (query[field] !== undefined) {
        ctx.addIssue({
          code: 'custom',
          message: `${field} requires prNumber detail mode.`,
          path: [field],
        });
      }
    }
  } else {
    if (!query.owner?.trim()) {
      ctx.addIssue({
        code: 'custom',
        message: 'owner is required with prNumber.',
        path: ['owner'],
      });
    }
    if (!query.repo?.trim()) {
      ctx.addIssue({
        code: 'custom',
        message: 'repo is required with prNumber.',
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
