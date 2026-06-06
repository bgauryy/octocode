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

export function readSession(): PersistedSession | null {
  return readSessionFromCache();
}

export function writeSession(session: PersistedSession): void {
  writeSessionToCache(session);
}

export function flushSession(): void {
  flushSessionFromCache();
}

export function flushSessionSync(): void {
  flushSessionSyncFromCache();
}

export function getOrCreateSession(options?: SessionOptions): PersistedSession {
  if (options?.forceNew) {
    const newSession = createNewSession();
    writeSessionToCache(newSession);
    flushSessionFromCache();
    return newSession;
  }

  const existingSession = readSessionFromCache();

  if (existingSession) {
    const updatedSession: PersistedSession = {
      ...existingSession,
      lastActiveAt: new Date().toISOString(),
    };
    writeSessionToCache(updatedSession);
    flushSessionFromCache();
    return updatedSession;
  }

  const newSession = createNewSession();
  writeSessionToCache(newSession);
  flushSessionFromCache();
  return newSession;
}

export function getSessionId(): string | null {
  const session = readSessionFromCache();
  return session?.sessionId ?? null;
}

export function updateSessionStats(
  updates: Partial<SessionStats>
): SessionUpdateResult {
  const session = readSessionFromCache();

  if (!session) {
    return { success: false, session: null };
  }

  const currentStats = normalizeStats(session.stats);

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

  writeSessionToCache(updatedSession);
  return { success: true, session: updatedSession };
}

export function incrementToolCalls(count: number = 1): SessionUpdateResult {
  return updateSessionStats({ toolCalls: count });
}

export function incrementErrors(count: number = 1): SessionUpdateResult {
  return updateSessionStats({ errors: count });
}

export function incrementRateLimits(count: number = 1): SessionUpdateResult {
  return updateSessionStats({ rateLimits: count });
}

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

export function deleteSession(): boolean {
  clearCache();

  stopFlushTimer();
  unregisterExitHandlers();

  return deleteSessionFile();
}

export function _resetSessionState(): void {
  resetCacheState();
}

export { SESSION_FILE, STATS_FILE } from './sessionDiskIO.js';
