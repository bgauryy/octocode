export type {
  GitHubCacheHitStats,
  PersistedStats,
  PersistedSession,
  SessionStats,
  SessionTotalUsageStats,
  SessionUpdateResult,
  SessionOptions,
  StatsCounterMap,
  ToolCharSavingsStats,
} from './types.js';

export { SESSION_FILE, STATS_FILE } from './storage.js';

export {
  readSession,
  writeSession,
  getOrCreateSession,
  getSessionId,
  deleteSession,
  flushSession,
  flushSessionSync,
} from './storage.js';

export {
  updateSessionStats,
  incrementToolCalls,
  incrementErrors,
  incrementRateLimits,
  incrementRateLimitByProvider,
  incrementToolCharSavings,
  incrementGitHubCacheHits,
  incrementGitHubCacheRateLimits,
  incrementPackageRegistryFailures,
  resetSessionStats,
} from './storage.js';

export { _resetSessionState } from './storage.js';
