import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { exceedsSize, nativeDebugDir } from './lib/native-target.mjs';

function withTree(run) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'native-target-'));
  try {
    run(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test('size is compared across nested directories', () => {
  withTree(dir => {
    fs.mkdirSync(path.join(dir, 'deps'));
    fs.writeFileSync(path.join(dir, 'a'), Buffer.alloc(60));
    fs.writeFileSync(path.join(dir, 'deps', 'b'), Buffer.alloc(60));
    assert.equal(exceedsSize(dir, 119), true);
    assert.equal(exceedsSize(dir, 120), false);
  });
});

test('a hardlinked file counts once', () => {
  withTree(dir => {
    fs.writeFileSync(path.join(dir, 'a'), Buffer.alloc(100));
    fs.linkSync(path.join(dir, 'a'), path.join(dir, 'b'));
    assert.equal(exceedsSize(dir, 100), false);
  });
});

test('a missing directory is empty', () => {
  withTree(dir => {
    assert.equal(exceedsSize(path.join(dir, 'absent'), 0), false);
  });
});

test('the debug directory follows CARGO_TARGET_DIR', () => {
  assert.equal(nativeDebugDir({ CARGO_TARGET_DIR: '/scratch/lane' }), path.join('/scratch/lane', 'debug'));
  assert.ok(nativeDebugDir({}).endsWith(path.join('packages', 'octocode-native', 'target', 'debug')));
});
