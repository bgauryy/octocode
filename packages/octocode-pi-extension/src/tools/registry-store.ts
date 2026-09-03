/**
 * registry-store — shared primitives for the filesystem-backed dynamic registries
 * (dynamic-tools and dynamic-skills). Both persist a JSON index under a directory
 * shared by many parallel agents and need the same atomic write, corruption-safe
 * read, cross-process mutex, and keyword tokenization. This module is the single
 * source of truth for that machinery so a fix (e.g. a stale-lock reclaim change)
 * lands in one place instead of two hand-copied blocks.
 */

import fs from 'node:fs';
import path from 'node:path';
import { ensurePrivateDirectory, hardenPrivateFile, PRIVATE_FILE_MODE } from '@octocodeai/octocode-awareness/mcp-state';

/** A candidate needs at least this many overlapping keyword tokens to count as a match. */
export const KEYWORD_MATCH_THRESHOLD = 2;

const LOCK_TIMEOUT_MS = 5_000;
const LOCK_STALE_MS = 30_000;

/** Lowercase alphanumeric token set, used for keyword-overlap fallback resolution. */
export function tokenize(s: string): Set<string> {
  return new Set(s.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean));
}

/**
 * Atomic JSON write on the same filesystem (write-temp + rename) so a concurrent
 * reader never observes a partial/torn file.
 */
export function writeJsonAtomic(filePath: string, value: unknown): void {
  ensurePrivateDirectory(path.dirname(filePath));
  hardenPrivateFile(filePath);
  const tmp = `${filePath}.${process.pid}.${Date.now()}.tmp`;
  fs.writeFileSync(tmp, JSON.stringify(value, null, 2), { encoding: 'utf8', mode: PRIVATE_FILE_MODE, flag: 'wx' });
  fs.renameSync(tmp, filePath);
  hardenPrivateFile(filePath);
}

/**
 * Read+parse JSON, returning `fallback` on missing/corrupt/invalid content rather
 * than throwing — a corrupt index must never crash the agent. `isValid` lets the
 * caller reject a structurally wrong (but parseable) shape.
 */
export function readJsonSafe<T>(filePath: string, fallback: T, isValid?: (raw: unknown) => boolean): T {
  try {
    const raw = JSON.parse(fs.readFileSync(filePath, 'utf8')) as unknown;
    if (!raw || typeof raw !== 'object') return fallback;
    if (isValid && !isValid(raw)) return fallback;
    return raw as T;
  } catch {
    return fallback;
  }
}

/**
 * Cross-process mutex around a read-modify-write of a shared registry directory.
 * `fs.mkdirSync` is atomic — it fails if the lock dir already exists — so it is a
 * correct inter-process lock. NOT reentrant: callers must not nest. A lock older
 * than LOCK_STALE_MS is treated as abandoned (crashed holder) and reclaimed so a
 * dead process can never wedge the registry.
 *
 * @param dir       registry root (created if missing)
 * @param lockName  lock directory name (e.g. `.index.lock`)
 * @param label     used in the timeout error message
 */
export function withRegistryLock<T>(dir: string, lockName: string, label: string, fn: () => T): T {
  ensurePrivateDirectory(dir);
  const lock = path.join(dir, lockName);
  const start = Date.now();
  for (;;) {
    try {
      fs.mkdirSync(lock, { mode: 0o700 });
      break;
    } catch {
      try {
        if (Date.now() - fs.statSync(lock).mtimeMs > LOCK_STALE_MS) {
          // Atomic steal: rename() lets exactly ONE racer move the stale dir; the
          // rest get ENOENT and retry. A bare rmdir here is unsafe — two processes
          // that both saw the lock as stale would each rmdir, and the second would
          // delete the FIRST's freshly re-created lock, admitting both into fn().
          const tomb = `${lock}.stale-${process.pid}-${start}`;
          fs.renameSync(lock, tomb);
          // Re-confirm on the moved inode: if it was actually fresh (a holder
          // grabbed it between our stat and rename), put it back rather than
          // stealing a live lock; otherwise drop the tombstone.
          let stillStale = true;
          try {
            stillStale = Date.now() - fs.statSync(tomb).mtimeMs > LOCK_STALE_MS;
          } catch {
            stillStale = true;
          }
          if (stillStale) {
            try { fs.rmdirSync(tomb); } catch { /* already gone */ }
          } else {
            try { fs.renameSync(tomb, lock); } catch { try { fs.rmdirSync(tomb); } catch { /* orphan avoided */ } }
          }
          continue;
        }
      } catch {
        // lock vanished / was stolen by another racer between mkdir and reclaim → retry
      }
      if (Date.now() - start > LOCK_TIMEOUT_MS) throw new Error(`${label} registry lock timeout`);
      // Block ~15ms without burning the CPU / event loop. Atomics.wait on a
      // throwaway shared buffer is a synchronous sleep (index ops are sub-ms, so
      // real contention is rare and brief).
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 15);
    }
  }
  try {
    return fn();
  } finally {
    try {
      fs.rmdirSync(lock);
    } catch {
      // already released
    }
  }
}
