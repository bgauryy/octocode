/**
 * OQL result-envelope builders + proof lattice.
 *
 * evidence.kind:
 *   proof       every required predicate evaluated exactly over the universe
 *   partial     pages / candidates / residual checks remain
 *   candidate   at least one predicate/provider filter is approximate
 *   unsupported planner could not execute the requested semantics
 *
 * answerReady requires proof + complete (unless the caller asked only for
 * candidates).
 */
import { blocksAnswer } from './diagnostics.js';
import type {
  EvidenceKind,
  OqlBackendCall,
  OqlDiagnostic,
  OqlExplainPlan,
  OqlProofGradedResultRow,
  OqlProvenance,
  OqlResultEnvelope,
  Pagination,
} from './types.js';

export interface BuildEnvelopeArgs {
  queryId?: string;
  queryIndex?: number;
  results: OqlProofGradedResultRow[];
  pagination?: Pagination;
  next?: OqlResultEnvelope['next'];
  diagnostics: OqlDiagnostic[];
  provenance: OqlProvenance[];
  /** Whether the plan was executable (no UNSUPPORTED nodes / blocking errors). */
  executable: boolean;
  /** Whether any backend call was approximate (exact:false). */
  approximate?: boolean;
  plan?: OqlExplainPlan;
}

export function buildEnvelope(args: BuildEnvelopeArgs): OqlResultEnvelope {
  const kind = proofKind(args);
  const complete = isComplete(args);
  const answerReady = kind === 'proof' && complete;

  return {
    ...(args.queryId ? { queryId: args.queryId } : {}),
    ...(args.queryIndex !== undefined ? { queryIndex: args.queryIndex } : {}),
    results: args.results,
    ...(args.pagination ? { pagination: args.pagination } : {}),
    ...(args.next && Object.keys(args.next).length ? { next: args.next } : {}),
    diagnostics: args.diagnostics,
    provenance: args.provenance,
    evidence: { answerReady, complete, kind },
    ...(args.plan ? { plan: args.plan } : {}),
  };
}

/** Diagnostic codes that mean the requested semantics could not be executed. */
const UNSUPPORTED_CODES = new Set([
  'invalidQuery',
  'ambiguousSugar',
  'unknownField',
  'unsupportedTarget',
  'unsupportedPredicate',
  'unsupportedBoolean',
  'unsupportedScope',
  'unsupportedVendorPredicate',
  'vendorNoEquivalent',
  'responseShapeMismatch',
]);

function proofKind(args: BuildEnvelopeArgs): EvidenceKind {
  if (!args.executable) return 'unsupported';
  // An adapter can discover unsatisfiable semantics the planner allowed
  // (e.g. a boolean predicate over target:"code"): unsupported, not partial.
  if (args.diagnostics.some(d => UNSUPPORTED_CODES.has(d.code))) {
    return 'unsupported';
  }
  if (args.approximate || diagnosticsApproximate(args.diagnostics)) {
    return 'candidate';
  }
  if (blocksAnswer(args.diagnostics)) return 'partial';
  if (hasOpenPages(args)) return 'partial';
  return 'proof';
}

function diagnosticsApproximate(diagnostics: OqlDiagnostic[]): boolean {
  return diagnostics.some(d => d.code === 'providerSemanticsApproximate');
}

function hasOpenPages(args: BuildEnvelopeArgs): boolean {
  if (args.pagination?.hasMore) return true;
  // next.page (more result pages) and next.matchPage (more matches within a
  // capped file) both mean the agent has not seen everything → not complete,
  // not proof. They are lossless pagination cursors, not failures.
  if (
    args.next &&
    Object.keys(args.next).some(
      k => k.startsWith('next.page') || k === 'next.matchPage'
    )
  )
    return true;
  return false;
}

function isComplete(args: BuildEnvelopeArgs): boolean {
  if (!args.executable) return false;
  if (hasOpenPages(args)) return false;
  // residual/partial diagnostics that block an answer mean "not complete"
  return !args.diagnostics.some(
    d => d.blocksAnswer || d.code === 'partialResult'
  );
}

export function backendsApproximate(calls: OqlBackendCall[]): boolean {
  return calls.some(c => !c.exact);
}

/** Envelope for a query that failed validation/planning (no execution). */
export function unsupportedEnvelope(
  diagnostics: OqlDiagnostic[],
  plan?: OqlExplainPlan,
  queryId?: string,
  queryIndex?: number,
  next?: OqlResultEnvelope['next']
): OqlResultEnvelope {
  return buildEnvelope({
    queryId,
    queryIndex,
    results: [],
    diagnostics,
    provenance: [],
    executable: false,
    plan,
    ...(next && Object.keys(next).length ? { next } : {}),
  });
}
