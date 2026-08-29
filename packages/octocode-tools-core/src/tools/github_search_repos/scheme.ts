import { z } from 'zod';
import { GitHubReposSearchSingleQuerySchema as CoreGitHubReposSearchSingleQuerySchema } from '../../toolContract/schemas.js';
import { GITHUB_SEARCH_MAX_LIMIT } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import type {
  ItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

const queryOverrides = {
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT).optional(),
  page: relaxedPageNumberField.default(1),
  // `match` here selects WHICH text fields to search — a different concept
  // from ghSearchCode's `match`, which selects WHERE the search looks (file
  // contents vs paths). Don't carry intuition across tools.
  match: z
    .array(z.enum(['name', 'description', 'readme']))
    .optional()
    .describe(
      "Which text fields to search: name, description, and/or readme. Defaults to name+description; add 'readme' for broader, slower full-text search. (Unlike ghSearchCode, where `match` instead selects file-contents vs file-paths — a different concept sharing this name.)"
    ),
} as const;

export const GitHubReposSearchSingleQueryLocalSchema = describeQuerySchema(
  CoreGitHubReposSearchSingleQuerySchema,
  queryOverrides
);

export const GitHubReposSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    createQueryShapeSchema(
      CoreGitHubReposSearchSingleQuerySchema,
      queryOverrides
    )
  );

// ---------------------------------------------------------------------------
// Output TYPES — describes what ghSearchRepos returns. No zod: the MCP server
// registers no outputSchema. Index signature mirrors the original
// .passthrough() for additive runtime fields. Shared envelope lives in
// types/toolOutput.ts.
// ---------------------------------------------------------------------------

export interface LocalRepositoryDetail {
  owner: string;
  repo: string;
  stars?: number;
  forks?: number;
  openIssuesCount?: number;
  language?: string;
  license?: string;
  description?: string;
  homepage?: string;
  pushedAt?: string;
  createdAt?: string;
  defaultBranch?: string;
  topics?: string[];
  visibility?: string;
  url?: string;
  updatedAt?: string;
}

// Repo-search-specific pagination: canonical base + search-confidence fields.
export interface RepoSearchPagination extends ItemPagination {
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
}

export interface RepositoryResultData {
  repositories?: Array<string | LocalRepositoryDetail>;
  pagination?: RepoSearchPagination;
  // Ready-to-run follow-ups for the top-ranked hit (viewStructure/searchCode).
  next?: Record<string, ToolContinuation>;
  // Partial-variant failures and empty-result guidance.
  [key: string]: unknown;
}

export type GitHubSearchRepositoriesOutputLocal =
  BulkToolOutput<RepositoryResultData>;
