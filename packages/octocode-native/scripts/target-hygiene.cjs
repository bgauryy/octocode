#!/usr/bin/env node
'use strict';

/**
 * Keeps the shared Cargo `target/` from growing without bound. Stable Cargo has no
 * target GC, and every package, feature or toolchain selection leaves its own copy of
 * our crates in `deps/`, `.fingerprint/` and `build/`, each named `<name>-<hash>`.
 *
 * After each run, the wrappers (build-native.cjs, rust-tests.cjs, rust-lint.mjs) call
 * finishCargoRun/writeKeep with Cargo's JSON messages, which list every unit's outputs,
 * fresh ones included:
 * - the `<name>-<hash>` stems the run used go to `target/.keep/<selection>.json`;
 * - `.rcgu.o` objects of a unit the run recompiled, older than the run, are deleted
 *   (left over when a unit's codegen units change, or by an incremental build: rust-lang/rust#161824);
 * - one line reports each deps/ dir's entry count.
 *
 *   target-hygiene.cjs prune [--dry-run] [--days N]   (`dev.mjs prune:rust`)
 *
 * Prune refuses while a cargo, rustc or nextest process is alive. In each profile dir a
 * keep record names, it deletes `deps/`, `.fingerprint/` and `build/` entries whose hash
 * no record of the last N (7) days lists, rustc's leftover temp dirs, `incremental/`,
 * and top-level files Cargo did not produce (copies of binaries). It also deletes
 * `target/debug/incremental` and `target/rust-lint/{debug,tmp}`. Nothing outside
 * `target/` is touched, so the staged launcher binaries (npm/<platform>/octocode) stay.
 */

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const { parseArgs } = require('node:util');

const TARGET = path.join(__dirname, '..', 'target');
const KEEP_DAYS = 7;
const DAY_MS = 24 * 60 * 60 * 1000;
/** A unit's output name: `<name>-<16 hex>`, then an extension or nothing. The hash is the same in deps/, .fingerprint/ and build/. */
const STEM = /^(.+?)-([0-9a-f]{16})(?=\.|$)/;
/** rustc's temporary dirs (`rustcXXXXXX`, `rmetaXXXXXX`), left in deps/ by an interrupted compile. */
const RUSTC_TEMP = /^(?:rustc|rmeta)[A-Za-z0-9]{6}$/;
const UNIT_DIRS = ['deps', '.fingerprint', 'build'];
const CARGO_PROCESS = /^(?:cargo|rustc|rustdoc|clippy-driver|cargo-.+)$/;

function hashOf(name) {
  return STEM.exec(name)?.[2] ?? null;
}

/** The profile dir (`target/debug`, `target/<triple>/release`) an artifact path sits in. */
function profileDirOf(file) {
  let dir = path.dirname(file);
  if (STEM.test(path.basename(dir))) dir = path.dirname(dir); // build/<stem>/<file>
  return ['deps', 'build', 'examples'].includes(path.basename(dir)) ? path.dirname(dir) : dir;
}

/** Splits a cargo stdout stream: JSON messages are collected, any other line is returned for printing. */
function parseCargoLine(line, messages) {
  if (line.startsWith('{')) {
    try {
      const message = JSON.parse(line);
      if (message && typeof message.reason === 'string') {
        messages.push(message);
        return null;
      }
    } catch {
      // not a cargo message
    }
  }
  return line;
}

/** What a run's `compiler-artifact` and `build-script-executed` messages say it used. */
function collectArtifacts(messages) {
  const stems = new Set();
  const profileDirs = new Set();
  const uplifted = new Set();
  const rebuilt = new Map(); // dir holding a unit's outputs → hashes of units recompiled this run
  for (const message of messages) {
    let paths;
    if (message.reason === 'compiler-artifact') paths = [...(message.filenames ?? []), message.executable].filter(Boolean);
    else if (message.reason === 'build-script-executed' && message.out_dir) paths = [message.out_dir];
    else continue;
    for (const file of paths) {
      const profileDir = profileDirOf(file);
      profileDirs.add(profileDir);
      if (path.dirname(file) === profileDir) uplifted.add(file);
      for (const part of path.relative(profileDir, file).split(path.sep)) {
        const match = STEM.exec(part);
        if (!match) continue;
        stems.add(`${match[1]}-${match[2]}`);
        if (message.reason === 'compiler-artifact' && message.fresh === false) {
          const dir = path.dirname(file);
          if (!rebuilt.has(dir)) rebuilt.set(dir, new Set());
          rebuilt.get(dir).add(match[2]);
        }
      }
    }
  }
  return { stems, profileDirs, uplifted, rebuilt };
}

/**
 * Deletes `.rcgu.o` objects of units this run recompiled that the compile did not
 * rewrite (mtime before the run). A fresh unit's objects stay: with
 * `split-debuginfo = "unpacked"` they hold the debug info its binaries point to.
 */
