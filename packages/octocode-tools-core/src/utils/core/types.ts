export interface ExecResult {
  code: number | null;
  stdout: string;
  stderr: string;
  success: boolean;
}

export interface ExecOptions {
  cwd?: string;
  timeout?: number;
  env?: Record<string, string>;
  maxOutputSize?: number;
  toolName?: string;
}

export interface PaginationInfo {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;

  charOffset?: number;
  charLength?: number;
  totalChars?: number;
  /** How the page boundary was chosen: snapped to a semantic block boundary
   *  (`'semantic'`) or fixed by char count (`'char-limit'`). */
  chunkMode?: 'semantic' | 'char-limit';
  /** Set when `chunkMode` is `'char-limit'` and the cut is mid-block.
   *  Char offset of the next top-level block start; extend charLength to it. */
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
}

export interface SearchStats {
  matchCount?: number;
  matchedLines?: number;
  filesMatched?: number;
  filesSearched?: number;
  bytesSearched?: number;
  searchTime?: string;
}

export interface CacheStats {
  hits: number;
  misses: number;
  sets: number;
  totalKeys: number;
  lastReset: Date;
}
