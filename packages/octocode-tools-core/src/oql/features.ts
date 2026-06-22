/**
 * OQL output-feature capability check.
 *
 * Sibling to `capabilities.ts` (which routes *predicates*); this routes *output
 * features* — the content view and the projected `select` fields — against what
 * the chosen target/backend can actually produce. When a requested feature is
 * not backable, OQL emits a typed diagnostic instead of silently degrading:
 *
 *   - `signatureUnsupported` — a `symbols` content view requested for a target
 *     whose backend has no symbol skeleton (PR/commit/diff content).
 *
 * (Structural metavariable captures ARE returned by the engine and flow into
 * `row.metavars`/`row.metavarRanges`, so no diagnostic is needed there — see
 * gap 12.)
 *
 * One table, one emitter: a new unsupported combination is a row here, never a
 * new conditional scattered across adapters.
 */
import { diagnostic } from './diagnostics.js';
import type { OqlDiagnostic, OqlQuery } from './types.js';

/** Targets whose content is PR/commit/diff text — no symbol-skeleton view. */
const NO_SYMBOLS_VIEW_TARGETS = new Set<OqlQuery['target']>([
  'pullRequests',
  'commits',
  'diff',
]);

/**
 * Diagnose requested-but-unbackable output features. Pure; emits zero or more
 * non-blocking diagnostics. Called by the planner so the limitation rides in the
 * plan and the envelope alike.
 */
export function checkOutputFeatures(query: OqlQuery): OqlDiagnostic[] {
  const out: OqlDiagnostic[] = [];

  // 1. symbols content view on PR/commit/diff content.
  const view = query.fetch?.content?.contentView;
  if (view === 'symbols' && NO_SYMBOLS_VIEW_TARGETS.has(query.target)) {
    out.push(
      diagnostic(
        'signatureUnsupported',
        `A "symbols" content view is not available for target:"${query.target}" (PR/commit/diff content has no symbol skeleton); request "exact" or "compact".`,
        {
          queryPath: 'fetch.content.contentView',
          severity: 'warning',
          blocksAnswer: false,
          repair: {
            message: 'Set fetch.content.contentView to "exact" or "compact".',
          },
        }
      )
    );
  }

  return out;
}
