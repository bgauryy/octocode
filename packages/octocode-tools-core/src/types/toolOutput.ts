/**
 * Hand-written output TYPES for the direct tools.
 *
 * Tools keep one shared TypeScript envelope generic plus a per-tool `data`
 * interface. MCP advertises the matching broad runtime contract sourced from
 * @octocodeai/octocode-core, avoiding per-interface schema duplication.
 *
 * INPUT/query schemas are unaffected: those are registered and parsed, so they
 * stay as zod.
 */
import type { ToolResultMeta } from './toolResults.js';

/** Outermost char-window pagination wrapping a whole bulk response. */
export interface ResponsePaginationInfo {
  scope: 'content.text';
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  charOffset: number;
  charLength: number;
  totalChars: number;
  nextCharOffset?: number;
}

/** One result row in a bulk tool response. */
export interface BulkToolResultRow<TData> {
  /** Zero-based position of the originating query in the submitted batch. */
  index: number;
  status?: 'empty' | 'error';
  meta?: ToolResultMeta;
  data: TData;
}

/** The shared bulk envelope every direct tool returns, parameterized by its per-query `data` payload. */
export interface BulkToolOutput<TData> {
  results: Array<BulkToolResultRow<TData>>;
  base?: string;
  shared?: Record<string, string | number | boolean>;
  responsePagination?: ResponsePaginationInfo;
}
