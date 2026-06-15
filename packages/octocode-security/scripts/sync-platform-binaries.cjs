/**
 * Copy built native binaries into the per-platform optional package dirs.
 * The root package intentionally excludes `.node` files; these packages are
 * the publishable native artifacts.
 */
'use strict';

const { spawnSync } = require('child_process');
const { copyFileSync, existsSync, unlinkSync } = require('fs');
const { join } = require('path');

const PLATFORM_PACKAGES = {
  'darwin-arm64': 'darwin-arm64',
  'darwin-x64': 'darwin-x64',
  'linux-arm64-gnu': 'linux-arm64-gnu',
  'linux-x64-gnu': 'linux-x64-gnu',
  'linux-x64-musl': 'linux-x64-musl',
  'win32-x64-msvc': 'win32-x64-msvc',
};

let copied = 0;

function adHocSignDarwinBinary(binaryPath) {
  if (process.platform !== 'darwin') {
    return;
  }

  runCodesign(['--force', '--sign', '-', binaryPath], `sign ${binaryPath}`);
  runCodesign(
    ['--verify', '--strict', '--verbose=2', binaryPath],
    `verify ${binaryPath}`
  );
}

function runCodesign(args, action) {
  const result = spawnSync('codesign', args, { encoding: 'utf8' });
  if (result.status !== 0) {
    const details = [result.stdout, result.stderr].filter(Boolean).join('\n');
    throw new Error(`Failed to ${action}\n${details}`);
  }
}

for (const [triple, dirName] of Object.entries(PLATFORM_PACKAGES)) {
  const binaryName = `octocode-security.${triple}.node`;
  const sourcePath = join(__dirname, '..', binaryName);
  const packageDir = join(__dirname, '..', 'npm', dirName);
  const destinationPath = join(packageDir, binaryName);

  if (!existsSync(sourcePath)) {
    continue;
  }

  if (!existsSync(packageDir)) {
    throw new Error(
      `Missing optional package dir for ${binaryName}: ${packageDir}`
    );
  }

  copyFileSync(sourcePath, destinationPath);
  if (triple.startsWith('darwin-')) {
    adHocSignDarwinBinary(destinationPath);
  }
  unlinkSync(sourcePath);
  copied += 1;
  console.log(`${binaryName} copied to npm/${dirName}; root artifact removed`);
}

if (copied === 0) {
  console.warn('No platform binaries found to copy');
}
