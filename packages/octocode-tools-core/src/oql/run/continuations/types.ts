/**
 * Shared types for the per-row continuation builders (registry.ts, records.ts,
 * semantics.ts). Split out so builder modules can share the context/type shape
 * without creating a runtime circular import with the registry.
 */
import type { OqlContinuation, OqlQuery, OqlResultRow } from '../../types.js';

export interface ContinuationCtx {
  query: OqlQuery;
  /** code rows: rebuild an absolute `from` from a relativized row path. */
  fileFrom?: (rowPath: string) => OqlQuery['from'];
}

export type RowContinuationBuilder = (
  row: OqlResultRow,
  ctx: ContinuationCtx
) => Record<string, OqlContinuation> | undefined;
