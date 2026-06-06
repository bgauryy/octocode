import type { EvidenceMetadata, FlatQueryResult } from './toolResults.js';

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

export interface BulkResponseConfig<
  TQuery = object,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> {
  toolName: string;
  keysPriority?: string[];

  peerHints?: boolean;

  peerEvidence?: boolean;
  concurrency?: number;

  minQueryTimeoutMs?: number;

  finalize?: BulkFinalizer<TQuery, TOutput>;
}

export interface BulkToolResponse {
  results: FlatQueryResult[];

  hints?: string[];

  base?: string;

  shared?: Record<string, string | number | boolean>;

  evidence?: EvidenceMetadata;
}
