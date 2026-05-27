/**
 * Shared tool result types — status discriminator, pagination shape,
 * and per-query result shapes consumed by every tool runner.
 *
 * @module types/toolResults
 */

import type { GitHubAPIError } from '../github/githubAPI.js';

export type QueryStatus = 'hasResults' | 'empty' | 'error';

interface ToolResult {
  status: QueryStatus;
  hints?: string[];
  [key: string]: unknown;
}

export interface ToolErrorResult extends ToolResult {
  status: 'error';
  error: string | GitHubAPIError;
}

export interface ToolSuccessResult extends ToolResult {
  status: 'hasResults' | 'empty';
}

/**
 * Common pagination information used across tools.
 */
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

/**
 * Optional callback invoked when a tool is called with queries.
 */
export type ToolInvocationCallback = (
  toolName: string,
  queries: unknown[]
) => Promise<void>;

/** Processed result from bulk query execution. */
export interface ProcessedBulkResult {
  data?: Record<string, unknown>;
  error?: string | GitHubAPIError;
  status: QueryStatus;
  hints?: readonly string[] | string[];
  [key: string]: unknown;
}

/** Flattened query result for bulk operations. */
export interface FlatQueryResult {
  id: string;
  status: QueryStatus;
  data: Record<string, unknown>;
}

/** Error information for failed queries. */
export interface QueryError {
  queryIndex: number;
  error: string;
}

/** Single-result structured response format. */
export interface StructuredToolResponse {
  data?: unknown;
  hints?: string[];
  instructions?: string;
  [key: string]: unknown;
}

/**
 * Cross-tool evidence metadata. Tools opt in to populating these fields so
 * the agent can tell whether a response is answer-ready, complete, and what
 * kind of evidence was returned without inspecting the payload shape.
 */
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
    | 'references'
    | 'calls';
  answerReady?: boolean;
  confidence?: 'high' | 'medium' | 'low';
  complete?: boolean;
  reason?: string;
  missingFields?: string[];
}
