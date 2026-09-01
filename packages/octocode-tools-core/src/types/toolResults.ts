import type { GitHubAPIError } from '../github/githubAPI.js';

export type QueryStatus = 'empty' | 'error';

interface ToolResult {
  status?: QueryStatus;
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
  nextPage?: number;
  nextMatchPage?: number;
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
  [key: string]: unknown;
}

export interface FlatQueryResult {
  /** Zero-based position of the originating query in the submitted batch. */
  index: number;
  /** Present only when this query's response was served from cache. */
  cache?: 1;
  status?: QueryStatus;
  meta: ToolResultMeta;
  data: Record<string, unknown>;
}

export type EvidenceKind =
  'exact' | 'lexical' | 'structural' | 'syntactic' | 'semantic' | 'provider';

export interface ToolResultMeta {
  evidence: {
    kind: EvidenceKind;
    confidence: 'high' | 'medium' | 'low';
  };
  diagnostics?: {
    codes?: string[];
    hints?: string[];
    partial?: boolean;
  };
}

export interface QueryError {
  queryIndex: number;
  error: string;
}

export interface StructuredToolResponse {
  data?: unknown;
  instructions?: string;
  [key: string]: unknown;
}
