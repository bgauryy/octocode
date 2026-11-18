'use strict';

const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const { tmpdir } = require('node:os');
const path = require('node:path');
const { test } = require('node:test');
const { cargoProcesses, finishCargoRun, main, prune } = require('../scripts/target-hygiene.cjs');

const LIVE = '1111111111111111';
const OLD = '2222222222222222';
const BUILD_RUN = '3333333333333333';
const OTHER_SELECTION = '4444444444444444';
const STALE_RECORD = '5555555555555555';
const DAY = 24 * 60 * 60 * 1000;

function fakeTree(t) {
  const root = fs.mkdtempSync(path.join(tmpdir(), 'target-hygiene-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const target = path.join(root, 'target');
  const debug = path.join(target, 'debug');
  const write = (rel, text = 'x') => {
    const file = path.join(root, rel);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, text);
    return file;
  };
  for (const hash of [LIVE, OLD, OTHER_SELECTION, STALE_RECORD]) {
    write(`target/debug/deps/libcore_crate-${hash}.rlib`);
    write(`target/debug/deps/core_crate-${hash}.d`);
    write(`target/debug/deps/core_crate-${hash}.core_crate.abc-cgu.0.rcgu.o`);
    write(`target/debug/.fingerprint/core-crate-${hash}/lib-core_crate`);
  }
  write(`target/debug/build/c-sys-${BUILD_RUN}/out/lib.a`);
  write(`target/debug/build/c-sys-${OLD}/out/lib.a`);
  write('target/debug/deps/rustcAbC123/temp.o');
  write('target/debug/deps/notes.txt');
  write('target/debug/octocode');
  write('target/debug/octocode.d');
  write('target/debug/bw-diag');
  write('target/debug/bw-diag.d');
  write('target/debug/.cargo-lock');
  write('target/debug/incremental/core_crate-x/s-1/query-cache.bin');
  write('target/rust-lint/debug/deps/x');
  write('target/rust-lint/tmp/y');
  write('target/rust-lint/clippy.toml');
  write(`target/platforms/linux-x64/release/deps/libcore_crate-${OLD}.rlib`);
  write('npm/darwin-arm64/octocode');
  const messages = [
    { reason: 'compiler-artifact', fresh: true, filenames: [path.join(debug, 'deps', `libcore_crate-${LIVE}.rlib`)], executable: null },
    { reason: 'build-script-executed', out_dir: path.join(debug, 'build', `c-sys-${BUILD_RUN}`, 'out') },
    { reason: 'compiler-artifact', fresh: true, filenames: [path.join(debug, 'octocode')], executable: path.join(debug, 'octocode') },
    { reason: 'build-finished', success: true },
  ];
  return { root, target, debug, messages };
}

const exists = (root, rel) => fs.existsSync(path.join(root, rel));
const keepStems = (target, selection) => JSON.parse(fs.readFileSync(path.join(target, '.keep', `${selection}.json`), 'utf8')).stems;

test('prune removes only the stems no recent keep record lists, and copies Cargo did not produce', t => {
  const { root, target, debug, messages } = fakeTree(t);
  const now = Date.now();
  finishCargoRun({ selection: 'build-dev-host', messages, success: true, startedMs: now, targetRoot: target, log: () => {} });
  const other = [{ reason: 'compiler-artifact', fresh: true, filenames: [path.join(debug, 'deps', `libcore_crate-${OTHER_SELECTION}.rlib`)] }];
  finishCargoRun({ selection: 'test-octocode-engine', messages: other, success: true, startedMs: now, targetRoot: target, log: () => {} });
  const stale = [{ reason: 'compiler-artifact', fresh: true, filenames: [path.join(debug, 'deps', `libcore_crate-${STALE_RECORD}.rlib`)] }];
  finishCargoRun({ selection: 'test-coverage', messages: stale, success: true, startedMs: now, targetRoot: target, log: () => {} });
  const staleFile = path.join(target, '.keep', 'test-coverage.json');
  const staleRecord = JSON.parse(fs.readFileSync(staleFile, 'utf8'));
  fs.writeFileSync(staleFile, JSON.stringify({ ...staleRecord, recordedAt: new Date(now - 8 * DAY).toISOString() }));

  const preview = prune({ targetRoot: target, now, processes: () => [], dryRun: true });
  assert.ok(preview.removed.length > 0);
  assert.ok(exists(root, `target/debug/deps/libcore_crate-${OLD}.rlib`), 'a dry run deletes nothing');

  const result = prune({ targetRoot: target, now, processes: () => [] });
  assert.deepEqual(result.records.map(record => record.selection).sort(), ['build-dev-host', 'test-octocode-engine']);
  for (const hash of [LIVE, OTHER_SELECTION]) {
    assert.ok(exists(root, `target/debug/deps/libcore_crate-${hash}.rlib`), `live stem ${hash} survives`);
    assert.ok(exists(root, `target/debug/deps/core_crate-${hash}.core_crate.abc-cgu.0.rcgu.o`), `live stem ${hash} keeps its debug objects`);
    assert.ok(exists(root, `target/debug/.fingerprint/core-crate-${hash}`), `live fingerprint ${hash} survives`);
  }
  for (const hash of [OLD, STALE_RECORD]) {
    assert.ok(!exists(root, `target/debug/deps/libcore_crate-${hash}.rlib`), `unlisted stem ${hash} is removed`);
    assert.ok(!exists(root, `target/debug/deps/core_crate-${hash}.d`));
    assert.ok(!exists(root, `target/debug/.fingerprint/core-crate-${hash}`));
  }
  assert.ok(exists(root, `target/debug/build/c-sys-${BUILD_RUN}/out/lib.a`), 'a build script run the record lists survives');
  assert.ok(!exists(root, `target/debug/build/c-sys-${OLD}`));
  assert.ok(!exists(root, 'target/debug/deps/rustcAbC123'), 'rustc temp dirs go');
  assert.ok(exists(root, 'target/debug/deps/notes.txt'), 'an entry with no stem is not judged');
  assert.ok(exists(root, 'target/debug/octocode') && exists(root, 'target/debug/octocode.d'), 'uplifted outputs stay');
  assert.ok(!exists(root, 'target/debug/bw-diag') && !exists(root, 'target/debug/bw-diag.d'), 'copied binaries go');
  assert.ok(exists(root, 'target/debug/.cargo-lock'));
  assert.ok(!exists(root, 'target/debug/incremental'));
  assert.ok(!exists(root, 'target/rust-lint/debug') && !exists(root, 'target/rust-lint/tmp'));
  assert.ok(exists(root, 'target/rust-lint/clippy.toml'));
  assert.ok(exists(root, `target/platforms/linux-x64/release/deps/libcore_crate-${OLD}.rlib`), 'a profile dir no record names is left as is');
  assert.ok(exists(root, 'npm/darwin-arm64/octocode'), 'staged binaries outside target/ stay');
  const removedDeps = result.removed.find(entry => entry.dir === path.join(debug, 'deps'));
  assert.ok(removedDeps.count >= 7 && removedDeps.bytes > 0);
});

test('with no recent keep record, prune leaves every unit dir alone', t => {
  const { root, target } = fakeTree(t);
  const result = prune({ targetRoot: target, processes: () => [] });
  assert.equal(result.records.length, 0);
  assert.ok(exists(root, `target/debug/deps/libcore_crate-${OLD}.rlib`));
  assert.ok(exists(root, 'target/debug/bw-diag'));
  assert.ok(!exists(root, 'target/debug/incremental'));
});

test('prune refuses, and deletes nothing, while a cargo process is alive', async t => {
  const { root, target, messages } = fakeTree(t);
  finishCargoRun({ selection: 'build-dev-host', messages, success: true, startedMs: Date.now(), targetRoot: target, log: () => {} });
  const fake = path.join(root, 'bin', 'cargo');
  fs.mkdirSync(path.dirname(fake));
  // A link, not a copy: macOS kills a copied system binary at launch (exit 137).
  fs.symlinkSync('/bin/sleep', fake);
  const child = spawn(fake, ['30'], { stdio: 'ignore' });
  t.after(() => child.kill('SIGKILL'));
  await new Promise(resolve => child.once('spawn', resolve));
  const result = prune({ targetRoot: target });
  assert.ok(result.refused.some(process => process.pid === child.pid && process.name === 'cargo'), JSON.stringify(result.refused));
  assert.ok(exists(root, `target/debug/deps/libcore_crate-${OLD}.rlib`));
  assert.ok(exists(root, 'target/debug/incremental'));
});

test('cargo-family processes are recognized by executable name only', () => {
  const ps = [
    '  101 /Users/me/.rustup/toolchains/1.99.1-aarch64-apple-darwin/bin/rustc',
    '  102 cargo-nextest',
    '  103 /Applications/Visual Studio Code.app/Contents/MacOS/Electron',
    '  104 /Users/me/.cargo/bin/rust-analyzer',
    '  105 clippy-driver',
    '  106 /usr/local/bin/node',
    '  107 cargo',
  ].join('\n');
  assert.deepEqual(cargoProcesses(ps).map(process => process.pid), [101, 102, 105, 107]);
});

test('a run sweeps stale objects of units it recompiled, never of fresh ones, and records what it used', t => {
  const { root, target, debug } = fakeTree(t);
  const startedMs = Date.now();
  const past = new Date(startedMs - 60_000);
  const deps = path.join(debug, 'deps');
  const leftover = path.join(deps, `core_crate-${LIVE}.core_crate.old-cgu.3.rcgu.o`);
  fs.writeFileSync(leftover, 'x');
  for (const name of fs.readdirSync(deps)) fs.utimesSync(path.join(deps, name), past, past);
  const rewritten = path.join(deps, `core_crate-${LIVE}.core_crate.abc-cgu.0.rcgu.o`);
  fs.writeFileSync(rewritten, 'new');
  const messages = [
    { reason: 'compiler-artifact', fresh: false, filenames: [path.join(deps, `libcore_crate-${LIVE}.rlib`)] },
    { reason: 'compiler-artifact', fresh: true, filenames: [path.join(deps, `libcore_crate-${OTHER_SELECTION}.rlib`)] },
  ];
  const lines = finishCargoRun({ selection: 'test-octocode-native', messages, success: true, startedMs, targetRoot: target, log: () => {} });
  assert.ok(!fs.existsSync(leftover), 'an object the recompile did not rewrite goes');
  assert.ok(fs.existsSync(rewritten), 'an object the recompile wrote stays');
  assert.ok(exists(root, `target/debug/deps/core_crate-${OTHER_SELECTION}.core_crate.abc-cgu.0.rcgu.o`), "a fresh unit's debug objects stay");
  assert.equal(lines.length, 1);
  assert.match(lines[0], /^deps: \d+ entries \(.+\), removed 1 stale \.rcgu\.o$/);
  assert.deepEqual(keepStems(target, 'test-octocode-native'), [`libcore_crate-${LIVE}`, `libcore_crate-${OTHER_SELECTION}`]);

  // A failed run may stop before Cargo lists every unit: it adds to the record.
  const partial = [{ reason: 'compiler-artifact', fresh: true, filenames: [path.join(deps, `libcore_crate-${OLD}.rlib`)] }];
  finishCargoRun({ selection: 'test-octocode-native', messages: partial, success: false, startedMs, targetRoot: target, log: () => {} });
  assert.equal(keepStems(target, 'test-octocode-native').length, 3);
  finishCargoRun({ selection: 'test-octocode-native', messages: partial, success: true, startedMs, targetRoot: target, log: () => {} });
  assert.deepEqual(keepStems(target, 'test-octocode-native'), [`libcore_crate-${OLD}`]);
});

test('an unknown argument or missing subcommand is a usage error, never a prune', () => {
  const errors = [];
  const original = console.error;
  console.error = message => errors.push(message);
  try {
    assert.equal(main(['prune', '--bogus']), 2);
    assert.equal(main([]), 2);
    assert.equal(main(['prune', '--days', '0']), 2);
  } finally {
    console.error = original;
  }
  assert.equal(errors.length, 3);
});

test('every build-native mode asks Cargo for the JSON messages keep records come from', () => {
  const { hostsCommand } = require('../scripts/build-native.cjs');
  for (const mode of [{}, { release: true }, { check: true }]) {
    assert.ok(hostsCommand(null, mode, '/tmp/t').args.includes('--message-format=json-render-diagnostics'));
  }
});
