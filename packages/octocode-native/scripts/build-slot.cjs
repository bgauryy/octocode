#!/usr/bin/env node
'use strict';

/**
 * Machine-wide Rust build slots: at most two cargo builds, checks, lints or test runs at
 * once, across every session, agent and isolated copy of this repo, since they all share
 * `os.tmpdir()`. On 2026-11-17 five concurrent builds drove the load to 59-172 on 12 cores
 * and 32 GB, with swap; a crate that needed 80 s of CPU took 365 s.
 *
 *   build-slot.cjs -- <command> [args...]    run the command while holding a slot
 *
 * The wrappers hold one for their whole run: build-native.cjs per platform job,
 * rust-tests.cjs (`test:rust`, `coverage:rust`), rust-lint.mjs (`lint:rust`), the package
 * scripts that call cargo, and `dev.mjs test:dev`. A bare `cargo build/test/clippy`
 * bypasses the limit.
 *
 * A slot is the file `slot-<n>.json` in SLOT_DIR, hard-linked into place from a complete
 * owner record (pid, label, cwd, start), so taking one is atomic. A record whose pid is
 * dead, or now names another process (its start time differs), is stale and reclaimed. A
 * process started under a held slot inherits it (HELD_ENV), so a wrapper run by another
 * wrapper does not wait for a second slot. OCTOCODE_BUILD_SLOTS overrides the count.
 *
 * Each slot also gets its share of the cores: its children run with CARGO_BUILD_JOBS,
 * NEXTEST_TEST_THREADS and RUST_TEST_THREADS set to the cores divided by the slot count,
 * unless the caller set them. Two cargo builds at `-j<cores>` each still put twice the
 * cores' work on the machine (load 92 on 12 cores, measured 2026-11-17).
 */

const { spawnSync } = require('node:child_process');
const { randomUUID } = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { runForwardingSignals } = require('../bin/launch-native.cjs');

const SLOT_DIR = path.join(os.tmpdir(), 'octocode-build-slots');
const COUNT_ENV = 'OCTOCODE_BUILD_SLOTS';
const HELD_ENV = 'OCTOCODE_BUILD_SLOT_HELD';
const DEFAULT_COUNT = 2;
const POLL_MS = 1000;
const NOTICE_MS = 60_000;
const SLOT_FILE = /^slot-(\d+)\.json$/;
/** The variables a slot's children read for their parallelism. */
const SHARE_ENV = ['CARGO_BUILD_JOBS', 'NEXTEST_TEST_THREADS', 'RUST_TEST_THREADS'];

function slotCount(env = process.env) {
  const value = env[COUNT_ENV];
  if (value === undefined || value === '') return DEFAULT_COUNT;
  const count = Number(value);
  if (!Number.isInteger(count) || count < 1) throw new Error(`${COUNT_ENV} must be a positive integer, got "${value}"`);
  return count;
}

const slotFile = (dir, n) => path.join(dir, `slot-${n}.json`);

/** `env` with one slot's share of the cores for each parallelism variable it leaves unset. */
function shareCores(env, count, cores = os.availableParallelism()) {
  const share = String(Math.max(1, Math.floor(cores / count)));
  const shared = { ...env };
  for (const name of SHARE_ENV) if (shared[name] === undefined || shared[name] === '') shared[name] = share;
  return shared;
}

function readRaw(file) {
  try {
    return fs.readFileSync(file, 'utf8');
  } catch (error) {
    if (error.code === 'ENOENT') return undefined;
    throw error;
  }
}

/** The owner record in `text`, or null when it is not one (a pid of 0 or less would signal a group). */
function parseRecord(text) {
  try {
    const record = JSON.parse(text);
    return Number.isInteger(record?.pid) && record.pid > 0 ? record : null;
  } catch {
    return null;
  }
}

/** The process's start time as `ps` prints it, or null where `ps` cannot tell. */
function processStart(pid) {
  if (process.platform === 'win32') return null;
  const run = spawnSync('ps', ['-o', 'lstart=', '-p', String(pid)], { encoding: 'utf8', env: { ...process.env, LC_ALL: 'C' } });
  return (run.status === 0 && run.stdout.trim()) || null;
}

let ownStart;
const confirmed = new Set();

/** Whether the record's process still runs: its pid answers and, where `ps` tells, started when recorded. */
function ownerAlive(record) {
  try {
    process.kill(record.pid, 0);
  } catch (error) {
    if (error.code !== 'EPERM') return false;
  }
  if (!record.start || confirmed.has(record.token)) return true;
  const start = processStart(record.pid);
  if (start !== null && start !== record.start) return false;
  if (start !== null) confirmed.add(record.token);
  return true;
}

/** Links a complete copy of `text` to `file`; false when the file exists. */
function linkRecord(text, file) {
  const draft = `${file}.${process.pid}-${randomUUID()}.draft`;
  fs.writeFileSync(draft, text);
  try {
    fs.linkSync(draft, file);
    return true;
  } catch (error) {
    if (error.code === 'EEXIST') return false;
    throw error;
  } finally {
    fs.rmSync(draft, { force: true });
  }
}

/** Removes the stale record `seen` from `file`. */
function reclaim(file, seen) {
  const moved = `${file}.${process.pid}-${randomUUID()}.stale`;
  try {
    fs.renameSync(file, moved);
  } catch (error) {
    if (error.code === 'ENOENT') return;
    throw error;
  }
  try {
    // Another waiter reclaimed it first and a new owner took the slot: give the record back.
    if (readRaw(moved) !== seen) fs.linkSync(moved, file);
  } catch (error) {
    if (error.code !== 'EEXIST') throw error;
  } finally {
    fs.rmSync(moved, { force: true });
  }
}

