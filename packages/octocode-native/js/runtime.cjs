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
// Avoid a top-level `return`: it is legal in CommonJS but trips tools that
// parse this file as a plain script/ESM (e.g. Vite/Rolldown SSR transforms
// during tests), so break out of the loop on the first successful load.
let loaded = false;
for (const candidate of candidates) {
  if (!existsSync(candidate)) continue;
  try {
    module.exports = require(candidate);
    loaded = true;
    break;
  } catch (error) {
    loadErrors.push(`${candidate}: ${error?.message ?? error}`);
  }
}

if (!loaded) {
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
}
