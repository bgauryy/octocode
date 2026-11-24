import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { RATCHET, adopt, clippyArgs, collect, compare, ratchetConfig, ratchetKey, rekey, rekeyPrefix, serialize, shrink } from './rust-lint.mjs';

const record = (code, message, file, line = 1, kind = ['lib']) => ({
  reason: 'compiler-message',
  target: { kind },
  message: { code: { code }, message, spans: [{ is_primary: true, file_name: file, line_start: line, column_start: 1 }] },
});
const SLEEP = 'use of a disallowed method `std::thread::sleep`';

test('a disallowed call is counted apart per banned path; other lints are not ratchets', () => {
  assert.equal(ratchetKey(record('clippy::disallowed_methods', SLEEP, 'x').message), 'clippy::disallowed_methods(std::thread::sleep)');
  assert.equal(ratchetKey(record('clippy::string_slice', 'slice', 'x').message), 'clippy::string_slice');
  assert.equal(ratchetKey(record('clippy::needless_return', 'x', 'x').message), undefined);
});

test('collect counts each span once, per crate and file, and skips build scripts', () => {
  const found = collect([
    record('clippy::string_slice', 's', 'crates/io/src/a.rs', 1),
    record('clippy::string_slice', 's', 'crates/io/src/a.rs', 1),
    record('clippy::string_slice', 's', 'crates/io/src/a.rs', 9),
    record('clippy::string_slice', 's', 'crates/cli/src/b.rs', 2),
    record('clippy::string_slice', 's', 'crates/cli/build.rs', 2, ['custom-build']),
  ]);
  assert.deepEqual(found, { 'clippy::string_slice': { io: { 'crates/io/src/a.rs': 2 }, cli: { 'crates/cli/src/b.rs': 1 } } });
});

test('moving a finding between files of a crate is neither growth nor staleness', () => {
  const baseline = { lint: { io: { 'a.rs': 1 } } };
  assert.deepEqual(compare(baseline, { lint: { io: { 'b.rs': 1 } } }), { grown: [], stale: [] });
});

test('a crate that grew names the files, one that shrank is stale, a new lint counts from zero', () => {
  const baseline = { lint: { io: { 'a.rs': 1 }, cli: { 'c.rs': 2 } } };
  const { grown, stale } = compare(baseline, { lint: { io: { 'a.rs': 1, 'b.rs': 1 }, cli: { 'c.rs': 1 } }, fresh: { io: { 'd.rs': 1 } } });
  assert.deepEqual(grown, [
    { key: 'lint', crate: 'io', had: 1, has: 2, files: ['b.rs'] },
    { key: 'fresh', crate: 'io', had: 0, has: 1, files: ['d.rs'] },
  ]);
  assert.deepEqual(stale, [{ key: 'lint', crate: 'cli', had: 2, has: 1 }]);
});

test('--write lowers and refreshes but never raises or adds', () => {
  const baseline = { lint: { io: { 'a.rs': 2 }, cli: { 'c.rs': 2 }, gone: { 'g.rs': 1 } } };
  const found = { lint: { io: { 'a.rs': 3 }, cli: { 'moved.rs': 1 } }, fresh: { io: { 'd.rs': 1 } } };
  assert.deepEqual(shrink(baseline, found), { lint: { io: { 'a.rs': 2 }, cli: { 'moved.rs': 1 } } });
});

test('--adopt adds only keys the baseline lacks', () => {
  assert.deepEqual(adopt({ lint: { io: { 'a.rs': 1 } } }, { lint: { io: { 'a.rs': 5 } }, fresh: { io: { 'd.rs': 1 } } }), {
    lint: { io: { 'a.rs': 1 } },
    fresh: { io: { 'd.rs': 1 } },
  });
});

test('the file is stable: sorted keys and a note', () => {
  assert.equal(serialize({ b: { y: {} }, a: { x: {} } }).indexOf('"a"') < serialize({ b: { y: {} }, a: { x: {} } }).indexOf('"b"'), true);
  assert.match(serialize({}), /only shrink/);
});

test('the ratchet config keeps clippy.toml settings and replaces only its bans', () => {
  const base = 'a = 1\ndisallowed-methods = [\n    { path = "x" },\n]\nb = 2\n';
  assert.equal(ratchetConfig(base, 'disallowed-methods = ["y"]\n'), 'a = 1\nb = 2\ndisallowed-methods = ["y"]\n');
});

