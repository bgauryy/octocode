import type { LspReadiness } from '@octocodeai/octocode-engine/lsp/types';

import type {
  LspSemanticEnvelope,
  SemanticEmptyCategory,
  SemanticEmptyState,
} from './semanticTypes.js';

// Empty categories that mean "the server ran but returned nothing" for a
// semantic/relation query — the exact case where a not-yet-indexed server is
// indistinguishable from a true absence. `unsupportedOperation`,
// `symbolNotFound`, and `anchorFailed` are excluded: they reflect a capability
// or resolution problem, not indexing, so an indexing caveat would mislead.
const READINESS_SENSITIVE_EMPTY_CATEGORIES: ReadonlySet<SemanticEmptyCategory> =
  new Set([
    'noLocations',
    'noReferences',
    'noCalls',
    'noTypeHierarchy',
    'noHover',
  ]);

/**
 * The "server returned nothing" category of an envelope, whether the emptiness
 * is expressed as an `empty`-kind payload (definition/hover) or a nested
 * `empty` on an otherwise-shaped payload (references/callers). Returns
 * `undefined` when the envelope carries real results.
 */
export function zeroResultEmptyCategory(
  envelope: LspSemanticEnvelope
): SemanticEmptyCategory | undefined {
  const payload = envelope.payload;
  if (payload.kind === 'empty') return payload.category;
  const nested = (payload as { empty?: SemanticEmptyState }).empty;
  return nested?.category;
}

/**
 * Attach a caveat when a semantic query came back empty AND the language server
 * never confirmed it finished indexing (readiness other than `progressIdle`).
 * Without this, an agent reads a zero as "symbol is unused" when it may just be
 * "server not indexed yet". `readiness === undefined` means the wait was
 * skipped (servers that answer immediately) — those are trusted, no caveat.
 */
export function attachReadinessWarning(
  envelope: LspSemanticEnvelope,
  readiness: LspReadiness | undefined
): LspSemanticEnvelope {
  if (readiness === undefined || readiness === 'progressIdle') return envelope;
  const category = zeroResultEmptyCategory(envelope);
  if (!category || !READINESS_SENSITIVE_EMPTY_CATEGORIES.has(category)) {
    return envelope;
  }
  const warning =
    `Language server did not confirm indexing completion (readiness: ${readiness}) — ` +
    `zero results may mean the project is not yet indexed, not that the symbol is absent. ` +
    `Retry the query, or warm the relevant consumer files first.`;
  return { ...envelope, warnings: [...(envelope.warnings ?? []), warning] };
}
