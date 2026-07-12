import {
  existsSync,
  readFileSync,
  writeFileSync,
  unlinkSync,
  renameSync,
} from 'node:fs';
import { isStatsEnabled } from '@octocodeai/config';
import { ensureOctocodeDir } from '../credentials/storage.js';
import { paths } from '../paths.js';
import {
  PersistedSessionSchema,
  PersistedStatsSchema,
  SessionStatsSchema,
} from './schemas.js';
import { createDefaultStats, withDerivedUsageTotals } from './statsDefaults.js';
import type { PersistedSession, SessionStats } from './types.js';

export const SESSION_FILE = paths.session;
export const STATS_FILE = paths.stats;

function writeJsonAtomic(file: string, data: unknown): void {
  const tempFile = `${file}.tmp`;
  writeFileSync(tempFile, JSON.stringify(data, null, 2), { mode: 0o600 });
  renameSync(tempFile, file);
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

function readStatsFromDisk(): SessionStats {
  if (!existsSync(STATS_FILE)) {
    return createDefaultStats();
  }
  try {
    const content = readFileSync(STATS_FILE, 'utf8');
    return parseStatsFileContent(content) ?? createDefaultStats();
  } catch {
    return createDefaultStats();
  }
}

export function writeSessionToDisk(session: PersistedSession): void {
  ensureOctocodeDir();

  // stats.json is opt-in — set OCTOCODE_ENABLE_STATS=1 to persist tool usage stats.
  if (isStatsEnabled()) {
    writeJsonAtomic(STATS_FILE, {
      version: session.version,
      stats: withDerivedUsageTotals(session.stats),
    });
  }

  // session.json holds identity only — no stats.
  const { stats: _stats, ...sessionWithoutStats } = session;
  writeJsonAtomic(SESSION_FILE, sessionWithoutStats);
}

export function readSessionFromDisk(): PersistedSession | null {
  if (!existsSync(SESSION_FILE)) {
    return null;
  }

  try {
    const content = readFileSync(SESSION_FILE, 'utf8');
    const parsed = JSON.parse(content);
    const result = PersistedSessionSchema.safeParse(parsed);
    if (!result.success) {
      return null;
    }

    return {
      ...result.data,
      stats: isStatsEnabled() ? readStatsFromDisk() : createDefaultStats(),
    };
  } catch {
    return null;
  }
}

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
