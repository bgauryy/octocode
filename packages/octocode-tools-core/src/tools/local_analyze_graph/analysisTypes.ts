import type { WalkResult } from '../../graph/buildFileGraph.js';

export type GraphOperation =
  | 'deadCode'
  | 'cycles'
  | 'dependencies'
  | 'dependents'
  | 'path'
  | 'reachability';

export interface AnalyzeGraphQuery {
  operation: GraphOperation;
  /** Absolute repo/package root to scan. Inferred from `file` when omitted and `file` is absolute. */
  path?: string;
  file?: string;
  target?: string;
  depth?: number;
  entrypoints?: string[];
  includeTests?: boolean;
  excludeDir?: string[];
  maxFiles?: number;
  limit?: number;
  page?: number;
  pageSize?: number;
}

export interface AnalyzeGraphOutput {
  status?: 'empty' | 'error';
  error?: string;
  errorCode?: string;
  operation: GraphOperation;
  path: string;
  filesScanned?: number;
  filesSkipped?: number;
  truncated?: boolean;
  terminalLimit?: boolean;
  partialReasons?: Array<'maxFiles' | 'limit' | 'filesSkipped'>;
  totalAvailable?: number;
  results: Array<Record<string, unknown>>;
  summary?: Record<string, unknown>;
  pagination?: {
    currentPage: number;
    totalPages: number;
    entriesPerPage: number;
    totalEntries: number;
    hasMore: boolean;
    outOfRange?: boolean;
  };
  next?: Record<string, unknown>;
  warnings?: string[];
  confidence?: 'low';
  [key: string]: unknown;
}

export interface AnalyzeGraphContext {
  getGraph?: (
    path: string,
    excludeDir: string[],
    maxFiles: number
  ) => WalkResult | Promise<WalkResult>;
}
