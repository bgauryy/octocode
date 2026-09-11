import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import {
  writeEphemeralFileAtomicSync,
  writePrivateFileAtomicSync,
} from '../src/tools/atomic-state-file.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

function fixture(): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-atomic-state-'));
  roots.push(root);
  return root;
}

test('private state publication creates private parents and leaves no temporary artifact', () => {
  const root = fixture();
  const target = path.join(root, 'private', 'nested', 'state.json');
  writePrivateFileAtomicSync(target, '{"ok":true}\n');
  assert.equal(fs.readFileSync(target, 'utf8'), '{"ok":true}\n');
  assert.deepEqual(fs.readdirSync(path.dirname(target)), ['state.json']);
  if (process.platform !== 'win32') {
    assert.equal(fs.statSync(path.dirname(target)).mode & 0o777, 0o700);
    assert.equal(fs.statSync(target).mode & 0o777, 0o600);
  }
});

test('ephemeral publication atomically replaces content and leaves no temporary artifact', () => {
  const root = fixture();
  const target = path.join(root, 'workspace', 'discovery.json');
  writeEphemeralFileAtomicSync(target, 'first');
  writeEphemeralFileAtomicSync(target, 'second');
  assert.equal(fs.readFileSync(target, 'utf8'), 'second');
  assert.deepEqual(fs.readdirSync(path.dirname(target)), ['discovery.json']);
});
