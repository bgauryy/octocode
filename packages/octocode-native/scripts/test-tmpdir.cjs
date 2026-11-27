'use strict';

/**
 * A private temp root for one Rust test run. rust-tests.cjs (per run) and test-watchdog.cjs
 * (per test binary) create one, pass it to their children as TMPDIR, and remove it when the
 * run ends: pass, fail or kill. A test process that nextest's timeout or the watchdog kills
 * never runs its `TempDir` drop; what it left goes with the root.
 *
 * The root lives in /tmp, as the CLI tests' socket fixture (`PersistentCli`) does, because a
 * socket path must fit `sockaddr_un` (104 bytes on macOS, NUL included). The serve tests
 * bind `<TMPDIR>/.tmpXXXXXX/octocode-<uid>/serve-<16 hex>.sock`, 52 bytes past TMPDIR: under
 * macOS's per-user TMPDIR (48 bytes) a root of any name overflows (107 bytes with an empty
 * prefix), while `/private/tmp/octocode-rust-test-XXXXXX` (38 bytes) makes it 90.
 *
 * Removing the root first stops every process whose working directory is still inside it:
 * the run started it (an `octocode serve` daemon a test spawned and never stopped), and it
 * would otherwise outlive the run with its sockets gone. Each is stopped by its pid, with
 * SIGTERM and then SIGKILL after a short grace.
 */

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const BASE = process.platform === 'win32' ? os.tmpdir() : '/tmp';
const PREFIX = 'octocode-rust-test-';
/** How long a stopped process gets to exit on SIGTERM before SIGKILL. */
const GRACE_MS = 2000;

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** Pids of the processes whose working directory is `dir` or inside it, from `lsof`. */
function processesInside(dir) {
  if (process.platform === 'win32') return [];
  let real = dir;
  try {
    real = fs.realpathSync(dir);
  } catch {
    return [];
  }
  const listed = spawnSync('lsof', ['-a', '-d', 'cwd', '-F', 'pn'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  const inside = cwd => [dir, real].some(root => cwd === root || cwd.startsWith(root + path.sep));
  const found = [];
  let pid = null;
  for (const line of (listed.stdout ?? '').split('\n')) {
    if (line.startsWith('p')) pid = Number(line.slice(1));
    else if (line.startsWith('n') && pid !== null && pid !== process.pid && inside(line.slice(1))) found.push(pid);
  }
  return [...new Set(found)];
}

/** Stops the processes still working inside `dir`; returns how many there were. */
function stopProcessesInside(dir) {
  const pids = processesInside(dir);
  for (const pid of pids) {
    try {
      process.kill(pid, 'SIGTERM');
    } catch {
      // already gone
    }
  }
  const pause = new Int32Array(new SharedArrayBuffer(4));
  const deadline = Date.now() + GRACE_MS;
  while (Date.now() < deadline && pids.some(alive)) Atomics.wait(pause, 0, 0, 50);
  for (const pid of pids.filter(alive)) {
    try {
      process.kill(pid, 'SIGKILL');
    } catch {
      // already gone
    }
  }
  return pids.length;
}

/** Gives the owner access to every directory under `dir`: a killed test can leave one at mode 000. */
function unlock(dir) {
  let stat;
  try {
    stat = fs.lstatSync(dir);
  } catch {
    return;
  }
  if (!stat.isDirectory()) return;
  fs.chmodSync(dir, 0o700);
  for (const name of fs.readdirSync(dir)) unlock(path.join(dir, name));
}

function removeTree(dir) {
  try {
    fs.rmSync(dir, { recursive: true, force: true });
  } catch {
    unlock(dir);
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

/** Creates a root; `remove()` deletes it, and so does the process's exit at the latest. */
function testTmpdir() {
  const dir = fs.mkdtempSync(path.join(BASE, PREFIX));
  let removed = false;
  const remove = () => {
    if (removed) return;
    removed = true;
    process.removeListener('exit', remove);
    const stopped = stopProcessesInside(dir);
    if (stopped > 0) console.error(`test temp root: stopped ${stopped} process(es) the run left running in ${dir}`);
    try {
      removeTree(dir);
    } catch (error) {
      console.error(`test temp root ${dir} was not removed: ${error.message}`);
    }
  };
  process.on('exit', remove);
  return { dir, remove };
}

module.exports = { BASE, PREFIX, processesInside, stopProcessesInside, testTmpdir };
