import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import {
  buildObject,
  DEFAULT_GITHUB_STRUCTURE_ITEMS_PER_PAGE,
  defineTool,
  intRange,
  MAX_GITHUB_STRUCTURE_DEPTH,
  MAX_GITHUB_STRUCTURE_ITEMS_PER_PAGE,
  metaFields,
  pageNumber,
} from './_toolkit.js';

export const ghViewRepoStructure: ToolSpec = defineTool({
  name: 'ghViewRepoStructure',
  type: 'Github',
  shortDescription: "Browse a GitHub repository's directory tree.",
  instructions: `Orient a repo/tree before fetching files, or verify paths/branches after 404s. Know a filename fragment? ghSearchCode(match:"path") is cheaper. Follow structure[] paths into ghGetFileContent or a clone sparsePath.`,
  schema: {
    path: 'Repo-relative directory; ""/"." for root.',
    branch: 'Ref; fallback warning means default branch was used.',
    maxDepth: 'Tree recursion depth.',
    page: 'Advance only on hasMore.',
    include:
      'Optional repo enrichments (each adds ~1 API call, fetched concurrently): "sizes" per-file bytes; "languages" byte breakdown + dominant language; "contributors" top contributors; "branches"; "tags". Repo-level (ignore path) except "sizes".',
  },
});

export const GitHubViewRepoStructureQuerySchema = buildObject(
  ghViewRepoStructure.schema,
  {
    ...metaFields,
    owner: z.string(),
    repo: z.string(),
    branch: z.string().optional(),
    path: z.string().optional(),
    maxDepth: intRange(0, MAX_GITHUB_STRUCTURE_DEPTH).optional(),
    page: pageNumber(),
    itemsPerPage: intRange(1, MAX_GITHUB_STRUCTURE_ITEMS_PER_PAGE).default(
      DEFAULT_GITHUB_STRUCTURE_ITEMS_PER_PAGE
    ),
    include: z
      .array(z.enum(['sizes', 'languages', 'contributors', 'branches', 'tags']))
      .optional(),
  }
);
