import { z } from 'zod';

import {
  buildObject,
  DEFAULT_GITHUB_SEARCH_LIMIT,
  intRange,
  MAX_GITHUB_SEARCH_LIMIT,
  metaFields,
  pageNumber,
  StringArray,
} from './_toolkit.js';

export const githubCodeOperationDescriptions = {
  keywords:
    'ANDed; keep a phrase as one item; alternatives need separate queries.',
  path: 'Repo path prefix, not a full file path.',
  match:
    'For ghSearch operation:"code": "file" returns snippets and character matchIndices; "path" returns paths only. Operation:"repositories" uses text-field names.',
  extension: 'File extension filter, e.g. "ts" (no dot).',
  filename: 'Exact filename filter.',
  language: 'GitHub language filter, e.g. "typescript".',
  concise: 'Flat "owner/repo:path" rows.',
  limit: 'Results per page (paginate with page).',
};

export const GitHubCodeSearchQuerySchema = buildObject(
  githubCodeOperationDescriptions,
  {
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
  }
);
