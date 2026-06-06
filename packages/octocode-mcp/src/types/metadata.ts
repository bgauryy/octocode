export type HintStatus = 'empty' | 'error';

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
    | 'rate_limit'
    | 'lsp_unavailable';
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

  matchedPaths?: string[];
  totalMatches?: number;
  hasMore?: boolean;
  flagFiles?: string[];
}

type HintGenerator = (context: HintContext) => (string | undefined)[];

export interface ToolHintGenerators {
  empty: HintGenerator;
  error: HintGenerator;
}
