'use strict';

/**
 * The test executables one cargo build produced, and their run side by side.
 *
 * `cargo test` runs a selection's binaries one after another, and each waits for its
 * slowest tests while its other threads idle: on 2026-11-18 the unit run kept 3.1 of
 * its 6 threads busy (910 CPU s in 296 s; packages/octocode-benchmark/campaigns/
 * 20.2.0-2026-11-16-round3/velocity/velocity-ab.tsv). Started together under one
 * thread budget, the slowest first, the run lasts about as long as its slowest binary.
 *
 * A binary runs as cargo runs it: from its package's directory, with the package's
 * CARGO_MANIFEST_DIR, CARGO_PKG_NAME and CARGO_PKG_VERSION, under the stuck-test
 * watchdog. Its output is printed whole when it ends, so binaries do not interleave.
 */

const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const WATCHDOG = path.join(__dirname, 'test-watchdog.cjs');
const RESULT = /^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/;

/** `name` and `version` of a cargo package id: `path+file:///…/dir#name@1.0`, `path+file:///…/name#1.0` or `name 1.0 (…)`. */
function packageOf(id, manifestPath) {
  const fragment = id.includes('#') ? id.slice(id.lastIndexOf('#') + 1) : '';
  if (fragment.includes('@')) {
    const at = fragment.lastIndexOf('@');
    return { name: fragment.slice(0, at), version: fragment.slice(at + 1) };
  }
  if (fragment) return { name: path.basename(path.dirname(manifestPath)), version: fragment };
  const [name, version] = id.split(' ');
  return { name, version: version ?? '' };
}

/** The test executables cargo's JSON messages name, one per test target, in message order. */
function testExecutables(messages) {
  const found = new Map();
  for (const message of messages) {
    if (message.reason !== 'compiler-artifact' || !message.executable || !message.profile?.test) continue;
    const { name, version } = packageOf(message.package_id, message.manifest_path);
    found.set(message.executable, {
      crate: name,
      version,
      kind: message.target.kind[0],
      target: message.target.name,
      executable: message.executable,
      dir: path.dirname(message.manifest_path),
    });
  }
  return [...found.values()];
}

/** `octocode-tool-github` for `tool-github` or `octocode-tool-github`; null when no crate has the name. */
function crateNamed(name, crates) {
  return [name, `octocode-${name}`].find(candidate => crates.has(candidate)) ?? null;
}

const keyOf = binary => `${binary.crate}/${binary.kind}/${binary.target}`;

function readDurations(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return {};
  }
}

/** Slowest first: the last run's seconds when known, else the executable's size. */
function slowestFirst(binaries, durations) {
  const size = binary => {
    try {
      return fs.statSync(binary.executable).size;
    } catch {
      return 0;
    }
  };
  const weight = binary => [durations[keyOf(binary)] ?? -1, size(binary)];
  return [...binaries].sort((a, b) => {
    const [da, sa] = weight(a);
    const [db, sb] = weight(b);
    return db - da || sb - sa;
  });
}

/** Sums libtest's `test result:` lines. */
function countResults(text) {
  const counts = { passed: 0, failed: 0, ignored: 0 };
  for (const line of text.split('\n')) {
    const match = RESULT.exec(line);
    if (match) {
      counts.passed += Number(match[1]);
      counts.failed += Number(match[2]);
      counts.ignored += Number(match[3]);
    }
  }
  return counts;
}

/**
 * Runs `binaries` side by side, at most `concurrency` at once, each with `--test-threads
 * threads` and `args` (libtest's filters and skips). Resolves the first failing status,
 * else 0, after every binary ran; records each binary's seconds in `durationsFile`.
 */
function runBinaries(binaries, { args = [], threads, concurrency, env, children = new Set(), durationsFile, log = console.log }) {
  const durations = durationsFile ? readDurations(durationsFile) : {};
  const queue = slowestFirst(binaries, durations);
  const startedMs = Date.now();
  const results = [];
  return new Promise(resolve => {
    let running = 0;
    const next = () => {
      if (queue.length === 0 && running === 0) return finish();
      while (running < concurrency && queue.length > 0) start(queue.shift());
    };
    const start = binary => {
      running += 1;
      const began = Date.now();
      const testArgs = [...args, '--test-threads', String(threads)];
      const child = spawn(process.execPath, [WATCHDOG, binary.executable, ...testArgs], {
        cwd: binary.dir,
        env: { ...env, CARGO_MANIFEST_DIR: binary.dir, CARGO_PKG_NAME: binary.crate, CARGO_PKG_VERSION: binary.version },
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      children.add(child);
      const out = [];
      child.stdout.on('data', chunk => out.push(chunk));
      child.stderr.on('data', chunk => out.push(chunk));
      const done = code => {
        children.delete(child);
        running -= 1;
        const seconds = (Date.now() - began) / 1000;
        const text = Buffer.concat(out).toString('utf8');
        const counts = countResults(text);
        results.push({ binary, code, seconds, ...counts });
        log(`=== ${keyOf(binary)} (${seconds.toFixed(1)} s, ${counts.passed} passed, ${counts.failed} failed) ===\n${text.trimEnd()}`);
        next();
      };
      child.on('error', error => {
        out.push(Buffer.from(`${error}\n`));
        done(1);
      });
      child.on('close', code => done(code ?? 1));
    };
    const finish = () => {
      const failed = results.filter(result => result.code !== 0);
      const total = results.reduce((sum, result) => ({ passed: sum.passed + result.passed, failed: sum.failed + result.failed }), { passed: 0, failed: 0 });
      const slowest = [...results].sort((a, b) => b.seconds - a.seconds).slice(0, 5);
      log(`=== binaries: ${results.length} in ${((Date.now() - startedMs) / 1000).toFixed(1)} s (${concurrency} at once, ${threads} threads each); ${total.passed} passed, ${total.failed} failed ===`);
      log(`slowest: ${slowest.map(result => `${keyOf(result.binary)} ${result.seconds.toFixed(1)} s`).join(', ')}`);
      for (const result of failed) log(`FAILED: ${keyOf(result.binary)} (exit ${result.code})`);
      if (durationsFile && results.length > 0) {
        for (const result of results) durations[keyOf(result.binary)] = Math.round(result.seconds * 10) / 10;
        try {
          fs.mkdirSync(path.dirname(durationsFile), { recursive: true });
          fs.writeFileSync(durationsFile, `${JSON.stringify(durations, null, 1)}\n`);
        } catch {
          // ordering is a speed hint only
        }
      }
      resolve(failed.length > 0 ? 1 : 0);
    };
    next();
  });
}

module.exports = { countResults, crateNamed, keyOf, packageOf, runBinaries, slowestFirst, testExecutables };
