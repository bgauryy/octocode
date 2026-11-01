'use strict';
const assert = require('node:assert/strict');
const { PLATFORMS, getPlatformSuffix } = require('../bin/platform.cjs');
for (const [platform, arch, musl, expected] of [
  ['darwin', 'arm64', false, 'darwin-arm64'],
  ['darwin', 'x64', false, 'darwin-x64'],
  ['win32', 'x64', false, 'win32-x64-msvc'],
  ['win32', 'arm64', false, 'win32-arm64-msvc'],
  ['linux', 'x64', false, 'linux-x64-gnu'],
  ['linux', 'x64', true, 'linux-x64-musl'],
  ['linux', 'arm64', false, 'linux-arm64-gnu'],
  ['linux', 'arm64', true, 'linux-arm64-musl'],
  ['freebsd', 'x64', false, null],
]) {
  assert.equal(getPlatformSuffix({ platform, arch, musl }), expected);
}
assert.ok(Object.hasOwn(PLATFORMS, getPlatformSuffix()));

// The libc answer comes from ldd's text when it names its libc: no process
// report and no `ldd --version` spawn on the per-launch path.
const lddOnly = ldd => ({
  platform: 'linux',
  arch: 'x64',
  readLdd: () => ldd,
  get report() {
    throw new Error('process report read although ldd named its libc');
  },
  get probe() {
    throw new Error('ldd spawned although ldd named its libc');
  },
});
for (const [ldd, expected] of [
  ['#!/bin/bash\n# This file is part of the GNU C Library.\n', 'linux-x64-gnu'],
  ['\x7fELF ... /lib/ld-musl-x86_64.so.1 musl libc', 'linux-x64-musl'],
]) {
  assert.equal(getPlatformSuffix(lddOnly(ldd)), expected);
}
// Neither libc named (or no ldd): the process report decides.
assert.equal(
  getPlatformSuffix({ platform: 'linux', arch: 'x64', readLdd: () => '', report: { header: { glibcVersionRuntime: '2.39' } } }),
  'linux-x64-gnu'
);
assert.equal(
  getPlatformSuffix({
    platform: 'linux',
    arch: 'arm64',
    readLdd: () => {
      throw new Error('ENOENT');
    },
    report: { header: {} },
    probe: { stdout: '', stderr: 'musl libc (aarch64)' },
  }),
  'linux-arm64-musl'
);
console.log('platform detection ok');