function tryTake(file, text) {
  const seen = readRaw(file);
  if (seen !== undefined) {
    const holder = parseRecord(seen);
    if (holder && ownerAlive(holder)) return false;
    reclaim(file, seen);
  }
  return linkRecord(text, file);
}

function duration(ms) {
  const seconds = Math.round(ms / 1000);
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m${String(seconds % 60).padStart(2, '0')}s`;
}

function waitingNotice(dir, count, label) {
  const lines = [`build-slot: "${label}" waits for one of ${count} machine-wide Rust build slots (${COUNT_ENV}); held by:`];
  for (let n = 1; n <= count; n += 1) {
    const record = parseRecord(readRaw(slotFile(dir, n)) ?? '');
    const since = Date.parse(record?.since);
    lines.push(
      record
        ? `  slot ${n}: pid ${record.pid} "${record.label}" in ${record.cwd}, ${Number.isNaN(since) ? 'since unknown' : `for ${duration(Date.now() - since)}`}`
        : `  slot ${n}: free`,
    );
  }
  return lines.join('\n');
}

/** The slot named by an inherited token whose holder still runs, else null. */
function inheritedSlot(dir, token) {
  if (!token) return null;
  let names;
  try {
    names = fs.readdirSync(dir);
  } catch {
    return null;
  }
  for (const name of names) {
    const n = SLOT_FILE.exec(name)?.[1];
    const record = n && parseRecord(readRaw(path.join(dir, name)) ?? '');
    if (record && record.token === token && ownerAlive(record)) return Number(n);
  }
  return null;
}

const held = new Set();
let releaseOnExit = false;
// Jobs of one process under an inherited slot run one at a time: the slot is one build.
let inheritedTail = Promise.resolve();

function releaseRecord({ file, text }) {
  if (readRaw(file) === text) fs.rmSync(file, { force: true });
}

function releaseAll() {
  for (const entry of held) releaseRecord(entry);
  held.clear();
}

function hold(file, n, text, token, env, count) {
  const entry = { file, text };
  held.add(entry);
  if (!releaseOnExit) {
    releaseOnExit = true;
    process.on('exit', releaseAll);
  }
  return {
    slot: n,
    env: { ...shareCores(env, count), [HELD_ENV]: token },
    release() {
      if (held.delete(entry)) releaseRecord(entry);
    },
  };
}

async function takeInherited(n, env) {
  const previous = inheritedTail;
  let done;
  inheritedTail = new Promise(resolve => {
    done = resolve;
  });
  await previous;
  return { slot: n, env: { ...env }, release: () => done() };
}

/**
 * Waits for a free slot and takes it for `label`. Resolves `{ slot, env, release }`: pass
 * `env` to the children (it marks the slot as theirs too) and call `release()` when the
 * build ends. A slot still held is released when the process exits; call exitOnSignals()
 * in a script so a signal exits through process.exit and releases it.
 */
async function acquireSlot(label, options = {}) {
  const { dir = SLOT_DIR, env = process.env, pollMs = POLL_MS, noticeMs = NOTICE_MS, log = message => console.error(message) } = options;
  const count = slotCount(env);
  const inherited = inheritedSlot(dir, env[HELD_ENV]);
  if (inherited !== null) return takeInherited(inherited, env);
  fs.mkdirSync(dir, { recursive: true });
  ownStart ??= processStart(process.pid);
  const record = { pid: process.pid, start: ownStart, label, cwd: process.cwd(), since: new Date().toISOString(), token: randomUUID() };
  const text = `${JSON.stringify(record)}\n`;
  const waitStarted = Date.now();
  let noticedAt = null;
  for (;;) {
    for (let n = 1; n <= count; n += 1) {
      const file = slotFile(dir, n);
      if (tryTake(file, text)) {
        if (noticedAt !== null) log(`build-slot: "${label}" took slot ${n} after ${duration(Date.now() - waitStarted)}`);
        return hold(file, n, text, record.token, env, count);
      }
    }
    if (noticedAt === null || Date.now() - noticedAt >= noticeMs) {
      log(waitingNotice(dir, count, label));
      noticedAt = Date.now();
    }
    await new Promise(resolve => setTimeout(resolve, pollMs));
  }
}

/** Runs `fn(slot)` while holding a slot; releases it however `fn` ends. */
async function withSlot(label, fn, options) {
  const slot = await acquireSlot(label, options);
  try {
    return await fn(slot);
  } finally {
    slot.release();
  }
}

/** Ends the process through process.exit on SIGINT, SIGTERM or SIGHUP, so exit cleanups run. */
function exitOnSignals() {
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(signal, () => process.exit(128 + os.constants.signals[signal]));
}

async function main(argv) {
  if (argv[0] !== '--' || argv.length < 2) {
    console.error('Usage: build-slot.cjs -- <command> [args...]');
    return 2;
  }
  const [command, ...args] = argv.slice(1);
  exitOnSignals();
  return withSlot([command, ...args].join(' '), async slot => {
    try {
      return await runForwardingSignals(command, args, { env: slot.env });
    } catch (error) {
      console.error(`build-slot: ${command}: ${error.message}`);
      return 127;
    }
  });
}

if (require.main === module) {
  main(process.argv.slice(2)).then(
    code => process.exit(code),
    error => {
      console.error(`build-slot: ${error.message}`);
      process.exit(1);
    },
  );
}

module.exports = { SLOT_DIR, COUNT_ENV, HELD_ENV, SHARE_ENV, acquireSlot, exitOnSignals, shareCores, slotCount, withSlot };
