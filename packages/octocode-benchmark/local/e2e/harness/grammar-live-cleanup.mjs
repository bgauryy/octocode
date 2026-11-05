// Who still uses a suite's private directory, and removing it without losing the run.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const capture = (command, args) =>
  spawnSync(command, args, { encoding: 'utf8', maxBuffer: 256 << 20 }).stdout ??
  '';

/** `ps -axo pid=,ppid=,etime=,command=` rows. */
export function parseProcessTable(text) {
  return text.split('\n').flatMap(line => {
    const match = /^\s*(\d+)\s+(\d+)\s+(\S+)\s+(.*)$/u.exec(line);
    return match
      ? [
          {
            pid: Number(match[1]),
            ppid: Number(match[2]),
            elapsed: match[3],
            command: match[4],
          },
        ]
      : [];
  });
}

/**
 * Processes naming `root` in their arguments or holding an open file or working directory
 * under it. The directory is private to one run, so any process here belongs to that run.
 */
export function processesUnder(
  root,
  {
    table = () => capture('ps', ['-axo', 'pid=,ppid=,etime=,command=']),
    openFilePids = () => capture('lsof', ['-t', '+D', root]),
    self = process.pid,
  } = {}
) {
  const open = new Set(openFilePids().split('\n').filter(Boolean).map(Number));
  return parseProcessTable(table()).filter(
    row => row.pid !== self && (open.has(row.pid) || row.command.includes(root))
  );
}

/** Files under `root` modified at or after `since`: what something kept writing. */
export function writtenSince(root, since) {
  const written = [];
  const walk = directory => {
    let entries;
    try {
      entries = fs.readdirSync(directory, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const file = path.join(directory, entry.name);
      if (entry.isDirectory()) walk(file);
      const stat = fs.lstatSync(file, { throwIfNoEntry: false });
      if (stat && stat.mtimeMs >= since)
        written.push({
          file: path.relative(root, file),
          mtime: new Date(stat.mtimeMs).toISOString(),
        });
    }
  };
  walk(root);
  return written;
}

/**
 * Wait for the run's processes to leave the directory, recording how long each stayed.
 * A server saves its state as it exits, so removing the directory before then loses the race
 * or lets the server rebuild it. Nothing is killed: what outlives the wait is reported.
 */
export async function awaitProcessExit(
  root,
  {
    holders = processesUnder,
    now = Date.now,
    pause = ms => new Promise(resolve => setTimeout(resolve, ms)),
    deadlineMs = 60_000,
    pollMs = 250,
  } = {}
) {
  const started = now();
  const seen = new Map();
  for (;;) {
    const current = holders(root);
    const elapsedMs = now() - started;
    for (const row of current)
      seen.set(row.pid, {
        ...(seen.get(row.pid) ?? row),
        lastSeenMs: elapsedMs,
      });
    if (!current.length || elapsedMs >= deadlineMs)
      return {
        waitedMs: elapsedMs,
        processes: [...seen.values()],
        stillRunning: current,
      };
    await pause(pollMs);
  }
}

/** Never throws: a failure names the error, who holds the directory, and what they wrote. */
export function removeLiveRoot(
  root,
  { since, remove = fs.rmSync, holders = processesUnder } = {}
) {
  const failed = error => ({
    removed: false,
    error,
    holders: holders(root),
    writtenSinceStop: writtenSince(root, since),
  });
  try {
    // Node's own retry for a directory that fills while it is emptied (ENOTEMPTY, EBUSY).
    remove(root, {
      recursive: true,
      force: true,
      maxRetries: 10,
      retryDelay: 200,
    });
  } catch (error) {
    return failed(error.message);
  }
  return fs.existsSync(root)
    ? failed('something recreated the directory while it was removed')
    : { removed: true };
}
