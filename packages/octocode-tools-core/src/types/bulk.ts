import type { FlatQueryResult } from './toolResults.js';

export interface BulkFinalizerInput<
  TQuery,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> {
  queries: TQuery[];

  results: FlatQueryResult[];
  config: BulkResponseConfig<TQuery, TOutput>;
}

export interface BulkFinalizerOutput<
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> {
  structuredContent: TOutput;

  /** Rendering priority only. Text is derived once from finalized structured
   * content so MCP and CLI cannot observe different semantic states. */
  keysPriority?: readonly string[];

  /** Optional presentation for copy-sensitive payloads. It receives the final,
   * reconciled structured value and therefore cannot render stale metadata. */
  renderText?: (structuredContent: Record<string, unknown>) => string;

  isError?: boolean;
}

export type BulkFinalizer<
  TQuery,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> = (
  input: BulkFinalizerInput<TQuery, TOutput>
) => BulkFinalizerOutput<TOutput>;

export interface BulkResponsePagination {
  responseCharOffset?: number;

  responseCharLength?: number;
}

export interface ResponsePaginationInfo {
  scope: 'content.text';
  currentPage: number;

  totalPages: number;

  hasMore: boolean;

  charOffset: number;

  charLength: number;

  totalChars: number;

  nextCharOffset?: number;

  next?: {
    tool: string;
    query: Record<string, unknown>;
  };
}

export interface BulkResponseConfig<
  TQuery = object,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> {
  toolName: string;
  keysPriority?: string[];

  concurrency?: number;

  minQueryTimeoutMs?: number;

  responsePagination?: BulkResponsePagination;

  finalize?: BulkFinalizer<TQuery, TOutput>;
}

export interface BulkToolResponse {
  results: FlatQueryResult[];

  base?: string;

  shared?: Record<string, string | number | boolean>;

  responsePagination?: ResponsePaginationInfo;
}
