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

export interface CompactionPolicyEvaluation {
  status: 'ok' | 'disabled' | 'under-reserved';
  contextWindow: number;
  configuredReserveTokens: number;
  recommendedReserveTokens: number;
}

export function evaluateCompactionPolicy(input: {
  contextWindow: number;
  enabled: boolean;
  reserveTokens: number;
}): CompactionPolicyEvaluation | undefined {
  const recommendedReserveTokens = reserveTokensForCompactionThreshold(input.contextWindow);
  const configuredReserveTokens = finiteNumber(input.reserveTokens);
  if (recommendedReserveTokens === undefined || configuredReserveTokens === undefined || configuredReserveTokens < 0) {
    return undefined;
  }
  return {
    status: !input.enabled
      ? 'disabled'
      : configuredReserveTokens < recommendedReserveTokens
        ? 'under-reserved'
        : 'ok',
    contextWindow: input.contextWindow,
    configuredReserveTokens,
    recommendedReserveTokens,
  };
}

function tokenCount(value: number): string {
  return Math.round(value).toLocaleString('en-US');
}

/** Return user-facing guidance only when Pi would compact later than Octocode's 80% target. */
export function formatCompactionPolicyWarning(input: {
  contextWindow: number;
  enabled: boolean;
  reserveTokens: number;
}): string | undefined {
  const policy = evaluateCompactionPolicy(input);
  if (!policy || policy.status === 'ok') return undefined;
  const target = `${tokenCount(policy.recommendedReserveTokens)} tokens (20% of ${tokenCount(policy.contextWindow)})`;
  if (policy.status === 'disabled') {
    return `Pi auto-compaction is disabled. Enable it and set compaction.reserveTokens to at least ${target} so compaction starts near 80%.`;
  }
  return `Pi compaction reserve is ${tokenCount(policy.configuredReserveTokens)} tokens; set compaction.reserveTokens to at least ${target} so compaction starts near 80%.`;
}
