/**
 * Pi-native compaction policy helpers.
 *
 * Pi checks its configured threshold between tool batches and resumes the same
 * agent run. Extensions must not call `ctx.compact()` to emulate that behavior:
 * Pi defines that API as a manual compaction, which aborts the current run and
 * intentionally does not continue it.
 */

export const OCTOCODE_COMPACTION_THRESHOLD = 0.8;

function finiteNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

export function reserveTokensForCompactionThreshold(contextWindow: number): number | undefined {
  if (!finiteNumber(contextWindow) || contextWindow <= 0) return undefined;
  return Math.ceil(contextWindow * (1 - OCTOCODE_COMPACTION_THRESHOLD));
}
