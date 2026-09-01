import { z } from 'zod';

import type { ToolSpec } from '../../types/index.js';
import { defineTool } from './_toolkit.js';
import { GitHubCodeSearchQuerySchema } from './githubCodeOperation.js';
import { GitHubReposSearchSingleQuerySchema } from './githubRepositoriesOperation.js';
import { GitHubViewRepoStructureQuerySchema } from './githubTreeOperation.js';

export const ghSearch: ToolSpec = defineTool({
  name: 'ghSearch',
  type: 'Github',
  shortDescription:
    'Search GitHub code or repositories, or browse a repository tree.',
  instructions: `Choose operation:"code" for code contents or file paths, "repositories" to discover repositories, or "tree" to browse a known repository.
Each operation has a strict input branch; fields from other operations are rejected. Use ghGetFileContent only after a path is known. Empty GitHub search results do not prove absence.`,
  schema: {
    operation: 'Required: "code", "repositories", or "tree".',
  },
});

const codeQuerySchema = GitHubCodeSearchQuerySchema.omit({ limit: true })
  .extend({
    match: GitHubCodeSearchQuerySchema.shape.match.describe(
      'For operation:"code": "file" returns snippets; "path" returns paths only.'
    ),
    pageSize: GitHubCodeSearchQuerySchema.shape.limit.describe(
      'Results returned per page.'
    ),
    operation: z
      .literal('code')
      .describe('Search code contents or file paths.'),
  })
  .strict();

const repositoriesQuerySchema = GitHubReposSearchSingleQuerySchema.omit({
  topicsToSearch: true,
  limit: true,
})
  .extend({
    topics: GitHubReposSearchSingleQuerySchema.shape.topicsToSearch.describe(
      'Repository topics; all must match.'
    ),
    pageSize: GitHubReposSearchSingleQuerySchema.shape.limit.describe(
      'Results returned per page.'
    ),
    match: GitHubReposSearchSingleQuerySchema.shape.match.describe(
      'For operation:"repositories": fields to search: name, description, or readme.'
    ),
    operation: z
      .literal('repositories')
      .describe('Discover repositories by keywords and repository filters.'),
  })
  .strict();

const treeQuerySchema = GitHubViewRepoStructureQuerySchema.omit({
  itemsPerPage: true,
})
  .extend({
    operation: z
      .literal('tree')
      .describe('Browse the tree of a known repository.'),
    pageSize: GitHubViewRepoStructureQuerySchema.shape.itemsPerPage.describe(
      'Tree entries returned per page.'
    ),
  })
  .strict();

export const GitHubSearchQuerySchema = z.discriminatedUnion('operation', [
  codeQuerySchema,
  repositoriesQuerySchema,
  treeQuerySchema,
]);

export type GitHubSearchQuery = z.infer<typeof GitHubSearchQuerySchema>;
