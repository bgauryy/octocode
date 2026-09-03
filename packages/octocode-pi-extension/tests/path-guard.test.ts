/**
 * TDD tests for path-guard.ts
 *
 * assertPathAllowed is security-critical: it prevents the edit and write tools
 * from touching files outside the allowed roots. Every branch deserves direct
 * unit coverage here so regressions surface immediately.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { beforeEach, afterEach, test } from 'vitest';
import { assertPathAllowed } from '../src/tools/path-guard.js';

let tmpDir: string;

beforeEach(() => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'guard-test-'));
});

afterEach(() => {
  fs.rmSync(tmpDir, { recursive: true, force: true });
});

// ─── Paths inside allowed roots ───────────────────────────────────────────────

test('allows a file directly inside cwd', () => {
  const target = path.join(tmpDir, 'output.txt');
  assert.doesNotThrow(() => assertPathAllowed(target, tmpDir));
});

test('allows a nested file inside cwd', () => {
  const target = path.join(tmpDir, 'nested', 'deep', 'file.ts');
  assert.doesNotThrow(() => assertPathAllowed(target, tmpDir));
});

test('allows a relative path that resolves into cwd', () => {
  assert.doesNotThrow(() => assertPathAllowed('output.txt', tmpDir));
});

test('allows a path inside the home directory', () => {
  const target = path.join(os.homedir(), '.octocode', 'test.txt');
  assert.doesNotThrow(() => assertPathAllowed(target, '/some/other/cwd'));
});

test('allows a path inside the OS temp dir', () => {
  const target = path.join(os.tmpdir(), 'vitest-safe-file.txt');
  assert.doesNotThrow(() => assertPathAllowed(target, '/some/other/cwd'));
});

test('allows /tmp spelling for macOS temp paths that realpath under /private', () => {
  if (process.platform !== 'darwin') return;
  assert.doesNotThrow(() => assertPathAllowed('/tmp/octocode-safe-file.txt', '/some/other/cwd'));
});

// ─── ALLOWED_PATHS env extension ─────────────────────────────────────────────

test('allows paths inside an ALLOWED_PATHS root', () => {
  const extraRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'allowed-extra-'));
  const target = path.join(extraRoot, 'safe.txt');
  const prev = process.env['ALLOWED_PATHS'];
  try {
    process.env['ALLOWED_PATHS'] = extraRoot;
    assert.doesNotThrow(() => assertPathAllowed(target, '/some/unrelated/cwd'));
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
    fs.rmSync(extraRoot, { recursive: true, force: true });
  }
});

test('ALLOWED_PATHS supports colon-separated entries', () => {
  const root1 = fs.mkdtempSync(path.join(os.tmpdir(), 'ap1-'));
  const root2 = fs.mkdtempSync(path.join(os.tmpdir(), 'ap2-'));
  const prev = process.env['ALLOWED_PATHS'];
  try {
    process.env['ALLOWED_PATHS'] = `${root1}:${root2}`;
    assert.doesNotThrow(() => assertPathAllowed(path.join(root2, 'file.txt'), '/other/cwd'));
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
    fs.rmSync(root1, { recursive: true, force: true });
    fs.rmSync(root2, { recursive: true, force: true });
  }
});

// ─── Blocked paths ────────────────────────────────────────────────────────────

test('blocks a path outside all roots', () => {
  // Use a unique tmp cwd so it cannot match any default root
  const isolatedCwd = fs.mkdtempSync(path.join(os.tmpdir(), 'isolated-'));
  const prev = process.env['ALLOWED_PATHS'];
  try {
    delete process.env['ALLOWED_PATHS'];
    // /usr/local/bin is outside cwd, home, tmp on all platforms
    assert.throws(
      () => assertPathAllowed('/usr/local/evil.txt', isolatedCwd),
      /outside the allowed roots/,
    );
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
    fs.rmSync(isolatedCwd, { recursive: true, force: true });
  }
});

test('blocks a relative path that escapes cwd via ".."', () => {
  // A deeply nested cwd so path.resolve('../../../etc') escapes it
  const deepCwd = path.join(tmpDir, 'a', 'b', 'c');
  fs.mkdirSync(deepCwd, { recursive: true });
  const prev = process.env['ALLOWED_PATHS'];
  try {
    delete process.env['ALLOWED_PATHS'];
    // Escape deep enough to land outside home/tmp (the file doesn't need to exist)
    assert.throws(
      () => assertPathAllowed('/usr/bin/evil', deepCwd),
      /outside the allowed roots/,
    );
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
  }
});

// ─── Error message quality ────────────────────────────────────────────────────

test('error message includes the action name', () => {
  const isolatedCwd = fs.mkdtempSync(path.join(os.tmpdir(), 'err-action-'));
  const prev = process.env['ALLOWED_PATHS'];
  try {
    delete process.env['ALLOWED_PATHS'];
    assert.throws(
      () => assertPathAllowed('/usr/local/evil.txt', isolatedCwd, 'scriptFile read'),
      /scriptFile read blocked/,
    );
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
    fs.rmSync(isolatedCwd, { recursive: true, force: true });
  }
});

test('error message names the blocked path', () => {
  const isolatedCwd = fs.mkdtempSync(path.join(os.tmpdir(), 'err-path-'));
  const prev = process.env['ALLOWED_PATHS'];
  try {
    delete process.env['ALLOWED_PATHS'];
    assert.throws(
      () => assertPathAllowed('/usr/local/evil.txt', isolatedCwd),
      /evil\.txt/,
    );
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
    fs.rmSync(isolatedCwd, { recursive: true, force: true });
  }
});

test('error message mentions ALLOWED_PATHS as the resolution path', () => {
  const isolatedCwd = fs.mkdtempSync(path.join(os.tmpdir(), 'err-hint-'));
  const prev = process.env['ALLOWED_PATHS'];
  try {
    delete process.env['ALLOWED_PATHS'];
    assert.throws(
      () => assertPathAllowed('/usr/local/evil.txt', isolatedCwd),
      /ALLOWED_PATHS/,
    );
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
    fs.rmSync(isolatedCwd, { recursive: true, force: true });
  }
});

// ─── Default action ───────────────────────────────────────────────────────────

test('default action is "access" when not supplied', () => {
  const isolatedCwd = fs.mkdtempSync(path.join(os.tmpdir(), 'err-default-action-'));
  const prev = process.env['ALLOWED_PATHS'];
  try {
    delete process.env['ALLOWED_PATHS'];
    assert.throws(
      () => assertPathAllowed('/usr/local/evil.txt', isolatedCwd),
      /access blocked/,
    );
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
    fs.rmSync(isolatedCwd, { recursive: true, force: true });
  }
});
