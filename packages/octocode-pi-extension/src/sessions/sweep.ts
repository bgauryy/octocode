import fs from 'node:fs';
import path from 'node:path';
import { SessionManager } from '@earendil-works/pi-coding-agent';
import { runningPid } from '../agentdb/liveness.js';
import { readMeta, writeMeta } from '../agentdb/meta.js';
import { liveJobFiles } from '../files/bash-orphans.js';
import { CLEANUP_DAYS_ENV, envInt } from '../shared/env.js';
import { PID_SESSION_PREFIX, sessionDirName, sessionsRoot } from '../shared/home.js';
import { sweepStaleOutputs } from '../shared/sweep.js';
import { DAY_MS, HOUR_MS } from '../shared/util.js';
import { processAlive } from '../shared/process.js';
import { shotsDir } from '../subagents/screenshots.js';
import type { SessionIndex } from './store.js';

/** The sweep runs at most this often across all sessions (meta `last_sweep`). */
const SWEEP_EVERY_MS = 6 * HOUR_MS;
/**
 * Spilled output, bash logs, idle checkpoints of a kept session, subagent screenshots, and the folders of sessions Pi
 * no longer lists (deleted, a pid folder, a subagent's (its conversation is in its scratch folder), whose spilled files its parent's report may
 * still cite), are removed this many days after they were last written (`OCTOCODE_CLEANUP_DAYS`): one retention, so
 * a path in a kept transcript stays readable as long as the session's own output does. The `archive` folder (compacted
 * history, `shared/archive.ts`) is the searchable twin of the transcript: a kept session keeps it, and it goes with the
 * session's folder, whose age counts its writes.
 */
const OUTPUT_DAYS = 30;

/**
 * Ids of every session Pi has a file for: in its default sessions dir and in each custom session dir a session ran from.
 * Pi lists an unreadable dir as empty, so each custom dir is read first: one that cannot be read (unmounted, removed,
 * no permission) rejects, and the sweep then removes nothing.
 */
async function piSessionIds(dirs: readonly string[]): Promise<Set<string>> {
  const lists = await Promise.all([
    SessionManager.listAll(),
    ...dirs.map(async (dir) => {
      await fs.promises.readdir(dir);
      return SessionManager.listAll(dir);
    }),
  ]);
  return new Set(lists.flat().map((info) => info.id));
}

/** Newest modification time among `files`; 0 when none exists. */
async function newestWrite(files: readonly string[]): Promise<number> {
  const times = await Promise.all(files.map((file) => fs.promises.stat(file).then((stat) => stat.mtimeMs, () => 0)));
  return Math.max(0, ...times);
}

/** Newest write in a session folder: the folder, its kind folders (the archive's included: a compaction is activity) and the checkpoint journal. 0 when it is missing. */
const lastWrite = (dir: string): Promise<number> =>
  newestWrite([dir, path.join(dir, 'output'), path.join(dir, 'bash'), path.join(dir, 'archive'), path.join(dir, 'checkpoints'), path.join(dir, 'checkpoints', 'journal.jsonl')]);

/** Whether a session's checkpoints (journal, blobs) were last written more than `maxAgeMs` ago; false when there are none. */
async function checkpointsIdle(dir: string, maxAgeMs: number, now: number): Promise<boolean> {
  const newest = await newestWrite([dir, path.join(dir, 'journal.jsonl'), path.join(dir, 'blobs')]);
  return newest > 0 && now - newest > maxAgeMs;
}

const remove = (dir: string) => fs.promises.rm(dir, { recursive: true, force: true });

/** A session id's folder name, or none for a malformed id (`sessionDirName` refuses it): one bad row skips, not the sweep. */
function folderName(id: string): string[] {
  try {
    return [sessionDirName(id)];
  } catch {
    return [];
  }
}

