#!/usr/bin/env node
/**
 * The release workflow's job matrices, derived from `bin/platform.cjs`.
 *
 *   release-matrix.cjs   prints `build=`, `verify=` and `platforms=` lines for $GITHUB_OUTPUT
 *
 * build: one entry per platform, on the host that cross-links it with the
 * tool an owner's build uses: darwin on macOS (Apple SDK), Windows on Linux
 * (cargo-xwin), Linux on the Linux runner of the other CPU, so
 * build-native.cjs links it with cargo-zigbuild against GLIBC_FLOOR (a
 * same-CPU Linux build is native and links the runner's glibc).
 *
 * verify: each platform on its own OS and CPU. musl runs in Alpine and every
 * gnu platform also runs in an image whose glibc is GLIBC_FLOOR, both through
 * `smoke-binary.cjs --container` on the Linux runner of that CPU.
 */
'use strict';

const { PLATFORMS } = require('../bin/platform.cjs');
const { GLIBC_FLOOR } = require('./build-native.cjs');

/** GitHub-hosted runners by OS and CPU. */
const RUNNERS = {
  darwin: { arm64: 'macos-latest', x64: 'macos-15-intel' },
  linux: { arm64: 'ubuntu-24.04-arm', x64: 'ubuntu-latest' },
  win32: { arm64: 'windows-11-arm', x64: 'windows-latest' },
};

/** Images the Linux verify entries run in: Alpine for musl, and AlmaLinux 8, whose glibc is the floor. */
const IMAGES = { musl: 'node:24-alpine', glibcFloor: { image: 'almalinux:8', glibc: '2.28' } };

function buildRunner({ os, arch }) {
  if (os === 'darwin') return RUNNERS.darwin.arm64;
  if (os === 'win32') return RUNNERS.linux.x64;
  return Object.entries(RUNNERS.linux).find(([cpu]) => cpu !== arch)[1];
}

function releaseMatrix() {
  if (IMAGES.glibcFloor.glibc !== GLIBC_FLOOR) throw new Error(`${IMAGES.glibcFloor.image} is glibc ${IMAGES.glibcFloor.glibc}, GLIBC_FLOOR is ${GLIBC_FLOOR}`);
  const build = [];
  const verify = [];
  for (const [platform, target] of Object.entries(PLATFORMS)) {
    build.push({ platform, os: target.os, triple: target.triple, runner: buildRunner(target) });
    const runner = RUNNERS[target.os][target.arch];
    const entry = (container, glibc = null) => ({
      name: container ? `${platform} in ${container}` : platform,
      id: container ? `${platform}-${container.replace(/[^\w.-]+/g, '-')}` : platform,
      platform, arch: target.arch, runner, container, glibc,
    });
    if (target.libc === 'musl') verify.push(entry(IMAGES.musl));
    else verify.push(entry(null));
    if (target.libc === 'gnu') verify.push(entry(IMAGES.glibcFloor.image, IMAGES.glibcFloor.glibc));
  }
  return { build, verify, platforms: Object.keys(PLATFORMS) };
}

if (require.main === module) {
  const { build, verify, platforms } = releaseMatrix();
  console.log(`build=${JSON.stringify(build)}`);
  console.log(`verify=${JSON.stringify(verify)}`);
  console.log(`platforms=${platforms.join(' ')}`);
}

module.exports = { RUNNERS, releaseMatrix };
