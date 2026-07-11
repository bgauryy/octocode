/**
 * OQL result envelope — pagination, provenance, continuations, and the
 * top-level `OqlResultEnvelope`/`OqlBatchResultEnvelope` shapes `runOqlSearch`
 * returns.
 */

import type { QuerySource } from './predicates.js';
import type { OqlCanonicalInput } from './query.js';
import type { OqlDiagnostic } from './diagnostics.js';
import type { OqlExplainPlan } from './planner.js';
import type { OqlProofGradedResultRow } from './recordRows.js';

export interface Pagination {
  currentPage?: number;
  totalPages?: number;
  nextPage?: number;
  itemsPerPage?: number;
  totalItems?: number;
  reportedTotalItems?: number;
  reachableTotalItems?: number;
  totalItemsKind?: string;
  itemUnit?: string;
  rowCount?: number;
  reportedRowCount?: number;
  totalItemsCapped?: boolean;
  uniqueFileCount?: number;
  hasMore: boolean;
  next?: OqlContinuation;
}

export interface OqlProvenance {
  backend: string;
  source?: QuerySource;
  predicateIds?: string[];
  pushed?: string[];
  residual?: string[];
  routed?: string[];
  materializedPath?: string;
  cache?: 'hit' | 'miss' | 'refresh' | 'stale';
}

export interface OqlContinuation {
  query: OqlCanonicalInput;
  baseQueryId?: string;
  queryIndex?: number;
  why?: string;
  confidence?: 'exact' | 'heuristic';
}

export interface OqlContinuationHint {
  why: string;
  confidence: 'exact' | 'heuristic';
}

export type EvidenceKind = 'proof' | 'partial' | 'candidate' | 'unsupported';

export interface OqlResultEnvelope {
  queryId?: string;
  queryIndex?: number;
  results: OqlProofGradedResultRow[];
  shared?: Record<string, unknown>;
  pagination?: Pagination;
  next?: Record<string, OqlContinuation>;
  nextHints?: Record<string, OqlContinuationHint>;
  diagnostics: OqlDiagnostic[];
  provenance: OqlProvenance[];
  evidence: {
    answerReady: boolean;
    complete: boolean;
    kind: EvidenceKind;
  };
  // present only when explain:true / --explain
  plan?: OqlExplainPlan;
}

export interface OqlBatchResultEnvelope {
  batchId?: string;
  mode: 'independent' | 'merge';
  children: Array<{
    queryId: string;
    queryIndex: number;
    envelope: OqlResultEnvelope;
  }>;
  merged?: OqlResultEnvelope;
  diagnostics: OqlDiagnostic[];
}

export type OqlRunResult = OqlResultEnvelope | OqlBatchResultEnvelope;
