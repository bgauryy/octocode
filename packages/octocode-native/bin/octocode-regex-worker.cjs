#!/usr/bin/env node
/**
 * Platform-selecting shim for the native `octocode-regex-worker` binary.
 * See bin/octocode.cjs for the full explanation.
 */
'use strict';

const { spawnSync } = require('child_process');
const { join } = require('path');
const { existsSync } = require('fs');
const { getPlatformSuffix } = require('./platform.cjs');

const isWindows = process.platform === 'win32';

const key = `${process.platform}-${process.arch}`;
const platformSuffix = getPlatformSuffix();

if (!platformSuffix) {
  console.error(
    `octocode-regex-worker: unsupported platform '${key}'.\n` +
      'Supported: darwin arm64/x64, Linux arm64 GNU, Linux x64 GNU/musl, Windows x64'
  );
  process.exit(1);
}

const pkgName = `@octocodeai/octocode-native-${platformSuffix}`;
const binaryName = isWindows
  ? 'octocode-regex-worker.exe'
  : 'octocode-regex-worker';

let pkgDir;
try {
  pkgDir = require
    .resolve(`${pkgName}/package.json`)
    .replace(/[\/\\]package\.json$/, '');
} catch {
  console.error(
    `octocode-regex-worker: platform package '${pkgName}' is not installed.\n` +
      `Try: npm install ${pkgName}`
  );
  process.exit(1);
}

const binaryPath = join(pkgDir, binaryName);

if (!existsSync(binaryPath)) {
  console.error(`octocode-regex-worker: binary not found at '${binaryPath}'.`);
  process.exit(1);
}

const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: 'inherit',
  windowsHide: false,
});

if (result.error) {
  console.error(
    `octocode-regex-worker: failed to start binary: ${result.error.message}`
  );
  process.exit(1);
}

process.exit(result.status ?? 1);
