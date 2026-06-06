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
  byteOffset?: number;
  byteLength?: number;
  totalBytes?: number;
  charOffset?: number;
  charLength?: number;
  totalChars?: number;
  filesPerPage?: number;
  totalFiles?: number;
  entriesPerPage?: number;
  totalEntries?: number;
  matchesPerPage?: number;
  totalMatches?: number;
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
