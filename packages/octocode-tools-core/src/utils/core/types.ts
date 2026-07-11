export interface ExecResult {
  code: number | null;
  stdout: string;
  stderr: string;
  success: boolean;
  /** True when output hit maxOutputSize and stdout is the partial prefix. */
  truncated?: boolean;
}

export interface ExecOptions {
  cwd?: string;
  timeout?: number;
  env?: Record<string, string>;
  maxOutputSize?: number;
  toolName?: string;
  /**
   * When true, an output-size overflow returns the partial stdout flagged
   * `truncated` instead of throwing. For commands (e.g. `strings` on a huge
   * binary) where a bounded prefix is still useful.
   */
  tolerateOutputLimit?: boolean;
}

/** Canonical pagination type — single source in types/toolResults.ts. */
export type { PaginationInfo } from '../../types/toolResults.js';

export interface SearchStats {
  totalOccurrences?: number;
  totalStructuralMatches?: number;
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
