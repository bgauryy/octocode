import type {
  ItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';
import type { ToolResultMeta } from '../../types/toolResults.js';

// ---------------------------------------------------------------------------
// Output TYPES — describes what the GitHub code operation returns. No zod: the MCP server
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
  queryIndex?: number;
  matches: GitHubCodeSearchFileMatch[];
}

export interface GitHubCodeSearchData {
  // concise:true collapses each file to "owner/repo:path" strings;
  // default mode returns structured file objects with matches.
  files: Array<string | GitHubCodeSearchFile>;
  pagination?: CodeSearchPaginationLocal;
  nonExistentScope?: true;
  incompleteResults?: true;
  next?: Record<string, ToolContinuation>;
}

export type RepoState =
  | { kind: 'notFound' }
  | { kind: 'archived' }
  | { kind: 'renamed'; fullName: string };

export type EmptyCodeSearchQuery = {
  index: number;
  nonExistentScope?: true;
  incompleteResults?: true;
};

export type RepoStateEntry = {
  index: number;
  state: RepoState;
  query: object | undefined;
};

export type CodeSearchAgentRow = {
  index: number;
  meta?: ToolResultMeta;
  data: GitHubCodeSearchData | { error: string };
};

export type GitHubCodeSearchOutputLocal = BulkToolOutput<
  GitHubCodeSearchData | { error: string }
> & {
  // GitHub code search returns no absolute line numbers; row-local `data.next` carries a
  // ready-made ghGetFileContent matchString call per result record so agents
  // can resolve exact file:line anchors in one step instead of cloning.
  // Index signature: satisfies BulkFinalizer's `TOutput extends
  // Record<string, unknown>` constraint (the old zod-inferred type did too).
  [key: string]: unknown;
};
