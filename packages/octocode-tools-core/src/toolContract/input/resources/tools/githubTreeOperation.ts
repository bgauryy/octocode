import { z } from 'zod';

import {
  buildObject,
  DEFAULT_GITHUB_STRUCTURE_ITEMS_PER_PAGE,
  intRange,
  MAX_GITHUB_STRUCTURE_DEPTH,
  MAX_GITHUB_STRUCTURE_ITEMS_PER_PAGE,
  metaFields,
  pageNumber,
} from './_toolkit.js';

export const githubTreeOperationDescriptions = {
  path: 'Repo-relative directory; ""/"." for root.',
  branch: 'Ref; fallback warning means default branch was used.',
  maxDepth: 'Tree recursion depth.',
  page: 'Advance only on hasMore.',
  metadataPage:
    'Page of the requested contributors, branches, or tags; follow next for each list independently.',
  include:
    'Optional sizes, languages, contributors, branches, or tags; each adds an API call.',
};

export const GitHubViewRepoStructureQuerySchema = buildObject(
  githubTreeOperationDescriptions,
  {
    ...metaFields,
    owner: z.string(),
    repo: z.string(),
    branch: z.string().optional(),
    path: z.string().optional(),
    maxDepth: intRange(0, MAX_GITHUB_STRUCTURE_DEPTH).optional(),
    page: pageNumber(),
    metadataPage: pageNumber().optional(),
    itemsPerPage: intRange(1, MAX_GITHUB_STRUCTURE_ITEMS_PER_PAGE).default(
      DEFAULT_GITHUB_STRUCTURE_ITEMS_PER_PAGE
    ),
    include: z
      .array(z.enum(['sizes', 'languages', 'contributors', 'branches', 'tags']))
      .optional(),
  }
);
