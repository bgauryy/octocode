/**
 * OQL diagnostic helpers. Diagnostics are first-class typed output, not prose
 * hints. `blocksAnswer:true` marks diagnostics that prevent `proof`.
 */
import type {
  DiagnosticCode,
  OqlContinuation,
  OqlDiagnostic,
  OqlSearchInputV1,
} from './types.js';

/** Diagnostic codes that, when present, prevent `evidence.kind:"proof"`. */
const BLOCKING_CODES = new Set<DiagnosticCode>([
  'invalidQuery',
  'ambiguousSugar',
  'unknownField',
  'unsupportedTarget',
  'unsupportedPredicate',
  'unsupportedBoolean',
  'unsupportedScope',
  'negativeUniverseRequired',
  'residualNotExact',
  'fieldTypeMismatch',
  'requiresMaterialization',
  'materializationNotAllowed',
  'materializationFailed',
  'parserFailed',
  'lspUnavailable',
  'budgetExhausted',
  // truncation means the agent has NOT seen the full result set — it cannot be
  // proof/complete until the remaining matches/content are paged in.
  'matchTruncated',
  'contentTruncated',
]);

const ERROR_CODES = new Set<DiagnosticCode>([
  'invalidQuery',
  'ambiguousSugar',
  'unknownField',
  'unsupportedTarget',
  'unsupportedPredicate',
  'unsupportedBoolean',
  'unsupportedScope',
  'fieldTypeMismatch',
  'materializationNotAllowed',
  'materializationFailed',
]);

export interface DiagnosticOptions {
  queryPath?: string;
  predicateId?: string;
  backend?: string;
  blocksAnswer?: boolean;
  severity?: OqlDiagnostic['severity'];
  repair?: {
    message: string;
    suggestedQuery?: OqlSearchInputV1;
  };
  continuation?: OqlContinuation;
}

export function diagnostic(
  code: DiagnosticCode,
  message: string,
  options: DiagnosticOptions = {}
): OqlDiagnostic {
  const severity =
    options.severity ?? (ERROR_CODES.has(code) ? 'error' : 'warning');
  const blocksAnswer =
    options.blocksAnswer ?? (BLOCKING_CODES.has(code) || severity === 'error');
  return {
    code,
    severity,
    message,
    blocksAnswer,
    ...(options.queryPath ? { queryPath: options.queryPath } : {}),
    ...(options.predicateId ? { predicateId: options.predicateId } : {}),
    ...(options.backend ? { backend: options.backend } : {}),
    ...(options.repair ? { repair: options.repair } : {}),
    ...(options.continuation ? { continuation: options.continuation } : {}),
  };
}

export function blocksAnswer(diagnostics: OqlDiagnostic[]): boolean {
  return diagnostics.some(d => d.blocksAnswer);
}

export function hasErrors(diagnostics: OqlDiagnostic[]): boolean {
  return diagnostics.some(d => d.severity === 'error');
}

/** Error raised when validation/normalization cannot produce a canonical query. */
export class OqlValidationError extends Error {
  readonly diagnostics: OqlDiagnostic[];
  constructor(diagnostics: OqlDiagnostic[]) {
    super(diagnostics.map(d => `${d.code}: ${d.message}`).join('; '));
    this.name = 'OqlValidationError';
    this.diagnostics = diagnostics;
  }
}
