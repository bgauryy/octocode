'use strict';

const { readFileSync } = require('fs');
const { spawnSync } = require('child_process');

/** Every published platform package suffix; `package.json` optionalDependencies must match. */
const PLATFORMS = {
  'darwin-arm64': { triple: 'aarch64-apple-darwin', os: 'darwin', arch: 'arm64', libc: null },
  'darwin-x64': { triple: 'x86_64-apple-darwin', os: 'darwin', arch: 'x64', libc: null },
  'linux-arm64-gnu': { triple: 'aarch64-unknown-linux-gnu', os: 'linux', arch: 'arm64', libc: 'gnu' },
  'linux-x64-gnu': { triple: 'x86_64-unknown-linux-gnu', os: 'linux', arch: 'x64', libc: 'gnu' },
  'linux-x64-musl': { triple: 'x86_64-unknown-linux-musl', os: 'linux', arch: 'x64', libc: 'musl' },
  'win32-x64-msvc': { triple: 'x86_64-pc-windows-msvc', os: 'win32', arch: 'x64', libc: null },
};

const BINARIES = ['octocode', 'octocode-regex-worker'];

function isMusl(system = {}) {
  const platform = system.platform ?? process.platform;
  if (platform !== 'linux') return false;
  try {
    const ldd = system.readLdd?.() ?? readFileSync('/usr/bin/ldd', 'utf8');
    if (ldd.toLowerCase().includes('musl'))
      return true;
  } catch {}
  const report = system.report ?? process.report?.getReport?.();
  if (report?.header?.glibcVersionRuntime) return false;
  const probe = system.probe ?? spawnSync('ldd', ['--version'], { encoding: 'utf8' });
  return `${probe.stdout ?? ''}${probe.stderr ?? ''}`
    .toLowerCase()
    .includes('musl');
}

function getPlatformSuffix(system = {}) {
  const platform = system.platform ?? process.platform;
  const arch = system.arch ?? process.arch;
  const libc = platform === 'linux' ? ((system.musl ?? isMusl(system)) ? 'musl' : 'gnu') : null;
  const match = Object.entries(PLATFORMS).find(
    ([, target]) => target.os === platform && target.arch === arch && target.libc === libc
  );
  return match ? match[0] : null;
}

/** Executable file name of a native binary (`octocode`, `octocode-regex-worker`) for an OS. */
function executableName(binary, os = process.platform) {
  return os === 'win32' ? `${binary}.exe` : binary;
}

module.exports = { BINARIES, PLATFORMS, executableName, getPlatformSuffix, isMusl };
