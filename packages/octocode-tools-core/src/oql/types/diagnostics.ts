/**
 * OQL diagnostics — the coded, severity-ranked notices the planner/runner
 * attach to a result envelope (unsupported predicates, truncation, auth,
 * etc.), each optionally carrying a repair suggestion or continuation.
 */

import type { OqlSearchInput } from './query.js';
import type { OqlContinuation } from './envelope.js';

export type DiagnosticCode =
  | 'invalidQuery'
  | 'ambiguousSugar'
  | 'unknownField'
  | 'unsupportedTarget'
  | 'unsupportedPredicate'
  | 'unsupportedBoolean'
  | 'unsupportedScope'
  | 'negativeUniverseRequired'
  | 'residualNotExact'
  | 'fieldTypeMismatch'
  | 'requiresMaterialization'
  | 'vendorNoEquivalent'
  | 'lossyTransform'
  | 'unsupportedVendorPredicate'
  | 'responseShapeMismatch'
  | 'materializationNotAllowed'
  | 'materializationFailed'
  | 'providerUnindexed'
  | 'providerSemanticsApproximate'
  | 'partialResult'
  | 'contentTruncated'
  | 'matchTruncated'
  | 'planTruncated'
  | 'budgetExhausted'
  | 'parserFailed'
  | 'partialParse'
  | 'signatureUnsupported'
  | 'lspUnavailable'
  | 'staleCache'
  | 'sanitized'
  | 'rateLimited'
  | 'authRequired'
  | 'zeroMatches'
  | 'symbolNotFound';

export interface OqlDiagnostic {
  code: DiagnosticCode;
  severity: 'info' | 'warning' | 'error';
  queryPath?: string;
  predicateId?: string;
  backend?: string;
  message: string;
  blocksAnswer: boolean;
  repair?: {
    message: string;
    suggestedQuery?: OqlSearchInput;
  };
  continuation?: OqlContinuation;
}
