import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
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

export const ghSearchCommits: ToolSpec = defineTool({
  name: 'ghSearchCommits',
  type: 'Github',
  shortDescription:
    "Walk a GitHub repository's commit history for a path or range.",
  instructions: `Commit archaeology — when/why a file/area changed — not current code (ghSearchCode/ghGetFileContent) or PRs (ghSearchPullRequests).
No message-keyword filter — only path/since/until/author/committer narrow the walk. Looking for a specific change (e.g. a rename) with no known SHA/path/date? Try ghSearchPullRequests/ghSearchCode for a lead first; otherwise page raw history by eye.
One commit's exact changes: set base:"SHA^", head:"SHA", includeDiff:true — its files+patches in one call; don't page history or fetch files.
owner+repo identify the repo; path scopes a file/dir (trailing / = subtree); since/until bound the window; branch selects a ref; includeDiff is heavy — scope to one SHA or a tight window; itemsPerPage/page walk history. For code identity, clone + local/LSP.`,
  schema: {
    limit:
      'Commits per page — alias of itemsPerPage (preferred when both are set).',
    owner: 'Repository owner.',
    repo: 'Repository name.',
    path: 'File/dir prefix; trailing / scopes the subtree.',
    since:
      'Only commits after this point. ISO date ("2026-01-01") or a relative window ("30d", "2w", "6m", "1y").',
    until:
      'Only commits before this point. ISO date or a relative window (same formats as since).',
    branch: 'Ref to walk; defaults to the default branch.',
    includeDiff:
      "Attach per-commit diffs. Scope it: with base:SHA^/head:SHA it returns one commit's full diff in a single call; heavier on a broad history walk.",
    author: 'Filter to commits by this author (GitHub login or email).',
    committer: 'Filter to commits by this committer (GitHub login or email).',
    base: 'Compare mode: set base+head (branch/tag/sha) to diff two refs (base...head) instead of walking history — returns status, ahead/behind counts, and the commits between them.',
    head: 'Compare mode: the head ref for the base...head comparison.',
    itemsPerPage: 'Commits returned per page (walk with page).',
    filePage:
      'With includeDiff: page through the changed-file list (walk with pagination.nextFilePage).',
    charOffset:
      "With includeDiff: start offset into a file's patch window — copy from patchPagination.nextCharOffset, do not compute.",
    charLength:
      'With includeDiff: characters of patch per window; continue via the returned next offset when hasMore.',
  },
});

export const SearchCommitsQuerySchema = buildObject(ghSearchCommits.schema, {
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
  itemsPerPage: intRange(1, 100).optional().default(30),
  includeDiff: z.boolean().optional().default(false),
  page: pageNumber(),
  filePage: optionalPageNumber(),
  charOffset: charOffset(),
  charLength: charLength(),
});
