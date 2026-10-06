'use strict';
const assert = require('node:assert/strict');
const { PLATFORMS, getPlatformSuffix } = require('../bin/platform.cjs');
const { optionalDependencies } = require('../package.json');
for (const [platform, arch, musl, expected] of [
  ['darwin', 'arm64', false, 'darwin-arm64'],
  ['darwin', 'x64', false, 'darwin-x64'],
  ['win32', 'x64', false, 'win32-x64-msvc'],
  ['linux', 'x64', false, 'linux-x64-gnu'],
  ['linux', 'x64', true, 'linux-x64-musl'],
  ['linux', 'arm64', false, 'linux-arm64-gnu'],
  ['linux', 'arm64', true, null],
  ['freebsd', 'x64', false, null],
]) {
  assert.equal(getPlatformSuffix({ platform, arch, musl }), expected);
}
assert.ok(Object.hasOwn(PLATFORMS, getPlatformSuffix()));
assert.deepEqual(
  Object.keys(optionalDependencies).sort(),
  Object.keys(PLATFORMS).map(suffix => `@octocodeai/octocode-native-${suffix}`).sort()
);
console.log('platform detection ok');