/**
 * Removes Octocode data older than `OCTOCODE_CLEANUP_DAYS` (default 30; `0` turns the sweep off), at most once per
 * `SWEEP_EVERY_MS` (unless `force`): the folder and extras of a session Pi no longer lists (a pid folder included)
 * unless its process is still running, and output and bash logs of the sessions kept (not a running job's record and log), checkpoints of a kept session
 * nothing wrote to, and subagent screenshots. The current session and Pi's own session files are never touched. When Pi's
 * listing (its default dir and every recorded custom session dir) fails nothing is removed. Returns the folder names removed. Never rejects.
 */
export async function sweepSessions(
  index: SessionIndex,
  current: string | undefined,
  options: { now?: number; force?: boolean; env?: NodeJS.ProcessEnv; listed?: (dirs: readonly string[]) => Promise<Set<string>> } = {},
): Promise<string[]> {
  const now = options.now ?? Date.now();
  const env = options.env ?? process.env;
  const removed: string[] = [];
  const days = envInt(env, CLEANUP_DAYS_ENV, OUTPUT_DAYS, { min: 0 });
  if (days === 0) return removed;
  const outputMs = days * DAY_MS;
  try {
    const last = Number(readMeta(index.agent.db, 'last_sweep') ?? 0);
    if (!options.force && now - last < SWEEP_EVERY_MS) return removed;
    // Claimed before the work, so concurrent session starts do not sweep twice; a failed listing gives the claim back.
    writeMeta(index.agent.db, 'last_sweep', String(now));
    let ids: Set<string>;
    try {
      ids = await (options.listed ?? piSessionIds)(index.sessionDirs());
    } catch {
      writeMeta(index.agent.db, 'last_sweep', String(last));
      return removed;
    }
    const listed = new Set([...ids].flatMap(folderName));
    const extras = new Map(index.all().flatMap((row) => folderName(row.id).map((name) => [name, row] as const)));
    const currentName = current ? folderName(current)[0] : undefined;
    const keep = new Set<string>(currentName ? [currentName] : []);
    const root = sessionsRoot(env);
    // Missing: no session folders yet.
    const names = await fs.promises.readdir(root).catch((): string[] => []);
    for (const name of names) {
      if (keep.has(name)) continue;
      const dir = path.join(root, name);
      const extra = extras.get(name);
      const pid = name.startsWith(PID_SESSION_PREFIX) ? Number(name.slice(PID_SESSION_PREFIX.length)) : undefined;
      const stale =
        pid !== undefined
          // A `_pid-<n>` folder names its process; no session row (or team row) exists for a session-less process.
          ? pid !== process.pid && !processAlive(pid) && now - (await lastWrite(dir)) > outputMs
          : !listed.has(name) && runningPid(extra?.pid, extra?.pid_started) === undefined && now - (await lastWrite(dir)) > outputMs;
      if (stale) {
        await remove(dir);
        if (extra) index.forget(extra.id);
        removed.push(name);
      } else keep.add(name);
    }
    // Extras of a session with neither a Pi file nor a folder left are dead weight.
    const present = new Set(names);
    for (const [name, extra] of extras) {
      if (!keep.has(name) && !listed.has(name) && !present.has(name) && runningPid(extra.pid, extra.pid_started) === undefined) index.forget(extra.id);
    }
    for (const name of keep) {
      await sweepStaleOutputs(path.join(root, name, 'output'), outputMs, undefined, now);
      // A running job's owner record is written once at its start: its age says nothing, and its log is still open.
      const bash = path.join(root, name, 'bash');
      await sweepStaleOutputs(bash, outputMs, await liveJobFiles(bash), now);
      // A session's checkpoints go whole (a journal pointing at removed blobs only makes a rewind skip files), and only
      // when nothing wrote to them for the period: resuming a session protects it, since it is then the current one.
      const checkpoints = path.join(root, name, 'checkpoints');
      if (name !== currentName && runningPid(extras.get(name)?.pid, extras.get(name)?.pid_started) === undefined && (await checkpointsIdle(checkpoints, outputMs, now))) await remove(checkpoints);
    }
    await sweepStaleOutputs(shotsDir(env), outputMs, undefined, now);
  } catch {
    // A sweep is housekeeping: whatever it could not remove, the next one tries again.
  }
  return removed;
}
