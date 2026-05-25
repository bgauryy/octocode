import type { SessionStats, SessionTotalUsageStats } from './types.js';

function sumCounterMap(counters: Record<string, number>): number {
  return Object.values(counters).reduce((sum, count) => sum + count, 0);
}

export function calculateTotalUsageStats(
  stats: SessionStats
): SessionTotalUsageStats {
  const charsSavedByTool = stats.charsSavedByTool ?? {};
  const githubCacheHits = stats.githubCacheHits ?? {
    hits: {},
    rateLimits: 0,
  };
  const rateLimitsByProvider = stats.rateLimitsByProvider ?? {};
  const packageRegistryFailures = stats.packageRegistryFailures ?? {};

  const charTotals = Object.values(charsSavedByTool).reduce(
    (totals, toolStats) => ({
      rawChars: totals.rawChars + toolStats.rawChars,
      responseChars: totals.responseChars + toolStats.responseChars,
      savedChars: totals.savedChars + toolStats.savedChars,
      charSavingsCalls: totals.charSavingsCalls + toolStats.calls,
    }),
    {
      rawChars: 0,
      responseChars: 0,
      savedChars: 0,
      charSavingsCalls: 0,
    }
  );

  return {
    toolCalls: stats.toolCalls,
    errors: stats.errors,
    rateLimits: stats.rateLimits,
    rateLimitsByProvider,
    ...charTotals,
    githubCacheHits: sumCounterMap(githubCacheHits.hits),
    githubCacheRateLimits: githubCacheHits.rateLimits,
    packageRegistryFailures: sumCounterMap(packageRegistryFailures),
    packageRegistryFailuresByRegistry: packageRegistryFailures,
  };
}

export function withDerivedUsageTotals(stats: SessionStats): SessionStats {
  const normalized: SessionStats = {
    ...stats,
    rateLimitsByProvider: stats.rateLimitsByProvider ?? {},
    charsSavedByTool: stats.charsSavedByTool ?? {},
    githubCacheHits: stats.githubCacheHits ?? { hits: {}, rateLimits: 0 },
    packageRegistryFailures: stats.packageRegistryFailures ?? {},
  };

  return {
    ...normalized,
    totalUsage: calculateTotalUsageStats(normalized),
  };
}

/**
 * Default persisted statistics.
 */
export function createDefaultStats(): SessionStats {
  return withDerivedUsageTotals({
    toolCalls: 0,
    errors: 0,
    rateLimits: 0,
    rateLimitsByProvider: {},
    charsSavedByTool: {},
    githubCacheHits: {
      hits: {},
      rateLimits: 0,
    },
    packageRegistryFailures: {},
  });
}
