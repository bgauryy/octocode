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
  /**
   * How the page boundary was chosen.
   * `'semantic'`   — snapped to a tree-sitter/heuristic block boundary; the
   *                  page ends at a complete function/class, never mid-body.
   * `'char-limit'` — fixed char-size cut; may end mid-block (see nextBlockChar).
   * Absent on structure/item pagination (not applicable).
   */
  chunkMode?: 'semantic' | 'char-limit';
  /**
   * When chunkMode is `'char-limit'` and the cut lands mid-block, this is the
   * char offset of the next top-level semantic boundary. Agents should
   * re-request with charLength extended to this offset rather than paginating
   * blindly through a function body.
   */
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

export interface EvidenceMetadata {
  kind?:
    | 'metadata'
    | 'content'
    | 'structure'
    | 'code'
    | 'docs'
    | 'config'
    | 'pr'
    | 'repo'
    | 'package'
    | 'definition'
    | 'references'
    | 'calls';
  answerReady?: boolean;
  confidence?: 'high' | 'medium' | 'low';
  complete?: boolean;
  reason?: string;
  missingFields?: string[];
}
