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

  chunkMode?: 'semantic' | 'char-limit';

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
