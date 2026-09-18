import type { FetchPagination } from '@octocodeai/octocode-core/extra-types';
import type { ToolContinuation } from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

export interface FileContentMatchRange {
  start: number;
  end: number;
}

export interface LocalFetchData {
  path?: string;
  content?: string;
  contentView?: 'none' | 'standard' | 'symbols';
  totalLines?: number;
  sourceChars?: number;
  sourceBytes?: number;
  returnedChars?: number;
  startLine?: number;
  endLine?: number;
  isPartial?: boolean;
  matchRanges?: FileContentMatchRange[];
  sourceLineRanges?: FileContentMatchRange[];
  pagination?: FetchPagination;
  returnedBytes?: number;
  returnedLines?: number;
  matchedLines?: number[];
  selectedMatchCount?: number;
  next?: Record<string, ToolContinuation>;
  modified?: string;
  warnings?: string[];
  matchNotFound?: boolean;
  searchedFor?: string;
}

export type LocalFetchOutput = BulkToolOutput<LocalFetchData>;
