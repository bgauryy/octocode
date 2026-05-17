/**
 * Verbosity Helpers
 *
 * Shared helpers for the per-tool `verbosity` wiring described in
 * `.octocode/rfc/rtk-token-techniques/RFC.md` §4.7. Each tool's handler
 * imports `isUltra` and `ultraDrillBackHint` to keep ultra-payload shaping
 * consistent across tools.
 *
 * Default-invariant: omitted `verbosity`, `"compact"`, and `"verbose"` are
 * treated identically by these helpers — only `"ultra"` triggers the
 * lossy summary path. Tools that opt to specialise `"verbose"` further
 * (e.g., add per-result context) do so locally.
 *
 * @see ../scheme/localSchemaOverlay.ts (`verbosityField`)
 */

import type { Verbosity } from './localSchemaOverlay.js';

/**
 * Returns true when the caller explicitly opted into ultra mode.
 *
 * `undefined`, `"compact"`, and `"verbose"` all return false to preserve
 * the byte-identity contract (RFC §3.1).
 */
export function isUltra(verbosity: Verbosity | undefined): boolean {
  return verbosity === 'ultra';
}

/**
 * Standard drill-back hint pair appended to every ultra response.
 *
 * RFC §4.7.9 acceptance: every ultra payload MUST carry a re-fetch
 * breadcrumb so the agent never lands in a dead end.
 *
 * @param drillbackCall — the exact tool call shape the agent should make
 *   to recover the dropped detail (e.g. `verbosity:"compact"` or
 *   `groupByFile:true`).
 */
export function ultraDrillBackHint(drillbackCall: string): string[] {
  return [
    `verbosity:"ultra" — detail dropped to save tokens.`,
    `Drill-back: ${drillbackCall}`,
  ];
}
