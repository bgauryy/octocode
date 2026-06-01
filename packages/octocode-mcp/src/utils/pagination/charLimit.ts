import { getConfigSync } from 'octocode-shared';
import { DEFAULT_OUTPUT_CONFIG } from 'octocode-shared';

/**
 * THE single pagination char limit for every tool result.
 *
 * One flow, one number: every paginator (auto-pagination trigger AND page size,
 * per-query / bulk / output-size limits, and the LSP content-budget clamps)
 * reads this. The value lives in exactly one place — the resolved config's
 * `output.pagination.defaultCharLength` (default in octocode-shared) — so there
 * are no alias constants to drift. Larger result sets are reached by paginating
 * (charOffset / page / *PerPage), never by returning a bigger single payload.
 */
export function getOutputCharLimit(): number {
  try {
    return getConfigSync().output.pagination.defaultCharLength;
  } catch {
    return DEFAULT_OUTPUT_CONFIG.pagination.defaultCharLength;
  }
}

/**
 * Hard ceiling for any single aggregated bulk response. Even a count-scaled
 * default must never exceed the documented max, or one response sails past the
 * MCP client's token budget. Mirrors LOCAL_OVERLAY_MAX_CHAR_LENGTH /
 * MAX_DEFAULT_OUTPUT_CHAR_LENGTH used by the paginators.
 */
export const MAX_DEFAULT_OUTPUT_CHAR_LENGTH = 100_000;

/**
 * Default char budget for a BULK response, reserving one base window per query.
 *
 * The single-query default ({@link getOutputCharLimit}) is correct for one
 * query, but applying it to an N-query bulk lets the first large query consume
 * the whole budget and starve its siblings onto page 2 (silent, since they
 * still "exist" behind the cursor). Scaling the auto-pagination default by the
 * query count gives every query a base-sized reserve, so a set of modestly
 * sized siblings all land on page 1. The linear cursor is untouched: this only
 * sizes the default window; explicit responseCharOffset/Length still override,
 * and the result is clamped to the {@link MAX_DEFAULT_OUTPUT_CHAR_LENGTH}
 * ceiling so an oversized bulk still self-paginates.
 */
export function getBulkDefaultCharLength(queryCount: number): number {
  const base = Math.min(
    Math.max(getOutputCharLimit(), 1),
    MAX_DEFAULT_OUTPUT_CHAR_LENGTH
  );
  const count = Math.max(Math.floor(queryCount) || 0, 1);
  return Math.min(base * count, MAX_DEFAULT_OUTPUT_CHAR_LENGTH);
}
