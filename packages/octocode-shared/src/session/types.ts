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

export interface PersistedSession {
  version: 1;

  sessionId: string;

  createdAt: string;

  lastActiveAt: string;

  stats: SessionStats;
}

export interface PersistedStats {
  version: 1;

  stats: SessionStats;
}

export interface SessionUpdateResult {
  success: boolean;
  session: PersistedSession | null;
}

export interface SessionOptions {
  forceNew?: boolean;
}
