/**
 * Verbosity Helpers
 *
 * Mirrors the canonical contract defined in
 * octocode-core/src/resources/global.ts `baseSchema.verbosity`:
 *
 *   basic (default) — full content + full hints/metadata.
 *   compact         — full content, TRIMMED hints/metadata.
 *   ultra           — counts/top refs only, snippets dropped.
 *
 * Omitted ≡ `basic`. Use compact when the data is enough; ultra for broad
 * probes when counts/top locations suffice.
 *
 * @see ../scheme/localSchemaOverlay.ts (`verbosityField`, `createVerbosityField`)
 */

import type { Verbosity } from './localSchemaOverlay.js';

/**
 * Returns true when the caller asked for ultra (lossy summary).
 * Only `"ultra"` triggers the trim-content path.
 */
export function isUltra(verbosity: Verbosity | undefined): boolean {
  return verbosity === 'ultra';
}

/**
 * Returns true when the caller asked for compact (full content, trimmed hints).
 * Used by tools to drop expensive hints/metadata while keeping the data
 * payload intact.
 */
export function isCompact(verbosity: Verbosity | undefined): boolean {
  return verbosity === 'compact';
}

/**
 * Returns true when the caller is in basic mode (or omitted verbosity).
 * Basic is the default — full content + full hints.
 */
export function isBasic(verbosity: Verbosity | undefined): boolean {
  return verbosity === undefined || verbosity === 'basic';
}

/**
 * Centralises the "omitted ≡ basic" contract. Pass through at the execution
 * boundary if you want to thread the resolved tier around explicitly; the
 * helpers above already handle `undefined` correctly so this is optional.
 */
export function normalizeVerbosity(v: Verbosity | undefined): Verbosity {
  return v ?? 'basic';
}

/**
 * Trim advisory hints under `compact`. Returns hints array with advisory-only
 * entries filtered out and the remainder capped to `cap` items.
 *
 * Tools pass a per-tool predicate identifying their advisory strings (recovery
 * prose, large-payload warnings, fallback-mode notes). Data-bearing hints
 * (pagination cursors, drill-back breadcrumbs) are not advisory.
 */
export function compactTrimHints(
  hints: string[] | readonly string[] | undefined,
  isAdvisory: (h: string) => boolean,
  cap = 2
): string[] | undefined {
  if (!hints || hints.length === 0) return hints as string[] | undefined;
  const kept: string[] = [];
  for (const h of hints) {
    if (!isAdvisory(h)) kept.push(h);
    if (kept.length >= cap) break;
  }
  return kept;
}

/**
 * Build a case-insensitive substring-OR predicate over a list of marker
 * phrases. Use instead of authoring rigid `^prefix` regex — substring
 * matching tolerates trivial wording shifts, surrounding whitespace, and
 * `[prefix] ...` wrappers.
 *
 * Example:
 *   const isAdvisory = makeAdvisoryPredicate([
 *     'large result set',
 *     'pivot terms',
 *     'lspMode',
 *   ]);
 *   compactTrimHints(hints, isAdvisory, 2);
 *
 * Pick markers that are distinctive enough to avoid false positives on
 * data-bearing hints (pagination cursors, drill-back lines, summaries).
 */
export function makeAdvisoryPredicate(
  markers: readonly string[]
): (h: string) => boolean {
  const lc = markers.map(m => m.toLowerCase());
  return (h: string): boolean => {
    if (!h) return false;
    const hl = h.toLowerCase();
    for (const m of lc) {
      if (hl.includes(m)) return true;
    }
    return false;
  };
}

/**
 * Standard drill-back hint pair appended to every ultra response.
 *
 * Every ultra payload MUST carry a re-fetch breadcrumb so the agent never
 * lands in a dead end.
 *
 * @param drillbackCall — the exact tool call shape the agent should make
 *   to recover the dropped detail (e.g. `verbosity:"basic"` or
 *   `groupByFile:true`).
 */
export function ultraDrillBackHint(drillbackCall: string): string[] {
  return [
    `verbosity:"ultra" — detail dropped to save tokens.`,
    `Drill-back: ${drillbackCall}`,
  ];
}

/**
 * Test-only sanity guard. Throws when an ultra payload exceeds the budget;
 * useful in snapshot tests to catch contract drift (e.g. a transformer that
 * accidentally keeps the data field around).
 */
export function assertUltraPayload(result: unknown, maxBytes = 2048): void {
  const size = JSON.stringify(result).length;
  if (size > maxBytes) {
    throw new Error(
      `assertUltraPayload: result is ${size} bytes (budget ${maxBytes}). Likely contract violation — ultra responses must be tiny.`
    );
  }
}
