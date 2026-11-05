import assert from 'node:assert/strict';
import { test } from 'node:test';
import { adopt, collect, compare, ratchetConfig, ratchetKey, serialize, shrink } from './rust-lint.mjs';

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
