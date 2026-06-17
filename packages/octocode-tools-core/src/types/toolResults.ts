import type { GitHubAPIError } from '../github/githubAPI.js';

export type QueryStatus = 'empty' | 'error';

interface ToolResult {
  status?: QueryStatus;
  hints?: string[];
  [key: string]: unknown;
}

export interface ToolErrorResult extends ToolResult {
  status: 'error';
  error: string | GitHubAPIError;
}

export interface ToolSuccessResult extends ToolResult {
  status?: 'empty';
}

export interface PaginationInfo {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  charOffset?: number;
  charLength?: number;
  totalChars?: number;
  nextCharOffset?: number;

  chunkMode?: 'semantic' | 'char-limit';

  nextBlockChar?: number;
  perPage?: number;
  itemsPerPage?: number;
  filesPerPage?: number;
  totalFiles?: number;
  entriesPerPage?: number;
  totalEntries?: number;
  matchesPerPage?: number;
  totalMatches?: number;
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
  uniqueFileCount?: number;
}

export type ToolInvocationCallback = (
  toolName: string,
  queries: unknown[]
) => Promise<void>;

export interface ProcessedBulkResult {
  data?: Record<string, unknown>;
  error?: string | GitHubAPIError;
  status?: QueryStatus;
  hints?: readonly string[] | string[];
  [key: string]: unknown;
}

export interface FlatQueryResult {
  id: string;
  status?: QueryStatus;
  data: Record<string, unknown>;
}

export interface QueryError {
  queryIndex: number;
  error: string;
}

export interface StructuredToolResponse {
  data?: unknown;
  hints?: string[];
  instructions?: string;
  [key: string]: unknown;
}