function sweepStaleObjects(rebuilt, startedMs) {
  const before = Math.floor(startedMs / 1000) * 1000; // coarse filesystem clocks round down
  let removed = 0;
  for (const [dir, hashes] of rebuilt) {
    let names;
    try {
      names = fs.readdirSync(dir);
    } catch {
      continue;
    }
    for (const name of names) {
      if (!name.endsWith('.rcgu.o') || !hashes.has(hashOf(name))) continue;
      const file = path.join(dir, name);
      try {
        if (fs.lstatSync(file).mtimeMs < before) {
          fs.unlinkSync(file);
          removed += 1;
        }
      } catch {
        // already gone
      }
    }
  }
  return removed;
}

function keepFile(targetRoot, selection) {
  return path.join(targetRoot, '.keep', `${selection}.json`);
}

function readKeep(file) {
  try {
    const record = JSON.parse(fs.readFileSync(file, 'utf8'));
    return record && Array.isArray(record.stems) ? record : null;
  } catch {
    return null;
  }
}

/**
 * Records a selection's stems. A failed run may have stopped before Cargo listed every
 * unit, so it adds to the previous record instead of replacing it.
 */
function writeKeep(selection, artifacts, { success, targetRoot = TARGET, now = Date.now() }) {
  const file = keepFile(targetRoot, selection);
  const previous = success ? null : readKeep(file);
  const union = (key, values) => [...new Set([...(previous?.[key] ?? []), ...values])].sort();
  const record = {
    selection,
    recordedAt: new Date(now).toISOString(),
    profileDirs: union('profileDirs', artifacts.profileDirs),
    uplifted: union('uplifted', artifacts.uplifted),
    stems: union('stems', artifacts.stems),
  };
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const temp = `${file}.${process.pid}.tmp`;
  fs.writeFileSync(temp, `${JSON.stringify(record, null, 1)}\n`);
  fs.renameSync(temp, file);
  return record;
}

/** The post-run step of a build or test wrapper; returns the lines it printed. */
function finishCargoRun({ selection, messages, success, startedMs, targetRoot = TARGET, log = console.log }) {
  const artifacts = collectArtifacts(messages);
  const removed = sweepStaleObjects(artifacts.rebuilt, startedMs);
  if (artifacts.stems.size) writeKeep(selection, artifacts, { success, targetRoot });
  const lines = [];
  for (const dir of [...artifacts.profileDirs].sort()) {
    let count;
    try {
      count = fs.readdirSync(path.join(dir, 'deps')).length;
    } catch {
      continue;
    }
    const shown = path.relative(process.cwd(), path.join(dir, 'deps')) || '.';
    lines.push(`deps: ${count} entries (${shown})${removed ? `, removed ${removed} stale .rcgu.o` : ''}`);
  }
  for (const line of lines) log(line);
  return lines;
}

/** Cargo-family processes in `ps -axo pid=,comm=` output (macOS prints the full path, Linux the name). */
function cargoProcesses(psOutput) {
  const found = [];
  for (const line of psOutput.split('\n')) {
    const match = /^\s*(\d+)\s+(.+?)\s*$/.exec(line);
    if (match && CARGO_PROCESS.test(path.basename(match[2]))) found.push({ pid: Number(match[1]), name: path.basename(match[2]) });
  }
  return found;
}

function liveCargoProcesses() {
  const ps = spawnSync('ps', ['-axo', 'pid=,comm='], { encoding: 'utf8', maxBuffer: 2 ** 28 });
  if (ps.error || ps.status !== 0) throw new Error(`cannot list processes: ${ps.error?.message ?? ps.stderr}`);
  return cargoProcesses(ps.stdout);
}

function inside(root, file) {
  const relative = path.relative(root, file);
  return relative !== '' && !relative.startsWith('..') && !path.isAbsolute(relative);
}

function diskBytes(file) {
  let stat;
  try {
    stat = fs.lstatSync(file);
  } catch {
    return 0;
  }
  let bytes = stat.blocks === undefined ? stat.size : stat.blocks * 512;
  if (stat.isDirectory()) for (const name of fs.readdirSync(file)) bytes += diskBytes(path.join(file, name));
  return bytes;
}

