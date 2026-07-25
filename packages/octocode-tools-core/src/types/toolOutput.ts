/**
 * Hand-written output TYPES for the direct tools.
 *
 * The per-tool zod OUTPUT schemas were removed: the MCP server registers no
 * outputSchema (see octocode-mcp registerBasicTool), so they validated nothing
 * at runtime, duplicated the tool result shapes, and forced every output-shape
 * change to be made in twelve places. Tools keep a plain TypeScript output
 * type instead — one shared envelope generic plus a per-tool `data` interface.
 *
 * INPUT/query schemas are unaffected: those are registered and parsed, so they
 * stay as zod.
 */
import type { ToolContinuation } from '../scheme/pagination.js';

/** Outermost char-window pagination wrapping a whole bulk response. */
export interface ResponsePaginationInfo {
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
  id: string;
  status?: 'empty' | 'error';
  data: TData;
  next?: Record<string, ToolContinuation>;
}

/** The shared bulk envelope every direct tool returns, parameterized by its per-query `data` payload. */
export interface BulkToolOutput<TData> {
  results: Array<BulkToolResultRow<TData>>;
  base?: string;
  shared?: Record<string, string | number | boolean>;
  responsePagination?: ResponsePaginationInfo;
  next?: Record<string, ToolContinuation>;
}
