/**
 * Session Module
 *
 * Persistent session management for Octocode packages.
 * Stores session identity in ~/.octocode/session.json and statistics in
 * ~/.octocode/stats.json with cross-platform support.
 */

// Types
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

// Storage constants
export { SESSION_FILE, STATS_FILE } from './storage.js';

// Core operations
export {
  readSession,
  writeSession,
  getOrCreateSession,
  getSessionId,
  deleteSession,
  flushSession,
  flushSessionSync,
} from './storage.js';

// Stats operations
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

// Testing utilities
export { _resetSessionState } from './storage.js';
