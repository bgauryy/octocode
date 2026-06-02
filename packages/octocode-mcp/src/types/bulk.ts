/**
 * Bulk-response config and finalizer types — the shape contract between
 * `executeBulkOperation` and per-tool finalizers that own their response shape.
 *
 * @module types/bulk
 */

import type {
  EvidenceMetadata,
  FlatQueryResult,
  PaginationInfo,
} from './toolResults.js';

/**
 * Input handed to a {@link BulkFinalizer} after `executeBulkOperation` has
 * processed every query. All side effects (provider calls, error isolation,
 * timeouts) have already run; the finalizer owns shape, char-pagination, and
 * serialization for tools that emit a non-default response shape.
 */
export interface BulkFinalizerInput<
  TQuery,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> {
  queries: TQuery[];
  /** One entry per query, in original order. Mirrors the default flat shape. */
  results: FlatQueryResult[];
  config: BulkResponseConfig<TQuery, TOutput>;
}

/**
 * Output of a finalizer. When typed via the schema generic (`TOutput =
 * z.infer<typeof MySchema>`), `structuredContent` is checked at compile time
 * against the registered MCP output schema — preventing shape drift that
 * would only surface as a runtime SDK validation error.
 *
 * The `TOutput extends Record<string, unknown>` constraint guarantees
 * structural compatibility with the MCP SDK's `CallToolResult.structuredContent`
 * type, so the bulk runner can return it directly without an `as` cast.
 */
export interface BulkFinalizerOutput<
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> {
  /** Sanitized structured content returned to the MCP client. */
  structuredContent: TOutput;
  /** Serialized text payload returned in `content[0].text`. */
  text: string;
  /** Whether to flag the response as an error. Defaults to standard derivation. */
  isError?: boolean;
}

export type BulkFinalizer<
  TQuery,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> = (
  input: BulkFinalizerInput<TQuery, TOutput>
) => BulkFinalizerOutput<TOutput>;

/** Configuration for bulk response formatting. */
export interface BulkResponseConfig<
  TQuery = object,
  TOutput extends Record<string, unknown> = Record<string, unknown>,
> {
  toolName: string;
  keysPriority?: string[];
  responseCharOffset?: number;
  responseCharLength?: number;
  /**
   * When true, hints attached to each query's data are lifted out and
   * deduped into a single top-level `hints` array on the response.
   * Opt-in because some output schemas reject extra top-level fields.
   * Defaults to false (per-query hints stay in `data.hints` as before).
   */
  peerHints?: boolean;
  /**
   * When true, per-query `data.evidence` blocks are lifted out, deduped,
   * and aggregated into a single top-level `evidence` object on the
   * response (answerReady = AND of all, complete = AND of all, confidence
   * = lowest of all). Opt-in for the same reason as `peerHints`.
   */
  peerEvidence?: boolean;
  /**
   * Maximum number of concurrent requests during bulk operations.
   * Lower values reduce rate limiting risk, higher values improve throughput.
   * @default 3
   */
  concurrency?: number;
  /**
   * Minimum per-query timeout in milliseconds.
   * Overrides the computed fair-share timeout when it would be lower.
   * Useful for expensive operations (e.g. LSP cold-start) that need
   * a guaranteed minimum regardless of query count.
   */
  minQueryTimeoutMs?: number;
  /**
   * Optional finalizer hook. When provided, the default per-query +
   * bulk-response char-pagination steps are skipped and the finalizer owns
   * the response shape entirely. Telemetry (`incrementToolCharSavings`)
   * still runs through the bulk runner so every tool reports raw vs sent
   * chars uniformly. Parametrize `TOutput` with `z.infer<typeof
   * MyOutputSchema>` to get compile-time shape safety.
   */
  finalize?: BulkFinalizer<TQuery, TOutput>;
}

/** Bulk response format. */
export interface BulkToolResponse {
  results: FlatQueryResult[];
  responsePagination?: PaginationInfo;
  /** Hints aggregated to peer level (no longer nested per-query). */
  hints?: string[];
  /** Common directory the relativized `path`/`uri` cells hang off (lean output). */
  base?: string;
  /** Scalar fields hoisted out of every leaf object because they shared one
   * identical value (lean output). Each leaf re-gains every key on reconstruction. */
  shared?: Record<string, string | number | boolean>;
  /** Aggregated evidence metadata, lifted from per-query `data.evidence`. */
  evidence?: EvidenceMetadata;
}
