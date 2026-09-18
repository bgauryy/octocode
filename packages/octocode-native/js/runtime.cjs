'use strict';

const { existsSync } = require('fs');
const { join } = require('path');
const { getPlatformSuffix } = require('../bin/platform.cjs');

const suffix = getPlatformSuffix();
if (!suffix) {
  throw new Error(
    `@octocodeai/octocode-native does not ship a runtime addon for ${process.platform}-${process.arch}`
  );
}

const binaryName = `octocode-native.${suffix}.node`;
const candidates = [
  join(__dirname, '..', binaryName),
  join(__dirname, '..', 'npm', suffix, binaryName),
];
const loadErrors = [];
for (const candidate of candidates) {
  if (!existsSync(candidate)) continue;
  try {
    module.exports = require(candidate);
    return;
  } catch (error) {
    loadErrors.push(`${candidate}: ${error?.message ?? error}`);
  }
}

try {
  module.exports = require(`@octocodeai/octocode-native-${suffix}/runtime`);
} catch (error) {
  loadErrors.push(
    `@octocodeai/octocode-native-${suffix}/runtime: ${error?.message ?? error}`
  );
  const failure = new Error(
    `@octocodeai/octocode-native: could not load the runtime addon for ${suffix}.` +
      `\nNative load attempts:\n  - ${loadErrors.join('\n  - ')}`
  );
  failure.code = 'OCTOCODE_NATIVE_LOAD_FAILED';
  throw failure;
}
