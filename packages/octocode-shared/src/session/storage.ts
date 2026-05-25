/**
 * Session Storage
 *
 * Persistent session storage in ~/.octocode/session.json and stats in
 * ~/.octocode/stats.json
 * Cross-platform support for Windows, Linux, and macOS.
 *
 * Uses batch saving to reduce disk I/O - changes are buffered in memory
 * and flushed to disk every 60 seconds or on process exit.
 */

import { randomUUID } from 'node:crypto';
import type {
  GitHubCacheHitStats,
  PersistedSession,
  SessionStats,
  SessionUpdateResult,
  SessionOptions,
  StatsCounterMap,
  ToolCharSavingsStats,
} from './types.js';
import { deleteSessionFile } from './sessionDiskIO.js';
import { createDefaultStats, withDerivedUsageTotals } from './statsDefaults.js';
import {
  readSession as readSessionFromCache,
  writeSession as writeSessionToCache,
  flushSession as flushSessionFromCache,
  flushSessionSync as flushSessionSyncFromCache,
  clearCache,
  stopFlushTimer,
  unregisterExitHandlers,
  resetCacheState,
} from './sessionCache.js';

// Current schema version
const CURRENT_VERSION = 1 as const;

function normalizeStats(stats: SessionStats): Required<SessionStats> {
  return withDerivedUsageTotals(stats) as Required<SessionStats>;
}

function mergeCharSavingsStats(
  current: Record<string, ToolCharSavingsStats>,
  updates?: Record<string, ToolCharSavingsStats>
): Record<string, ToolCharSavingsStats> {
  if (!updates) return current;

  const merged = { ...current };
  for (const [toolName, update] of Object.entries(updates)) {
    const existing = merged[toolName] ?? {
      rawChars: 0,
      responseChars: 0,
      savedChars: 0,
      calls: 0,
    };
    merged[toolName] = {
      rawChars: existing.rawChars + update.rawChars,
      responseChars: existing.responseChars + update.responseChars,
      savedChars: existing.savedChars + update.savedChars,
      calls: existing.calls + update.calls,
    };
  }
  return merged;
}

function mergeCounterMapStats(
  current: StatsCounterMap,
  updates?: StatsCounterMap
): StatsCounterMap {
  if (!updates) return current;

  const merged = { ...current };
  for (const [name, count] of Object.entries(updates)) {
    merged[name] = (merged[name] ?? 0) + count;
  }
  return merged;
}

function mergeGitHubCacheHitStats(
  current: GitHubCacheHitStats,
  updates?: GitHubCacheHitStats
): GitHubCacheHitStats {
  if (!updates) return current;

  const hits = { ...current.hits };
  for (const [cacheName, count] of Object.entries(updates.hits ?? {})) {
    hits[cacheName] = (hits[cacheName] ?? 0) + count;
  }

  return {
    hits,
    rateLimits: current.rateLimits + (updates.rateLimits ?? 0),
  };
}

/**
 * Create a new session with default values
 */
function createNewSession(): PersistedSession {
  const now = new Date().toISOString();
  return {
    version: CURRENT_VERSION,
    sessionId: randomUUID(),
    createdAt: now,
    lastActiveAt: now,
    stats: createDefaultStats(),
  };
}

/**
 * Read session (from cache or disk)
 * @returns The persisted session or null if not found/invalid
 */
export function readSession(): PersistedSession | null {
  return readSessionFromCache();
}

/**
 * Write session (to cache, batched to disk)
 * Changes are buffered and flushed every 60 seconds or on process exit.
 */
export function writeSession(session: PersistedSession): void {
  writeSessionToCache(session);
}

/**
 * Flush session to disk immediately
 * Use this when you need to ensure data is persisted (e.g., before critical operations)
 */
export function flushSession(): void {
  flushSessionFromCache();
}

/**
 * Flush session to disk synchronously (for exit handlers)
 */
export function flushSessionSync(): void {
  flushSessionSyncFromCache();
}

/**
 * Get or create a session
 * - If session exists and is valid, update lastActiveAt and return it
 * - If session doesn't exist or is invalid, create a new one
 *
 * @param options - Session options (forceNew to create fresh session)
 * @returns The persisted session
 */
export function getOrCreateSession(options?: SessionOptions): PersistedSession {
  // Force new session if requested
  if (options?.forceNew) {
    const newSession = createNewSession();
    writeSessionToCache(newSession);
    // Flush immediately for new sessions to ensure ID is persisted
    flushSessionFromCache();
    return newSession;
  }

  // Try to load existing session (from cache or disk)
  const existingSession = readSessionFromCache();

  if (existingSession) {
    const updatedSession: PersistedSession = {
      ...existingSession,
      lastActiveAt: new Date().toISOString(),
    };
    writeSessionToCache(updatedSession);
    // Flush immediately on first load to persist lastActiveAt
    flushSessionFromCache();
    return updatedSession;
  }

  const newSession = createNewSession();
  writeSessionToCache(newSession);
  // Flush immediately for new sessions to ensure ID is persisted
  flushSessionFromCache();
  return newSession;
}

/**
 * Get the current session ID without modifying the session
 * @returns The session ID or null if no session exists
 */
export function getSessionId(): string | null {
  const session = readSessionFromCache();
  return session?.sessionId ?? null;
}

