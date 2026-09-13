import type { z } from 'zod';
import type { FileContentQuerySchema } from '@octocodeai/octocode-core/schema';
import type { MinifyMode } from '@octocodeai/octocode-core/schema';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { FetchPagination } from '@octocodeai/octocode-core/extra-types';
import type { ToolContinuation } from '../../scheme/pagination.js';

export type FileContentExecutionQuery = FileContentQuery & {
  noTimestamp?: boolean;
  minify: MinifyMode;
  contextLines?: number;
  matchStringIsRegex?: boolean;
  matchStringCaseSensitive?: boolean;
};

export interface GitHubFileContentApiData {
  owner?: string;
  repo?: string;
  path?: string;
  content?: string;
  contentView?: 'none' | 'standard' | 'symbols';
  branch?: string;
  resolvedBranch?: string;
  startLine?: number;
  endLine?: number;
  isPartial?: boolean;
  errorCode?: 'contentSecurityLimit' | 'fullContentLimit' | 'noMatches';
  terminalLimit?: boolean;
  partialReasons?: Array<
    'security-selected-view-size-limit' | 'full-content-size-limit'
  >;
  totalLines?: number;
  sourceChars?: number;
  sourceBytes?: number;
  returnedChars?: number;
  returnedBytes?: number;
  returnedLines?: number;
  selectedMatchCount?: number;
  sourceLines?: number[];
  /** Internal contiguous-view origin; present only for unchanged exact source. */
  sourceLineOffset?: number;
  sourceLineRanges?: Array<{ start: number; end: number }>;
  next?: Record<string, ToolContinuation>;
  minifyFallback?: {
    requested: MinifyMode;
    applied: MinifyMode;
    reason: 'match-evidence' | 'outline-unavailable';
  };

  matchRanges?: Array<{ start: number; end: number }>;
  /** Exact matched-line numbers; matchRanges span the selected source windows. */
  matchedLines?: number[];
  matchLocations?: string[];
  warnings?: string[];
  lastModified?: string;
  lastModifiedBy?: string;
  pagination?: FetchPagination;
  cached?: boolean;
  matchNotFound?: boolean;
  searchedFor?: string;
}

interface GitHubFileContentApiResultBase {
  error?: string;
  hints?: string[];
}

export interface GitHubFileContentApiResult
  extends GitHubFileContentApiResultBase, GitHubFileContentApiData {}
