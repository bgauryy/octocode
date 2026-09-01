import { z } from 'zod';

import type { ToolSpec } from '../../types/index.js';
import {
  buildObject,
  charLength,
  charOffset,
  defineTool,
  intRange,
  metaFields,
  optionalPageNumber,
  pageNumber,
} from './_toolkit.js';

const commitHistoryContract: ToolSpec = defineTool({
  name: 'ghSearchHistory',
  type: 'Github',
  shortDescription:
    "Walk a GitHub repository's commit history for a path or range.",
  instructions: `Trace when or why code changed; use code tools for current bytes and PR search for review context. History mode filters by path/date/ref/author; compare mode needs base+head. This tool does not search commit messages.
For one commit's changes, compare base:"SHA^" to head:"SHA" with includeDiff. Scope diffs to one commit or a tight window. Use a clone and LSP for symbol identity.`,
  schema: {
    pageSize: 'Commits returned per page (walk with page).',
    owner: 'Repository owner.',
    repo: 'Repository name.',
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
  },
});

export const SearchCommitsQuerySchema = buildObject(
  commitHistoryContract.schema,
  {
    ...metaFields,
    owner: z.string(),
    repo: z.string(),
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
  }
).superRefine((query, ctx) => {
  if ((query.base && !query.head) || (query.head && !query.base)) {
    ctx.addIssue({
      code: 'custom',
      path: [query.base ? 'head' : 'base'],
      message: 'Set base and head together for compare mode.',
    });
  }
});
