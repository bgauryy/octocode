import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { KEEP, listPins, pin } from './pin.mjs';

const SCRIPT = path.join(path.dirname(fileURLToPath(import.meta.url)), 'pin.mjs');
const posix = process.platform !== 'win32';

/** A stand-in binary that answers --version like the native one. */
function fakeBinary(dir, version, body = '') {
  const file = path.join(dir, `bin-${version}`, 'octocode');
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `#!/bin/sh\n# ${body}\necho "octocode ${version}"\n`, { mode: 0o755 });
  return file;
}

function tempDir(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-pin-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return dir;
}

test('a pin is a runnable copy named by version and hash, with its record', { skip: !posix }, t => {
  const dir = tempDir(t);
  const source = fakeBinary(dir, '20.2.0+abc123');
  const result = pin({ source, dir: path.join(dir, 'pins'), now: '2026-11-24T00:00:01Z' });
  assert.equal(result.ok, true);
  assert.equal(result.version, '20.2.0+abc123');
  assert.match(result.sha256, /^[0-9a-f]{64}$/);
  assert.notEqual(result.path, source);
  assert.equal(path.basename(path.dirname(result.path)), `20.2.0+abc123-${result.sha256.slice(0, 12)}`);
  assert.deepEqual(fs.readFileSync(result.path), fs.readFileSync(source));
  assert.equal(spawnSync(result.path, ['--version'], { encoding: 'utf8' }).stdout.trim(), 'octocode 20.2.0+abc123');
  assert.equal(result.previous, null);
});

test('only the current and previous pins stay; re-pinning the same bytes reuses the pin', { skip: !posix }, t => {
  const dir = tempDir(t);
  const pins = path.join(dir, 'pins');
  const sources = ['1.0.0', '1.0.1', '1.0.2'].map(version => fakeBinary(dir, version));
  const results = sources.map((source, i) => pin({ source, dir: pins, now: `2026-11-24T00:00:0${i}Z` }));
  assert.equal(listPins(pins).length, KEEP);
  assert.deepEqual(results[2].removed, [path.dirname(results[0].path)]);
  assert.equal(fs.existsSync(results[0].path), false);
  assert.equal(results[2].previous.path, results[1].path);

  const again = pin({ source: sources[1], dir: pins, now: '2026-11-24T00:00:09Z' });
  assert.equal(again.reused, true);
  assert.equal(again.path, results[1].path);
  assert.equal(again.previous.path, results[2].path, 'the other kept pin becomes the previous one');
  assert.equal(listPins(pins)[0].path, results[1].path);
});

test('no built binary is refused with noBinary and exit 1; a bad flag exits 2', t => {
  const dir = tempDir(t);
  const missing = spawnSync(process.execPath, [SCRIPT, '--from', path.join(dir, 'absent'), '--dir', path.join(dir, 'pins')], { encoding: 'utf8' });
  assert.equal(missing.status, 1);
  assert.equal(JSON.parse(missing.stdout).error.code, 'noBinary');
  assert.equal(fs.existsSync(path.join(dir, 'pins')), false, 'nothing is written');
  const bad = spawnSync(process.execPath, [SCRIPT, '--keep', '3'], { encoding: 'utf8' });
  assert.equal(bad.status, 2);
  assert.equal(JSON.parse(bad.stdout).error.code, 'invalidArguments');
});

test('a binary whose copy cannot run is refused and leaves no pin', { skip: !posix }, t => {
  const dir = tempDir(t);
  const source = path.join(dir, 'octocode');
  fs.writeFileSync(source, '#!/bin/sh\nexit 3\n', { mode: 0o755 });
  assert.throws(() => pin({ source, dir: path.join(dir, 'pins') }), { code: 'binaryFailed' });
  assert.deepEqual(listPins(path.join(dir, 'pins')), []);
});
