import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import {
  buildObject,
  DEFAULT_GITHUB_SEARCH_LIMIT,
  defineTool,
  intRange,
  MAX_GITHUB_SEARCH_LIMIT,
  metaFields,
  pageNumber,
  StringArray,
} from './_toolkit.js';

export const ghSearchCode: ToolSpec = defineTool({
  name: 'ghSearchCode',
  type: 'Github',
  shortDescription:
    'Search code contents or file paths across GitHub repositories.',
  instructions: `Use for remote code/file discovery. Start match:"path" + concise for filenames; match:"file" only when snippets matter. Skip if you already have the path (ghGetFileContent) or need repo discovery (ghSearchRepos).
One-shot: if a match:"file" snippet already contains the answer (a field value, a symbol name, a state), STOP — that snippet is the evidence; don't chain a structure view or a content read to "confirm" it.
Param relations: owner+repo scopes tightly; path scopes a directory prefix; extension/language/filename narrow candidates; keywords are ANDed. Follow into ghGetFileContent, ghViewRepoStructure, or clone for AST/LSP proof. Empty/unindexed is not absence.`,
  schema: {
    keywords:
      'ANDed; keep a phrase as one item; alternatives need separate queries.',
    path: 'Repo path prefix, not a full file path.',
    match:
      '"file" searches contents, returns snippets+matchIndices (GitHub\'s raw offsets within each snippet — a separate mechanism from matchRanges/matchedLines on the content-read tools, don\'t conflate them). "path" searches only paths/names — no snippets, far cheaper; use to confirm a file exists before reading. (Distinct from ghSearchRepos/ghSearchPullRequests `match`, which picks text fields.)',
    extension: 'File extension filter, e.g. "ts" (no dot).',
    filename: 'Exact filename filter.',
    language: 'GitHub language filter, e.g. "typescript".',
    concise: 'Flat "owner/repo:path" rows.',
    limit: 'Results per page (paginate with page).',
  },
});

export const GitHubCodeSearchQuerySchema = buildObject(ghSearchCode.schema, {
  ...metaFields,
  keywords: StringArray,
  owner: z.string().optional(),
  repo: z.string().optional(),
  extension: z.string().optional(),
  filename: z.string().optional(),
  path: z.string().optional(),
  language: z.string().optional(),
  match: z.enum(['file', 'path']).default('file'),
  limit: intRange(1, MAX_GITHUB_SEARCH_LIMIT).default(
    DEFAULT_GITHUB_SEARCH_LIMIT
  ),
  page: pageNumber(),
  concise: z.boolean().optional(),
});
