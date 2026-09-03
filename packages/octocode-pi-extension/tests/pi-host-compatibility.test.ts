import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  APPROVED_PI_HOST_VERSION,
  PiHostCompatibilityError,
  assertSupportedPiHostVersion,
  resolveInstalledPiHostVersion,
  resolvePiHostVersion,
} from '../src/adapters/pi-host-compatibility.js';

test('accepts only the exact approved Pi host version', () => {
  assert.doesNotThrow(() => assertSupportedPiHostVersion(APPROVED_PI_HOST_VERSION));

  for (const version of ['0.84.1', '0.84.2', '0.84.3', '0.85.0', '^0.84.4', undefined]) {
    assert.throws(
      () => assertSupportedPiHostVersion(version),
      (error: unknown) => {
        assert.ok(error instanceof PiHostCompatibilityError);
        assert.equal(error.expectedVersion, '0.84.4');
        assert.equal(error.actualVersion, version);
        assert.equal(error.code, 'OCTOCODE_PI_HOST_INCOMPATIBLE');
        return true;
      },
    );
  }
});

test('uses explicit host metadata before installed package metadata', () => {
  assert.equal(resolvePiHostVersion({ hostVersion: '0.84.4' }, () => '0.84.1'), '0.84.4');
  assert.equal(resolvePiHostVersion({ version: '0.84.4' }, () => '0.84.1'), '0.84.4');
  assert.equal(resolvePiHostVersion({}, () => '0.84.4'), '0.84.4');
});

test('resolves the installed Pi peer metadata through its restricted export map', () => {
  assert.equal(resolveInstalledPiHostVersion(), APPROVED_PI_HOST_VERSION);
});

test('ignores malformed explicit metadata and fails closed when resolution is unavailable', () => {
  assert.equal(resolvePiHostVersion({ hostVersion: 'latest' }, () => undefined), undefined);
  assert.equal(resolvePiHostVersion({ version: '^0.84.4' }, () => undefined), undefined);
  assert.throws(() => assertSupportedPiHostVersion(resolvePiHostVersion({}, () => undefined)), PiHostCompatibilityError);
});
