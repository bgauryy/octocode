'use strict';

const { existsSync } = require('fs');
const { join } = require('path');
const { getPlatformSuffix } = require('../bin/platform.cjs');

const packageName = '@octocodeai/octocode-native';
const binaryName = 'octocode-engine';

function loadNativeBinding() {
  const key = getPlatformSuffix();
  if (!key) {
    const error = new Error(
      `${packageName}/engine does not ship a native addon for ${process.platform}-${process.arch}`
    );
    error.code = 'OCTOCODE_ENGINE_NATIVE_LOAD_FAILED';
    throw error;
  }

  const candidates = [
    join(__dirname, '..', `${binaryName}.${key}.node`),
    join(__dirname, '..', 'npm', key, `${binaryName}.${key}.node`),
    join(__dirname, '..', 'runtime', 'engine', `${binaryName}.${key}.node`),
    join(__dirname, '..', '..', 'runtime', 'engine', `${binaryName}.${key}.node`),
  ];
  const loadErrors = [];
  for (const candidate of candidates) {
    if (!existsSync(candidate)) continue;
    try {
      return require(candidate);
    } catch (error) {
      loadErrors.push(`${candidate}: ${error?.message ?? error}`);
    }
  }

  const platformEntry = `${packageName}-${key}/engine`;
  try {
    return require(platformEntry);
  } catch (error) {
    loadErrors.push(`${platformEntry}: ${error?.message ?? error}`);
  }

  const detail = loadErrors.length
    ? `\nNative load attempts:\n  - ${loadErrors.join('\n  - ')}`
    : `\nNo native binary found for ${key}.`;
  const error = new Error(
    `${packageName}/engine: could not load the native ${binaryName} addon for ${key}.` +
      detail +
      `\nIf a .node file exists above but failed to load, the current Node runtime may reject native addons. Re-run with system Node (\`which node\`).`
  );
  error.code = 'OCTOCODE_ENGINE_NATIVE_LOAD_FAILED';
  throw error;
}

const nativeBinding =
  globalThis.__OCTOCODE_ENGINE_BINDING__ ?? loadNativeBinding();

nativeBinding.SUPPORTED_SIGNATURE_EXTENSIONS = Object.freeze(
  nativeBinding.getSupportedSignatureExtensions().sort()
);
nativeBinding.SUPPORTED_GRAPH_FACT_EXTENSIONS = Object.freeze(
  typeof nativeBinding.getSupportedGraphFactExtensions === 'function'
    ? nativeBinding.getSupportedGraphFactExtensions().sort()
    : []
);
nativeBinding.SUPPORTED_STRUCTURAL_EXTENSIONS = Object.freeze(
  nativeBinding.getSupportedStructuralExtensions().sort()
);

module.exports = nativeBinding;
