// Sandbox bookkeeping that outlives one call: content-addressed helper staging and old run-folder pruning.
import { createHash } from 'crypto';
import {
  copyFileSync,
  existsSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'fs';
import { join } from 'path';

/** Run folders older than this many days are removed when a new sandbox run folder is created. */
export const RUN_DIR_MAX_AGE_DAYS = 3;
/** Same name shape the sandbox gives a run folder and `prune` matches. */
export const RUN_DIR_RE = /^\d{4}-\d{2}-\d{2}-\d{2}-\d{2}-\d{2}(?:-\d+)?$/;
/** Names the stamp of the helper set staged in a helpers folder. */
export const STAMP_FILE = '.stage-hash';

/** Hash of the names and bytes of every helper that exists in `sourceDir`. */
export function helperSetHash(sourceDir: string, names: string[]) {
  const hash = createHash('sha256');
  for (const name of [...names].sort()) {
    const src = join(sourceDir, name);
    if (!existsSync(src)) continue;
    hash.update(`${name}\0`).update(readFileSync(src)).update('\0');
  }
  return hash.digest('hex');
}

/**
 * Stages helpers into `targetDir` once per content hash. Returns the files copied (zero when the folder
 * already holds this exact set). Files land by rename so a concurrent run never reads a half-written one;
 * the stamp is written last, so a folder without a valid stamp is simply restaged.
 */
export function stageHelpers(
  sourceDir: string,
  targetDir: string,
  names: string[],
  warn: (message: string) => void = () => {}
) {
  const hash = helperSetHash(sourceDir, names);
  const stampPath = join(targetDir, STAMP_FILE);
  const present = names.filter(name => existsSync(join(sourceDir, name)));
  try {
    if (
      readFileSync(stampPath, 'utf8') === hash &&
      present.every(name => existsSync(join(targetDir, name)))
    )
      return 0;
  } catch {
    // No usable stamp: stage below.
  }
  // Another package version may share this folder: drop the old stamp so its check cannot match mid-copy.
  rmSync(stampPath, { force: true });
  let copied = 0;
  for (const name of present) {
    const dst = join(targetDir, name);
    const part = `${dst}.${process.pid}.part`;
    try {
      copyFileSync(join(sourceDir, name), part);
      renameSync(part, dst);
      copied++;
    } catch (error) {
      rmSync(part, { force: true });
      warn(
        `could not copy ${name}: ${error instanceof Error ? error.message : String(error)}`
      );
    }
  }
  if (copied === present.length) {
    try {
      writeFileSync(stampPath, hash, { mode: 0o600 });
    } catch {
      // Without a stamp the next call restages; staging itself succeeded.
    }
  }
  return copied;
}

/** A folder's last activity: nested writes do not bump its own mtime, so take its direct entries too. */
function newestMtimeMs(path: string) {
  let newest = statSync(path).mtimeMs;
  for (const entry of readdirSync(path))
    try {
      newest = Math.max(newest, statSync(join(path, entry)).mtimeMs);
    } catch {
      // An entry that vanished mid-scan does not age the folder.
    }
  return newest;
}

/**
 * Removes run folders under `runsDir` last touched more than `maxAgeMs` before `nowMs`, except `keep`.
 * Only timestamp-named folders are touched: session metadata, helpers, scratch and plans stay.
 */
export function pruneOldRunDirs(
  runsDir: string,
  nowMs: number,
  keep: string,
  maxAgeMs = RUN_DIR_MAX_AGE_DAYS * 24 * 60 * 60 * 1000
) {
  const removed: string[] = [];
  let entries: string[];
  try {
    entries = readdirSync(runsDir);
  } catch {
    return removed;
  }
  for (const name of entries) {
    const path = join(runsDir, name);
    if (path === keep || !RUN_DIR_RE.test(name)) continue;
    try {
      if (nowMs - newestMtimeMs(path) <= maxAgeMs) continue;
      rmSync(path, { recursive: true, force: true });
      removed.push(path);
    } catch {
      // Best effort: `prune` reports failures; a call never fails over old folders.
    }
  }
  return removed;
}
