/**
 * Session Types
 *
 * Types for session persistence across octocode packages.
 */

/**
 * Session statistics tracking
 */
export interface ToolCharSavingsStats {
  rawChars: number;
  responseChars: number;
  savedChars: number;
  calls: number;
}

export interface GitHubCacheHitStats {
  hits: Record<string, number>;
  rateLimits: number;
}

export type StatsCounterMap = Record<string, number>;

export interface SessionTotalUsageStats {
  toolCalls: number;
  errors: number;
  rateLimits: number;
  rateLimitsByProvider: StatsCounterMap;
  rawChars: number;
  responseChars: number;
  savedChars: number;
  charSavingsCalls: number;
  githubCacheHits: number;
  githubCacheRateLimits: number;
  packageRegistryFailures: number;
  packageRegistryFailuresByRegistry: StatsCounterMap;
}

export interface SessionStats {
  toolCalls: number;
  errors: number;
  rateLimits: number;
  rateLimitsByProvider?: StatsCounterMap;
  charsSavedByTool?: Record<string, ToolCharSavingsStats>;
  githubCacheHits?: GitHubCacheHitStats;
  packageRegistryFailures?: StatsCounterMap;
  totalUsage?: SessionTotalUsageStats;
}

/**
 * Session data kept in memory. Identity/timestamps are stored in
 * ~/.octocode/session.json and stats are stored in ~/.octocode/stats.json.
 */
export interface PersistedSession {
  /** Schema version for future migrations */
  version: 1;
  /** Unique session identifier (UUID) */
  sessionId: string;
  /** When the session was first created */
  createdAt: string;
  /** Last time the session was active (updated on init) */
  lastActiveAt: string;
  /** Cumulative session statistics */
  stats: SessionStats;
}

/**
 * Persisted stats data stored in ~/.octocode/stats.json
 */
export interface PersistedStats {
  /** Schema version for future migrations */
  version: 1;
  /** Cumulative statistics */
  stats: SessionStats;
}

/**
 * Result from updating session stats
 */
export interface SessionUpdateResult {
  success: boolean;
  session: PersistedSession | null;
}

/**
 * Options for creating/loading a session
 */
export interface SessionOptions {
  /** Force create a new session even if one exists */
  forceNew?: boolean;
}
