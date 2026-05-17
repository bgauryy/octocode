/**
 * Hint status types for determining which hints to return
 * - 'hasResults': Tool returned results successfully
 * - 'empty': Tool returned no results (but no error)
 * - 'error': Tool encountered an error
 */
export type HintStatus = 'hasResults' | 'empty' | 'error';

/**
 * Context that tools can provide to generate smarter, context-aware hints.
 * Used by dynamic hint generators to provide intelligent guidance.
 */
export interface HintContext {
  fileSize?: number;
  resultSize?: number;
  tokenEstimate?: number;
  entryCount?: number;

  matchCount?: number;
  fileCount?: number;
  isLarge?: boolean;

  errorType?:
    | 'size_limit'
    | 'not_found'
    | 'permission'
    | 'pattern_too_broad'
    | 'symbol_not_found'
    | 'file_not_found'
    | 'timeout'
    | 'not_a_function'
    | 'rate_limit';
  originalError?: string;
  status?: number;

  isRateLimited?: boolean;
  retryAfter?: number;
  rateLimitRemaining?: number;

  hasPattern?: boolean;
  hasPagination?: boolean;
  path?: string;
  hasOwnerRepo?: boolean;
  match?: 'file' | 'path';
  searchEngine?: 'rg';

  hasConfigFiles?: boolean;

  locationCount?: number;
  hasExternalPackage?: boolean;
  isFallback?: boolean;
  searchRadius?: number;
  lineHint?: number;
  symbolName?: string;
  uri?: string;
  hasMultipleFiles?: boolean;
  hasMorePages?: boolean;
  currentPage?: number;
  totalPages?: number;
  direction?: 'incoming' | 'outgoing';
  callCount?: number;
  depth?: number;
  hasMoreContent?: boolean;
  isPartial?: boolean;
  endLine?: number;
  totalLines?: number;
  nextCharOffset?: number;
  totalChars?: number;
  filteredAll?: boolean;
}

export type HintGenerator = (context: HintContext) => (string | undefined)[];

export interface ToolHintGenerators {
  hasResults: HintGenerator;
  empty: HintGenerator;
  error: HintGenerator;
}
