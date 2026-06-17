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

  text: string;

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
  currentPage: number;

  totalPages: number;

  hasMore: boolean;

  charOffset: number;

  charLength: number;

  totalChars: number;

  nextCharOffset?: number;
}

export interface BulkResponseConfig<
  TQuery = object,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> {
  toolName: string;
  keysPriority?: string[];

  peerHints?: boolean;

  concurrency?: number;

  minQueryTimeoutMs?: number;

  responsePagination?: BulkResponsePagination;

  finalize?: BulkFinalizer<TQuery, TOutput>;
}

export interface BulkToolResponse {
  results: FlatQueryResult[];

  hints?: string[];

  base?: string;

  shared?: Record<string, string | number | boolean>;

  responsePagination?: ResponsePaginationInfo;
}
