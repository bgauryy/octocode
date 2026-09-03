import {
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { isPersistentStorageEnabled } from '@octocodeai/config';
import {
  evictExpiredClones,
  evictExpiredTrees,
} from './tools/github_clone_repo/cache.js';
import { cleanupStaleCloneArtifacts } from './tools/github_clone_repo/cacheArtifacts.js';
import { sweepDiskCacheNow } from './utils/http/cache.js';

export const CACHE_MAINTENANCE_INTERVAL_MS = 24 * 60 * 60 * 1000;

const RETRY_DELAY_MS = 60 * 1000;
const STALE_LOCK_AGE_MS = 15 * 60 * 1000;
const MARKER_NAME = '.last-cache-maintenance';
const LOCK_NAME = '.cache-maintenance.lock';

let gcTimer: ReturnType<typeof setTimeout> | null = null;
let scheduledHome: string | null = null;
const inFlight = new Map<string, Promise<boolean>>();

function getMaintenanceRoot(octocodeHome: string): string {
  return join(octocodeHome, 'tmp');
}

export function getCacheMaintenanceMarkerPath(octocodeHome: string): string {
  return join(getMaintenanceRoot(octocodeHome), MARKER_NAME);
}

function readLastMaintenance(octocodeHome: string): number | null {
  try {
    const parsed = Number(
      readFileSync(getCacheMaintenanceMarkerPath(octocodeHome), 'utf8')
    );
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
  } catch {
    return null;
  }
}

export function getCacheMaintenanceDelayMs(
  octocodeHome: string,
  now = Date.now()
): number {
  const lastRun = readLastMaintenance(octocodeHome);
  if (lastRun === null) return 0;
  return Math.min(
    CACHE_MAINTENANCE_INTERVAL_MS,
    Math.max(0, lastRun + CACHE_MAINTENANCE_INTERVAL_MS - now)
  );
}

function acquireMaintenanceLock(
  octocodeHome: string,
  now: number
): string | null {
  const root = getMaintenanceRoot(octocodeHome);
  const lock = join(root, LOCK_NAME);
  try {
    mkdirSync(root, { recursive: true, mode: 0o700 });
  } catch {
    return null;
  }

  const tryAcquire = (): boolean => {
    try {
      mkdirSync(lock, { mode: 0o700 });
      return true;
    } catch {
      return false;
    }
  };

  if (tryAcquire()) return lock;

  try {
    if (now - statSync(lock).mtimeMs <= STALE_LOCK_AGE_MS) return null;
    const stale = `${lock}.stale-${process.pid}-${now}`;
    renameSync(lock, stale);
    rmSync(stale, { recursive: true, force: true });
  } catch {
    return null;
  }

  return tryAcquire() ? lock : null;
}

function writeMaintenanceMarker(octocodeHome: string, now: number): void {
  const marker = getCacheMaintenanceMarkerPath(octocodeHome);
  const temp = `${marker}.tmp-${process.pid}-${now}`;
  writeFileSync(temp, String(now), { encoding: 'utf8', mode: 0o600 });
  renameSync(temp, marker);
}

async function runMaintenance(
  octocodeHome: string,
  now: number
): Promise<boolean> {
  if (getCacheMaintenanceDelayMs(octocodeHome, now) > 0) return false;

  const lock = acquireMaintenanceLock(octocodeHome, now);
  if (!lock) return false;

  try {
    // Another process may have completed maintenance while this process waited
    // to acquire a recovered lock.
    if (getCacheMaintenanceDelayMs(octocodeHome, now) > 0) return false;

    try {
      evictExpiredClones(octocodeHome);
      evictExpiredTrees(octocodeHome);
      cleanupStaleCloneArtifacts(octocodeHome, now);
      await sweepDiskCacheNow(octocodeHome, now);
      writeMaintenanceMarker(octocodeHome, now);
      return true;
    } catch {
      // Maintenance is opportunistic and must never block CLI/MCP startup.
      return false;
    }
  } finally {
    try {
      rmSync(lock, { recursive: true, force: true });
    } catch {
      void 0;
    }
  }
}

/** Cheap persisted bootstrap gate shared by short-lived CLI and MCP processes. */
export function runCacheMaintenanceIfDue(
  octocodeHome: string,
  now = Date.now()
): Promise<boolean> {
  if (!isPersistentStorageEnabled()) return Promise.resolve(false);
  const active = inFlight.get(octocodeHome);
  if (active) return active;

  const pending = runMaintenance(octocodeHome, now).finally(() => {
    if (inFlight.get(octocodeHome) === pending) inFlight.delete(octocodeHome);
  });
  inFlight.set(octocodeHome, pending);
  return pending;
}

function scheduleNext(octocodeHome: string): void {
  if (scheduledHome !== octocodeHome) return;
  const delay = Math.max(
    RETRY_DELAY_MS,
    getCacheMaintenanceDelayMs(octocodeHome)
  );
  gcTimer = setTimeout(() => {
    gcTimer = null;
    void runCacheMaintenanceIfDue(octocodeHome).finally(() => {
      scheduleNext(octocodeHome);
    });
  }, delay);
  gcTimer.unref();
}

/** Starts the long-lived MCP cron-style scheduler without keeping Node alive. */
export function startCacheGC(octocodeHome: string): void {
  if (!isPersistentStorageEnabled()) return;
  if (scheduledHome !== null) return;
  scheduledHome = octocodeHome;
  void runCacheMaintenanceIfDue(octocodeHome).finally(() => {
    scheduleNext(octocodeHome);
  });
}

export function stopCacheGC(): void {
  scheduledHome = null;
  if (gcTimer) clearTimeout(gcTimer);
  gcTimer = null;
}
