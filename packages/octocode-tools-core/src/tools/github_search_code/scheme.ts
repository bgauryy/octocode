import { z } from 'zod';
import { GitHubCodeSearchQuerySchema as CoreGitHubCodeSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
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
import type { ItemPagination } from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

const queryOverrides = {
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT).optional(),
  page: relaxedPageNumberField.default(1),
  // `match` here selects WHERE the search looks (file contents vs paths) — a
  // different concept from `match` on ghSearchRepos/ghSearchPullRequests, which
  // select WHICH text fields to search. Don't carry intuition across tools.
  match: z
    .enum(['file', 'path'])
    .default('file')
    .describe(
      '"file" searches file contents and returns snippets with matchIndices. "path" searches only file paths/names — no snippets, far cheaper; use it to confirm a file exists before reading it. (Unlike ghSearchRepos/ghSearchPullRequests, where `match` instead selects which text FIELDS to search — a different concept sharing this name.)'
    ),
} as const;

export const GitHubCodeSearchQueryLocalSchema = describeQuerySchema(
  CoreGitHubCodeSearchQuerySchema,
  queryOverrides
);

export const GitHubCodeSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    createQueryShapeSchema(CoreGitHubCodeSearchQuerySchema, queryOverrides)
  );

// ---------------------------------------------------------------------------
// Output TYPES — describes what ghSearchCode returns. No zod: the MCP server
// registers no outputSchema. Shared envelope lives in types/toolOutput.ts.
// ---------------------------------------------------------------------------

// Search-specific pagination: extends the canonical base with fields that are
// semantically unique to code-search (not aliases for existing canonical fields).
export interface CodeSearchPaginationLocal extends ItemPagination {
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
  uniqueFileCount?: number;
}

export interface GitHubCodeSearchFileMatch {
  value?: string;
  pathOnly?: boolean;
  matchIndices?: Array<{ start: number; end: number; lineOffset: number }>;
}

export interface GitHubCodeSearchFile {
  owner: string;
  repo: string;
  path: string;
  queryId?: string;
  matches: GitHubCodeSearchFileMatch[];
}

export interface GitHubCodeSearchData {
  // concise:true collapses each file to "owner/repo:path" strings;
  // default mode returns structured file objects with matches.
  files: Array<string | GitHubCodeSearchFile>;
  pagination?: CodeSearchPaginationLocal;
}

export type GitHubCodeSearchOutputLocal =
  BulkToolOutput<GitHubCodeSearchData> & {
    emptyQueries?: Array<{
      id: string;
      nonExistentScope?: true;
      incompleteResults?: true;
    }>;
    // GitHub code search returns no absolute line numbers; `next` carries a
    // ready-made ghGetFileContent matchString call per result record so agents
    // can resolve exact file:line anchors in one step instead of cloning.
    errors?: Array<{ id: string; error: string }>;
    // Index signature: satisfies BulkFinalizer's `TOutput extends
    // Record<string, unknown>` constraint (the old zod-inferred type did too).
    [key: string]: unknown;
  };
