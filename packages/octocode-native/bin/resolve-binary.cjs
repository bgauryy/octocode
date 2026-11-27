'use strict';

/**
 * Shared platform → binary resolver: the single place that maps the current
 * platform to its compiled native binary. The `octocode` and `octocode-mcp`
 * launchers bundle it and spawn the binary directly.
 */

const { join } = require('path');
const { existsSync } = require('fs');
const { PLATFORMS, executableName, getPlatformSuffix, platformPackageName } = require('./platform.cjs');

/** `error.code` when this platform's package was not installed (a skipped optional dependency). */
const PLATFORM_PACKAGE_MISSING = 'OCTOCODE_PLATFORM_PACKAGE_MISSING';

function packageDir(name) {
  // require.resolve finds the package.json; strip it to get the dir.
  return require.resolve(`${name}/package.json`).replace(/[\/\\]package\.json$/, '');
}

/** Platform packages installed for other platforms: node_modules copied from another machine. */
function foreignPlatformPackages(suffix) {
  return Object.keys(PLATFORMS).filter(other => {
    if (other === suffix) return false;
    try {
      packageDir(platformPackageName(other));
      return true;
    } catch {
      return false;
    }
  });
}

function missingPackageError(suffix) {
  const name = platformPackageName(suffix);
  const foreign = foreignPlatformPackages(suffix);
  const reason = foreign.length
    ? `node_modules holds ${foreign.join(', ')} instead: it was installed on another platform ` +
      '(copied into a container, VM or another machine). Reinstall dependencies on this machine.'
    : 'The optional dependency was skipped (--omit=optional, --no-optional, or a lockfile written ' +
      'on another platform). Reinstall with optional dependencies enabled.';
  return Object.assign(new Error(`octocode: platform package '${name}' is not installed.\n${reason}`), {
    code: PLATFORM_PACKAGE_MISSING,
    suffix,
  });
}

/**
 * Resolve the absolute path to the native binary for the current platform.
 * Throws a descriptive Error when the platform is unsupported, the platform
 * package was not installed (`code: PLATFORM_PACKAGE_MISSING`), or the binary
 * is absent.
 *
 * @returns {string} absolute path to the native binary
 */
function resolveNativeBinaryPath() {
  const suffix = getPlatformSuffix();
  if (!suffix) {
    const key = `${process.platform}-${process.arch}`;
    throw new Error(
      `octocode: unsupported platform '${key}'.\n` +
        `Supported: ${Object.keys(PLATFORMS).join(', ')}`
    );
  }

  let dir;
  try {
    dir = packageDir(platformPackageName(suffix));
  } catch {
    throw missingPackageError(suffix);
  }

  const binaryPath = join(dir, executableName('octocode'));
  if (!existsSync(binaryPath)) {
    throw new Error(
      `octocode: binary not found at '${binaryPath}'.\n` +
        'Rebuild: yarn workspace @octocodeai/octocode-native build'
    );
  }
  return binaryPath;
}

module.exports = { PLATFORM_PACKAGE_MISSING, resolveNativeBinaryPath };
