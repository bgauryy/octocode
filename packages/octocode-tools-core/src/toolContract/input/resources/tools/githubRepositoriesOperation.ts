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

export const githubRepositoriesOperationDescriptions = {
  keywords:
    'ANDed; keep a phrase as one item; alternatives need separate queries.',
  topicsToSearch: 'ANDed topic filters; metadata can be sparse.',
  match:
    'For ghSearch operation:"repositories": text fields to search are name, description, and readme; readme is broader and slower. Operation:"code" uses "file" or "path".',
  sort: 'stars, forks, updated, help-wanted-issues, best-match.',
  language: 'Primary language filter, e.g. "rust".',
  stars: 'Star range: ">100", "10..50".',
  forks: 'Fork range: ">50", "10..100".',
  goodFirstIssues: 'Good-first-issue count range, e.g. ">5".',
  updated: 'Last push date: ">2024-01-01" or "a..b".',
  created: 'Creation date: ">2023-01-01" or "a..b".',
  size: 'Repo size in KB: ">1000", "50..500".',
  license: 'SPDX id, e.g. "mit".',
  archived: 'true = archived only; default excludes archived.',
  visibility: '"public" or "private" (private needs token scope).',
  concise: 'Flat "owner/repo" rows for triage.',
  limit: 'Results per page (paginate with page).',
};

export const GitHubReposSearchSingleQuerySchema = buildObject(
  githubRepositoriesOperationDescriptions,
  {
    ...metaFields,
    keywords: StringArray,
    topicsToSearch: StringArray,
    language: z.string().optional(),
    owner: z.string().optional(),
    stars: z.string().optional(),
    forks: z.string().optional(),
    goodFirstIssues: z.string().optional(),
    updated: z.string().optional(),
    created: z.string().optional(),
    size: z.string().optional(),
    match: z.array(z.enum(['name', 'description', 'readme'])).optional(),
    sort: z
      .enum(['stars', 'forks', 'help-wanted-issues', 'updated', 'best-match'])
      .default('best-match'),
    limit: intRange(1, MAX_GITHUB_SEARCH_LIMIT).default(
      DEFAULT_GITHUB_SEARCH_LIMIT
    ),
    page: pageNumber(),
    archived: z.boolean().optional(),
    visibility: z.enum(['public', 'private']).optional(),
    license: z.string().optional(),
    concise: z.boolean().optional(),
  }
);