function formatBytes(bytes) {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${unit ? value.toFixed(1) : value} ${units[unit]}`;
}

function recentRecords(targetRoot, sinceMs) {
  let names;
  try {
    names = fs.readdirSync(path.join(targetRoot, '.keep'));
  } catch {
    return [];
  }
  return names
    .filter(name => name.endsWith('.json'))
    .map(name => readKeep(path.join(targetRoot, '.keep', name)))
    .filter(record => record && Date.parse(record.recordedAt) >= sinceMs);
}

/** What prune would delete, grouped by the dir it is in: Map<dir, string[]>. */
function planPrune(targetRoot, records) {
  const keep = new Set(records.flatMap(record => record.stems.map(hashOf)));
  const uplifted = new Map(); // profile dir → names (up to the first dot) Cargo put there
  for (const record of records) {
    for (const dir of record.profileDirs) if (!uplifted.has(dir)) uplifted.set(dir, new Set());
    for (const file of record.uplifted ?? []) uplifted.get(path.dirname(file))?.add(path.basename(file).split('.')[0]);
  }
  const plan = new Map();
  const add = (dir, name) => {
    const file = path.join(dir, name);
    if (!inside(targetRoot, file) || !fs.existsSync(file)) return;
    if (!plan.has(dir)) plan.set(dir, []);
    plan.get(dir).push(name);
  };
  const covered = [...uplifted.keys()].filter(dir => inside(targetRoot, dir) && fs.existsSync(dir)).sort();
  for (const profileDir of covered) {
    for (const sub of UNIT_DIRS) {
      const dir = path.join(profileDir, sub);
      let names = [];
      try {
        names = fs.readdirSync(dir);
      } catch {
        continue;
      }
      for (const name of names.sort()) {
        const hash = hashOf(name);
        if (hash ? !keep.has(hash) : RUSTC_TEMP.test(name)) add(dir, name);
      }
    }
    add(profileDir, 'incremental');
    const produced = uplifted.get(profileDir);
    for (const entry of fs.readdirSync(profileDir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      if (!entry.isDirectory() && !entry.name.startsWith('.') && !produced.has(entry.name.split('.')[0])) add(profileDir, entry.name);
    }
  }
  add(path.join(targetRoot, 'debug'), 'incremental');
  add(path.join(targetRoot, 'rust-lint'), 'debug');
  add(path.join(targetRoot, 'rust-lint'), 'tmp');
  return { plan, covered };
}

/**
 * Deletes what no recent keep record protects. Returns { refused } with the live
 * cargo-family processes, or { removed: [{ dir, count, bytes }], covered, records }.
 */
function prune({ targetRoot = TARGET, now = Date.now(), days = KEEP_DAYS, dryRun = false, processes = liveCargoProcesses } = {}) {
  const live = processes();
  if (live.length) return { refused: live };
  const records = recentRecords(targetRoot, now - days * DAY_MS);
  const { plan, covered } = planPrune(targetRoot, records);
  const removed = [];
  for (const [dir, names] of plan) {
    let bytes = 0;
    for (const name of names) {
      const file = path.join(dir, name);
      bytes += diskBytes(file);
      if (!dryRun) fs.rmSync(file, { recursive: true, force: true });
    }
    removed.push({ dir, count: names.length, bytes, names });
  }
  return { removed, covered, records };
}

function main(argv) {
  let parsed;
  try {
    parsed = parseArgs({ args: argv, allowPositionals: true, options: { 'dry-run': { type: 'boolean', default: false }, days: { type: 'string' } } });
  } catch (error) {
    parsed = { error };
  }
  const days = parsed.values?.days === undefined ? KEEP_DAYS : Number(parsed.values.days);
  if (parsed.error || parsed.positionals.join(' ') !== 'prune' || !(days > 0)) {
    console.error(`${parsed.error ? `${parsed.error.message}\n` : ''}Usage: target-hygiene.cjs prune [--dry-run] [--days N]`);
    return 2;
  }
  const dryRun = parsed.values['dry-run'];
  const result = prune({ dryRun, days });
  if (result.refused) {
    console.error(`prune:rust: refused, cargo-family processes are running: ${result.refused.map(p => `${p.pid} ${p.name}`).join(', ')}`);
    return 1;
  }
  const show = file => path.relative(process.cwd(), file) || '.';
  const verb = dryRun ? 'would remove' : 'removed';
  if (!result.records.length) console.log(`prune:rust: no keep record from the last ${days} days, so deps/ is left as is; run build:dev, test:rust and lint:rust first`);
  for (const record of result.records) console.log(`keep: ${record.selection} (${record.recordedAt}, ${record.stems.length} stems)`);
  if (result.covered.length) console.log(`pruned profile dirs: ${result.covered.map(show).join(', ')}`);
  let total = 0;
  let count = 0;
  for (const { dir, count: entries, bytes, names } of result.removed) {
    total += bytes;
    count += entries;
    const detail = entries <= 12 ? `: ${names.join(', ')}` : '';
    console.log(`${verb} ${show(dir)}: ${entries} entries, ${formatBytes(bytes)}${detail}`);
  }
  console.log(`prune:rust: ${verb} ${count} entries, ${formatBytes(total)}`);
  return 0;
}

if (require.main === module) process.exit(main(process.argv.slice(2)));

module.exports = { cargoProcesses, collectArtifacts, finishCargoRun, main, parseCargoLine, prune, writeKeep };
