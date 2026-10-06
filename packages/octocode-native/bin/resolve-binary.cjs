'use strict';

/**
 * Shared platform → binary resolver: the single place that maps the current
 * platform to a compiled native binary. Used by this package's launcher bins
 * (`octocode.cjs`, `octocode-regex-worker.cjs`) and by the `octocode` npm CLI's
 * delegation boundary, so the platform-selection logic is never duplicated and
 * the npm CLI spawns the native binary directly instead of routing through a
 * second shim.
 */

const { join } = require('path');
const { existsSync } = require('fs');
const { executableName, getPlatformSuffix } = require('./platform.cjs');

/**
 * Resolve the absolute path to a compiled native binary for the current
 * platform. Throws a descriptive Error when the platform is unsupported, the
 * platform package was not installed, or the binary is absent.
 *
 * @param {string} [binary='octocode'] native binary name without extension
 * @returns {string} absolute path to the native binary
 */
function resolveNativeBinaryPath(binary = 'octocode') {
  const suffix = getPlatformSuffix();
  if (!suffix) {
    const key = `${process.platform}-${process.arch}`;
    throw new Error(
      `${binary}: unsupported platform '${key}'.\n` +
        'Supported: darwin arm64/x64, Linux arm64 GNU, Linux x64 GNU/musl, Windows x64'
    );
  }

  const pkgName = `@octocodeai/octocode-native-${suffix}`;

  let pkgDir;
  try {
    // require.resolve finds the package.json; strip it to get the dir.
    pkgDir = require
      .resolve(`${pkgName}/package.json`)
      .replace(/[\/\\]package\.json$/, '');
  } catch {
    throw new Error(
      `${binary}: platform package '${pkgName}' is not installed.\n` +
        'This usually means the optional dependency was skipped.\n' +
        `Try: npm install ${pkgName}`
    );
  }

  const binaryPath = join(pkgDir, executableName(binary));
  if (!existsSync(binaryPath)) {
    throw new Error(
      `${binary}: binary not found at '${binaryPath}'.\n` +
        'Rebuild: yarn workspace @octocodeai/octocode-native build'
    );
  }
  return binaryPath;
}

module.exports = { resolveNativeBinaryPath };
