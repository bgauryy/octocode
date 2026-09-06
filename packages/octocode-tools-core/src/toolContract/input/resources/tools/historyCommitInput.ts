import { z } from 'zod';

import {
  buildObject,
  charLength,
  charOffset,
  intRange,
  metaFields,
  optionalPageNumber,
  pageNumber,
} from './_toolkit.js';

const prose = {
  pageSize: 'Commits returned per page (walk with page).',
  owner: 'Repository owner.',
  repo: 'Repository name.',
  keywords:
    'Commit-message words or phrases on the default branch; cannot combine with path, branch, compare, or diffs.',
  path: 'File/dir prefix; trailing / scopes the subtree.',
  since: 'Lower date bound: ISO date or relative window such as "30d".',
  until: 'Upper date bound; same formats as since.',
  branch: 'Ref to walk; defaults to the default branch.',
  includeDiff: 'Attach diffs; prefer one commit or a tight window.',
  author: 'Author login or email.',
  committer: 'Committer login or email.',
  base: 'Compare-mode base ref; set with head.',
  head: 'Compare-mode head ref; set with base.',
  filePage: 'Changed-file page from pagination.nextFilePage.',
  charOffset: 'Patch offset from patchPagination.nextCharOffset.',
  charLength: 'Patch-window length.',
};

export const SearchCommitsQuerySchema = buildObject(prose, {
  ...metaFields,
  owner: z.string(),
  repo: z.string(),
  keywords: z.array(z.string().trim().min(1)).min(1).optional(),
  path: z.string().optional(),
  since: z.string().optional(),
  until: z.string().optional(),
  branch: z.string().optional(),
  author: z.string().optional(),
  committer: z.string().optional(),
  base: z.string().optional(),
  head: z.string().optional(),
  pageSize: intRange(1, 100).optional().default(30),
  includeDiff: z.boolean().optional().default(false),
  page: pageNumber(),
  filePage: optionalPageNumber(),
  charOffset: charOffset(),
  charLength: charLength(),
}).superRefine((query, ctx) => {
  validateCommitKeywordScope(query, ctx);
  if ((query.base && !query.head) || (query.head && !query.base)) {
    ctx.addIssue({
      code: 'custom',
      path: [query.base ? 'head' : 'base'],
      message: 'Set base and head together for compare mode.',
    });
  }
});

export function validateCommitKeywordScope(
  query: {
    keywords?: string[];
    path?: string;
    branch?: string;
    base?: string;
    head?: string;
    includeDiff?: boolean;
  },
  ctx: z.RefinementCtx
): void {
  if (!query.keywords?.length) return;
  for (const field of [
    'path',
    'branch',
    'base',
    'head',
    'includeDiff',
  ] as const) {
    if (query[field] !== undefined && query[field] !== false) {
      ctx.addIssue({
        code: 'custom',
        path: [field],
        message: `Commit-message keywords cannot be combined with ${field}; search covers the default branch. Use history without keywords for path/ref filters and ghGetHistoryItem for diffs.`,
      });
    }
  }
}
