import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test } from 'vitest';
import {
  CORE_PACKAGE_NAME,
  checkForCoreUpdate,
  isUpdateCheckDisabled,
  parseNpmViewVersion,
  readOwnVersion,
} from '../src/core-update-check.js';

// ─── isUpdateCheckDisabled ───────────────────────────────────────────────────

test('isUpdateCheckDisabled honors the same PI_OFFLINE convention Pi itself uses', () => {
  assert.equal(isUpdateCheckDisabled({}), false);
  assert.equal(isUpdateCheckDisabled({ PI_OFFLINE: '0' }), false);
  assert.equal(isUpdateCheckDisabled({ PI_OFFLINE: 'false' }), false);
  assert.equal(isUpdateCheckDisabled({ PI_OFFLINE: '1' }), true);
  assert.equal(isUpdateCheckDisabled({ PI_OFFLINE: 'true' }), true);
  assert.equal(isUpdateCheckDisabled({ PI_OFFLINE: 'TRUE' }), true);
  assert.equal(isUpdateCheckDisabled({ PI_OFFLINE: 'yes' }), true);
});

// ─── parseNpmViewVersion ──────────────────────────────────────────────────────

test('parseNpmViewVersion extracts the version from npm view --json output', () => {
  assert.equal(parseNpmViewVersion('"1.4.2"'), '1.4.2');
  assert.equal(parseNpmViewVersion('"1.4.2"\n'), '1.4.2');
});

test('parseNpmViewVersion returns undefined for empty or malformed output, never throws', () => {
  assert.equal(parseNpmViewVersion(''), undefined);
  assert.equal(parseNpmViewVersion('   '), undefined);
  assert.equal(parseNpmViewVersion('not json'), undefined);
  assert.equal(parseNpmViewVersion('{}'), undefined);
  assert.equal(parseNpmViewVersion('null'), undefined);
});

// ─── readOwnVersion ────────────────────────────────────────────────────────────

test('readOwnVersion reads the version field from package.json one level above baseDir', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'core-update-check-test-'));
  try {
    fs.writeFileSync(path.join(tmp, 'package.json'), JSON.stringify({ version: '2.3.4' }));
    const distDir = path.join(tmp, 'dist');
    fs.mkdirSync(distDir);
    assert.equal(readOwnVersion(distDir), '2.3.4');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('readOwnVersion returns undefined when package.json is missing or has no version', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'core-update-check-test-'));
  try {
    const distDir = path.join(tmp, 'dist');
    fs.mkdirSync(distDir);
    assert.equal(readOwnVersion(distDir), undefined, 'missing package.json');

    fs.writeFileSync(path.join(tmp, 'package.json'), JSON.stringify({ name: 'x' }));
    assert.equal(readOwnVersion(distDir), undefined, 'package.json with no version field');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

// ─── checkForCoreUpdate ────────────────────────────────────────────────────────

const previousPiOffline = process.env['PI_OFFLINE'];
beforeEach(() => {
  delete process.env['PI_OFFLINE'];
});
afterEach(() => {
  if (previousPiOffline === undefined) delete process.env['PI_OFFLINE'];
  else process.env['PI_OFFLINE'] = previousPiOffline;
});

test('checkForCoreUpdate returns undefined when the current version is unknown', async () => {
  const result = await checkForCoreUpdate(undefined, {}, {
    runNpmView: async () => '"9.9.9"',
  });
  assert.equal(result, undefined);
});

test('checkForCoreUpdate returns undefined and never calls npm view when PI_OFFLINE is set', async () => {
  let called = false;
  const result = await checkForCoreUpdate('1.0.0', { PI_OFFLINE: '1' }, {
    runNpmView: async () => {
      called = true;
      return '"9.9.9"';
    },
  });
  assert.equal(result, undefined);
  assert.equal(called, false, 'npm view must not run when offline mode is enabled');
});

test('checkForCoreUpdate returns undefined when already up to date', async () => {
  const result = await checkForCoreUpdate('1.4.2', {}, {
    runNpmView: async () => '"1.4.2"',
  });
  assert.equal(result, undefined);
});

test('checkForCoreUpdate returns the update info when the registry version differs', async () => {
  let requestedPackage: string | undefined;
  const result = await checkForCoreUpdate('1.4.2', {}, {
    runNpmView: async (pkg) => {
      requestedPackage = pkg;
      return '"1.5.0"';
    },
  });
  assert.equal(requestedPackage, CORE_PACKAGE_NAME);
  assert.deepEqual(result, { currentVersion: '1.4.2', latestVersion: '1.5.0' });
});

test('checkForCoreUpdate treats a locally-ahead version as "an update exists" too, matching Pi\'s own !== comparison', async () => {
  // Mirrors DefaultPackageManager#npmHasAvailableUpdate: any difference counts,
  // not just a semver-greater registry version — e.g. a local/dev build ahead
  // of what's published still gets flagged as different, not silently ignored.
  const result = await checkForCoreUpdate('9.9.9-dev', {}, {
    runNpmView: async () => '"1.5.0"',
  });
  assert.deepEqual(result, { currentVersion: '9.9.9-dev', latestVersion: '1.5.0' });
});

test('checkForCoreUpdate returns undefined, never throws, when npm view rejects (timeout, ENOENT, network error)', async () => {
  const result = await checkForCoreUpdate('1.4.2', {}, {
    runNpmView: async () => {
      throw new Error('npm view timed out');
    },
  });
  assert.equal(result, undefined);
});

test('checkForCoreUpdate returns undefined, never throws, when npm view returns garbage output', async () => {
  const result = await checkForCoreUpdate('1.4.2', {}, {
    runNpmView: async () => 'not valid json at all',
  });
  assert.equal(result, undefined);
});