/**
 * Update session statistics
 * Increments the specified stat counters (batched to disk)
 *
 * @param updates - Partial stats to increment
 * @returns Result with success status and updated session
 */
export function updateSessionStats(
  updates: Partial<SessionStats>
): SessionUpdateResult {
  const session = readSessionFromCache();

  if (!session) {
    return { success: false, session: null };
  }

  const currentStats = normalizeStats(session.stats);

  // Increment stats
  const updatedStats: SessionStats = withDerivedUsageTotals({
    toolCalls: currentStats.toolCalls + (updates.toolCalls ?? 0),
    errors: currentStats.errors + (updates.errors ?? 0),
    rateLimits: currentStats.rateLimits + (updates.rateLimits ?? 0),
    rateLimitsByProvider: mergeCounterMapStats(
      currentStats.rateLimitsByProvider,
      updates.rateLimitsByProvider
    ),
    charsSavedByTool: mergeCharSavingsStats(
      currentStats.charsSavedByTool,
      updates.charsSavedByTool
    ),
    githubCacheHits: mergeGitHubCacheHitStats(
      currentStats.githubCacheHits,
      updates.githubCacheHits
    ),
    packageRegistryFailures: mergeCounterMapStats(
      currentStats.packageRegistryFailures,
      updates.packageRegistryFailures
    ),
  });

  const updatedSession: PersistedSession = {
    ...session,
    lastActiveAt: new Date().toISOString(),
    stats: updatedStats,
  };

  // Write to cache (batched to disk every 60s)
  writeSessionToCache(updatedSession);
  return { success: true, session: updatedSession };
}

/**
 * Increment tool call counter (batched)
 */
export function incrementToolCalls(count: number = 1): SessionUpdateResult {
  return updateSessionStats({ toolCalls: count });
}

/**
 * Increment prompt call counter (batched)
 */
/**
 * Increment error counter (batched)
 */
export function incrementErrors(count: number = 1): SessionUpdateResult {
  return updateSessionStats({ errors: count });
}

/**
 * Increment rate limit counter (batched)
 */
export function incrementRateLimits(count: number = 1): SessionUpdateResult {
  return updateSessionStats({ rateLimits: count });
}

/**
 * Increment provider rate-limit counters. This updates both the global
 * rate-limit total and the per-provider breakdown.
 */
export function incrementRateLimitByProvider(
  provider: string,
  count: number = 1
): SessionUpdateResult {
  return updateSessionStats({
    rateLimits: count,
    rateLimitsByProvider: {
      [provider]: count,
    },
  });
}

/**
 * Increment per-tool character savings statistics.
 */
export function incrementToolCharSavings(
  toolName: string,
  rawChars: number,
  responseChars: number
): SessionUpdateResult {
  const safeRawChars = Number.isFinite(rawChars) ? Math.max(0, rawChars) : 0;
  const safeResponseChars = Number.isFinite(responseChars)
    ? Math.max(0, responseChars)
    : 0;

  return updateSessionStats({
    charsSavedByTool: {
      [toolName]: {
        rawChars: safeRawChars,
        responseChars: safeResponseChars,
        savedChars: Math.max(0, safeRawChars - safeResponseChars),
        calls: 1,
      },
    },
  });
}

/**
 * Increment the hit counter for a GitHub cache bucket.
 */
export function incrementGitHubCacheHits(
  cacheName: string,
  count: number = 1
): SessionUpdateResult {
  return updateSessionStats({
    githubCacheHits: {
      hits: {
        [cacheName]: count,
      },
      rateLimits: 0,
    },
  });
}

/**
 * Increment the GitHub rate limit counter stored next to cache hit stats.
 */
export function incrementGitHubCacheRateLimits(
  count: number = 1
): SessionUpdateResult {
  return updateSessionStats({
    githubCacheHits: {
      hits: {},
      rateLimits: count,
    },
  });
}

/**
 * Increment package registry failure counters. These are intentionally
 * separate from provider API rate limits.
 */
export function incrementPackageRegistryFailures(
  registry: string,
  count: number = 1
): SessionUpdateResult {
  return updateSessionStats({
    packageRegistryFailures: {
      [registry]: count,
    },
  });
}

/**
 * Reset session statistics to zero
 */
export function resetSessionStats(): SessionUpdateResult {
  const session = readSessionFromCache();

  if (!session) {
    return { success: false, session: null };
  }

  const updatedSession: PersistedSession = {
    ...session,
    lastActiveAt: new Date().toISOString(),
    stats: createDefaultStats(),
  };

  writeSessionToCache(updatedSession);
  return { success: true, session: updatedSession };
}

/**
 * Delete the current session (for testing or cleanup)
 * Also cleans up exit handlers to avoid listener warnings in tests
 * @returns true if session was deleted, false if it didn't exist
 */
export function deleteSession(): boolean {
  // Clear cache
  clearCache();

  // Stop flush timer and unregister handlers
  stopFlushTimer();
  unregisterExitHandlers();

  return deleteSessionFile();
}

/**
 * Reset internal state (for testing)
 * This properly cleans up all listeners to avoid MaxListenersExceededWarning
 * @internal
 */
export function _resetSessionState(): void {
  resetCacheState();
}

export { SESSION_FILE, STATS_FILE } from './sessionDiskIO.js';
