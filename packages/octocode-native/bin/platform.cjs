'use strict';

const { readFileSync } = require('fs');
const { spawnSync } = require('child_process');

function isMusl() {
  if (process.platform !== 'linux') return false;
  try {
    if (readFileSync('/usr/bin/ldd', 'utf8').toLowerCase().includes('musl'))
      return true;
  } catch {}
  const report = process.report?.getReport?.();
  if (report?.header?.glibcVersionRuntime) return false;
  const probe = spawnSync('ldd', ['--version'], { encoding: 'utf8' });
  return `${probe.stdout ?? ''}${probe.stderr ?? ''}`
    .toLowerCase()
    .includes('musl');
}

function getPlatformSuffix() {
  if (process.platform === 'darwin' && process.arch === 'arm64')
    return 'darwin-arm64';
  if (process.platform === 'darwin' && process.arch === 'x64')
    return 'darwin-x64';
  if (process.platform === 'win32' && process.arch === 'x64')
    return 'win32-x64-msvc';
  if (process.platform === 'linux' && process.arch === 'x64')
    return `linux-x64-${isMusl() ? 'musl' : 'gnu'}`;
  if (process.platform === 'linux' && process.arch === 'arm64' && !isMusl())
    return 'linux-arm64-gnu';
  return null;
}

module.exports = { getPlatformSuffix, isMusl };
