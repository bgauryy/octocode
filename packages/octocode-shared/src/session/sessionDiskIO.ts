/**
 * Session Disk I/O
 *
 * File read/write/flush operations for session and stats storage.
 * Uses atomic writes (temp file + rename) to prevent corruption.
 */

import {
  existsSync,
  readFileSync,
  writeFileSync,
  unlinkSync,
  renameSync,
} from 'node:fs';
import { ensureOctocodeDir } from '../credentials/storage.js';
import { paths } from '../paths.js';
import { createLogger } from '../logger/index.js';
import {
  PersistedSessionSchema,
  PersistedStatsSchema,
  SessionStatsSchema,
} from './schemas.js';
import { createDefaultStats, withDerivedUsageTotals } from './statsDefaults.js';
import type { PersistedSession, SessionStats } from './types.js';

const logger = createLogger('session');

// Storage constants
export const SESSION_FILE = paths.session;
export const STATS_FILE = paths.stats;

type SessionDiskPayload = Omit<PersistedSession, 'stats'> & {
  stats?: SessionStats;
};

function writeJsonAtomic(file: string, data: unknown): void {
  const tempFile = `${file}.tmp`;

  writeFileSync(tempFile, JSON.stringify(data, null, 2), {
    mode: 0o600,
  });

  renameSync(tempFile, file);
}

function toSessionDiskPayload(session: PersistedSession): SessionDiskPayload {
  const { stats: _stats, ...sessionWithoutStats } = session;
  return sessionWithoutStats;
}

function parseStatsFileContent(content: string): SessionStats | null {
  const parsed = JSON.parse(content);

  const persistedStatsResult = PersistedStatsSchema.safeParse(parsed);
  if (persistedStatsResult.success) {
    return withDerivedUsageTotals(persistedStatsResult.data.stats);
  }

  const rawStatsResult = SessionStatsSchema.safeParse(parsed);
  if (rawStatsResult.success) {
    return withDerivedUsageTotals(rawStatsResult.data);
  }

  return null;
}

function readStatsFromDisk(fallbackStats?: SessionStats): SessionStats {
  const fallback = fallbackStats
    ? withDerivedUsageTotals(fallbackStats)
    : createDefaultStats();

  if (!existsSync(STATS_FILE)) {
    return fallback;
  }

  try {
    const content = readFileSync(STATS_FILE, 'utf8');
    const stats = parseStatsFileContent(content);
    if (!stats) {
      logger.warn('Stats file has invalid format', { file: STATS_FILE });
      return fallback;
    }
    return stats;
  } catch {
    return fallback;
  }
}

/**
 * Write session directly to disk (internal)
 * Uses atomic write (temp file + rename) to prevent corruption on crash
 */
export function writeSessionToDisk(session: PersistedSession): void {
  ensureOctocodeDir();

  writeJsonAtomic(STATS_FILE, {
    version: session.version,
    stats: withDerivedUsageTotals(session.stats),
  });
  writeJsonAtomic(SESSION_FILE, toSessionDiskPayload(session));
}

/**
 * Read session directly from disk (internal)
 */
export function readSessionFromDisk(): PersistedSession | null {
  if (!existsSync(SESSION_FILE)) {
    return null;
  }

  try {
    const content = readFileSync(SESSION_FILE, 'utf8');
    const parsed = JSON.parse(content);
    const result = PersistedSessionSchema.safeParse(parsed);
    if (!result.success) {
      logger.warn('Session file has invalid format', { file: SESSION_FILE });
      return null;
    }

    return {
      ...result.data,
      stats: readStatsFromDisk(result.data.stats),
    };
  } catch {
    // Invalid JSON or read error - return null to create new session
    return null;
  }
}

/**
 * Delete session file from disk
 */
export function deleteSessionFile(): boolean {
  let deleted = false;

  for (const file of [SESSION_FILE, STATS_FILE]) {
    if (!existsSync(file)) continue;

    try {
      unlinkSync(file);
      deleted = true;
    } catch {
      return false;
    }
  }

  return deleted;
}