test('ratchet cfg and lint flags reach rustc for production and all-target passes', () => {
  for (const allTargets of [false, true]) {
    const args = clippyArgs(allTargets);
    const separator = args.indexOf('--');
    assert.equal(args[0], 'clippy');
    assert.equal(args.includes('--all-targets'), allTargets);
    assert.equal(args.includes('--all-features'), true);
    assert.equal(args.includes('--locked'), true);
    assert.equal(args.includes('--workspace'), true);
    assert.equal(args.includes('--message-format=json'), true);
    assert.equal(args.slice(0, separator).includes('--cfg'), false);
    assert.deepEqual(args.slice(separator + 1, separator + 3), ['--cfg', 'octocode_clippy_ratchet']);
    assert.deepEqual(args.slice(separator + 3), RATCHET.flatMap(lint => ['-W', lint]));
  }
});

test('lint:rust runs clippy in a build slot and releases it', { skip: process.platform === 'win32' }, t => {
  // A fake cargo first on PATH logs the slot holders it sees and fails, which ends the run after one pass.
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'rust-lint-slot-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const slots = path.join(root, 'octocode-build-slots');
  const log = path.join(root, 'cargo.log');
  fs.mkdirSync(path.join(root, 'bin'));
  fs.writeFileSync(path.join(root, 'bin', 'cargo'), `#!${process.execPath}
const fs = require('node:fs');
const labels = fs.readdirSync(${JSON.stringify(slots)}).filter(name => name.endsWith('.json'))
  .map(name => JSON.parse(fs.readFileSync(${JSON.stringify(slots)} + '/' + name, 'utf8')).label);
fs.appendFileSync(${JSON.stringify(log)}, JSON.stringify(labels) + '\\n');
process.exit(1);
`, { mode: 0o755 });
  const run = spawnSync(process.execPath, [fileURLToPath(new URL('./rust-lint.mjs', import.meta.url))], {
    encoding: 'utf8',
    // os.tmpdir() of the run is `root`, so it takes a private slot, never one of the machine's.
    env: { ...process.env, PATH: `${path.join(root, 'bin')}${path.delimiter}${process.env.PATH}`, TMPDIR: root, OCTOCODE_BUILD_SLOT_HELD: '' },
  });
  assert.equal(run.status, 1, run.stderr);
  assert.match(run.stderr, /rust-lint: clippy failed/);
  assert.deepEqual(fs.readFileSync(log, 'utf8').trim().split('\n').map(line => JSON.parse(line)), [['lint:rust']]);
  assert.deepEqual(fs.readdirSync(slots).filter(name => name.endsWith('.json')), []);
});

test('--rekey moves findings under a prefix to the new crate key and keeps every lint total', () => {
  const baseline = {
    lint: { runtime: { 'crates/runtime/src/contracts/a.rs': 2, 'crates/runtime/src/other.rs': 1 }, 'runtime-base': { 'crates/runtime-base/src/contracts/a.rs': 1 } },
    other: { runtime: { 'crates/runtime/src/contracts/b.rs': 4 } },
  };
  const { entries, moved, lints } = rekey(baseline, 'crates/runtime/src/contracts/', 'crates/runtime-base/src/contracts/');
  assert.deepEqual(entries, {
    lint: { runtime: { 'crates/runtime/src/other.rs': 1 }, 'runtime-base': { 'crates/runtime-base/src/contracts/a.rs': 3 } },
    other: { 'runtime-base': { 'crates/runtime-base/src/contracts/b.rs': 4 } },
  });
  assert.deepEqual(moved, { files: 2, findings: 6 });
  assert.deepEqual(lints, { lint: 4, other: 4 });
  assert.deepEqual(rekey(baseline, 'crates/absent/', 'crates/x/').moved, { files: 0, findings: 0 });
});

test('--rekey takes a bare crate name as its folder and refuses paths outside the native package', () => {
  assert.equal(rekeyPrefix('runtime'), 'crates/runtime/');
  assert.equal(rekeyPrefix('./crates/io/src/'), 'crates/io/src/');
  for (const bad of ['', undefined, '/abs/x', '../x/y']) assert.throws(() => rekeyPrefix(bad), { code: 'invalidArguments' });
  const run = spawnSync(process.execPath, [fileURLToPath(new URL('./rust-lint.mjs', import.meta.url)), '--rekey', 'io', 'io'], { encoding: 'utf8' });
  assert.equal(run.status, 2);
  assert.equal(JSON.parse(run.stdout).error.code, 'invalidArguments');
});
