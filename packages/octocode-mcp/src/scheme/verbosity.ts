/**
 * Verbosity Helpers — canonical 3-tier contract.
 *
 * Each tier moves THREE dimensions in lockstep, identically across every tool:
 *
 *                | fields per result        | page size (result count) | data truncation
 *   -------------|--------------------------|--------------------------|----------------
 *   basic (def.) | all schema fields        | full  (PAGE × 1)         | none
 *   compact      | core fields only         | half  (PAGE ÷ 2)         | none
 *   concise        | identity/count fields    | top   (small cap)        | yes (content/snippets dropped)
 *
 * Rules that hold for ALL tools:
 *  - Omitted ≡ basic.
 *  - Truncation happens ONLY in concise. basic and compact never truncate a
 *    returned value — they differ only in which fields and how many rows.
 *  - The verbosity feature itself emits NO hints. No "detail dropped",
 *    no "drill-back", no tier commentary. Tiers shape data silently; only
 *    genuinely data-bearing hints (pagination cursors, counts) remain.
 *
 * @see ../scheme/localSchemaOverlay.ts (`verbosityField`, `createVerbosityField`)
 */

import type { Verbosity } from './localSchemaOverlay.js';

/**
 * Returns true when the caller asked for concise (lossy summary).
 * Only `"concise"` triggers the trim-content path.
 */
export function isConcise(verbosity: Verbosity | undefined): boolean {
  return verbosity === 'concise';
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
  return v ?? ('basic' as Verbosity);
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
 * Test-only sanity guard. Throws when an concise payload exceeds the budget;
 * useful in snapshot tests to catch contract drift (e.g. a transformer that
 * accidentally keeps the data field around).
 */
export function assertConcisePayload(result: unknown, maxBytes = 2048): void {
  const size = JSON.stringify(result).length;
  if (size > maxBytes) {
    throw new Error(
      `assertConcisePayload: result is ${size} bytes (budget ${maxBytes}). Likely contract violation — concise responses must be tiny.`
    );
  }
}
