'use strict';

const { readFileSync } = require('fs');
const { spawnSync } = require('child_process');

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
  const musl = system.musl ?? isMusl(system);
  if (platform === 'darwin' && arch === 'arm64')
    return 'darwin-arm64';
  if (platform === 'darwin' && arch === 'x64')
    return 'darwin-x64';
  if (platform === 'win32' && arch === 'x64')
    return 'win32-x64-msvc';
  if (platform === 'linux' && arch === 'x64')
    return `linux-x64-${musl ? 'musl' : 'gnu'}`;
  if (platform === 'linux' && arch === 'arm64' && !musl)
    return 'linux-arm64-gnu';
  return null;
}

module.exports = { getPlatformSuffix, isMusl };
