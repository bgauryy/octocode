/**
 * Hint status types — hints fire only on no-result paths.
 * - 'empty': Tool returned no results (but no error)
 * - 'error': Tool encountered an error
 *
 * Success-path signals live in the response envelope (pagination/evidence/
 * warnings) and the tool description.
 */
export type HintStatus = 'empty' | 'error';

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

  // Query-shape fields used by empty-result hint generators.
  // These let per-tool hints.ts name the actual filters in play
  // when no results came back.
  owner?: string;
  repo?: string;
  branch?: string;
  extension?: string;
  filename?: string;
  keywords?: string[];
  state?: string;
  author?: string;
  query?: string;
  prNumber?: number;

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

  // githubSearchCode.hasResults uses this to warn when all returned matches
  // live in non-canonical paths (examples/__tests__/docs/fixtures).
  matchedPaths?: string[];
  // Total matches across pages (githubSearchCode pagination warning).
  totalMatches?: number;
  hasMore?: boolean;
  // githubViewRepoStructure.hasResults uses this to surface feature-flag /
  // *Mode / *Config / *Flag files that often gate the real implementation
  // a direct code search would miss.
  flagFiles?: string[];
}

type HintGenerator = (context: HintContext) => (string | undefined)[];

export interface ToolHintGenerators {
  empty: HintGenerator;
  error: HintGenerator;
}
