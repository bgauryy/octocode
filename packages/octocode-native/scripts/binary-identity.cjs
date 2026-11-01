'use strict';

/**
 * Static identity of a staged platform binary, read from its bytes on any
 * host: executable format and CPU (Mach-O, ELF, PE), the ELF interpreter and
 * the newest GLIBC_ version it requires, and the embedded package version and
 * contract fingerprint. `smoke-binary.cjs` and `pack-release.cjs` check every
 * platform package against `bin/platform.cjs` with it; only a run on the
 * target proves the binary works.
 */

const { readFileSync, statSync } = require('fs');
const path = require('path');
const { PLATFORMS, executableName, platformPackageName } = require('../bin/platform.cjs');
const { GLIBC_FLOOR } = require('./build-native.cjs');

const REPO = path.join(__dirname, '..', '..', '..');
const PROVENANCE = path.join(REPO, 'packages', 'octocode-config', 'contract', 'provenance.json');

const FORMAT_BY_OS = { darwin: 'mach-o', linux: 'elf', win32: 'pe' };
const MACHO_CPU = { 0x0100000c: 'arm64', 0x01000007: 'x64' };
const ELF_MACHINE = { 183: 'arm64', 62: 'x64' };
const PE_MACHINE = { 0xaa64: 'arm64', 0x8664: 'x64' };
const PT_INTERP = 3;
const SHT_GNU_VERNEED = 0x6ffffffe;

/** The contract fingerprint the binary must embed: the generated provenance. */
function expectedFingerprint() {
  return JSON.parse(readFileSync(PROVENANCE, 'utf8')).contractFingerprint;
}

const cString = (bytes, offset) => bytes.toString('latin1', offset, bytes.indexOf(0, offset));

/** Dotted version compare: negative when `a` is older than `b`. */
function compareVersions(a, b) {
  const left = a.split('.').map(Number);
  const right = b.split('.').map(Number);
  for (let i = 0; i < Math.max(left.length, right.length); i++) {
    const diff = (left[i] ?? 0) - (right[i] ?? 0);
    if (diff) return diff;
  }
  return 0;
}

/** Newest `GLIBC_x.y` among version-need names, or null. */
function maxGlibc(names) {
  const versions = names.flatMap(name => /^GLIBC_(\d+(?:\.\d+)*)$/.exec(name)?.[1] ?? []);
  return versions.toSorted(compareVersions).at(-1) ?? null;
}

/** ELF64 little-endian: CPU, `PT_INTERP` path, and every `.gnu.version_r` need name. */
function readElf(bytes) {
  if (bytes[4] !== 2 || bytes[5] !== 1) return { format: 'elf', arch: 'unsupported ELF class', interpreter: null, versionNeeds: [] };
  const u16 = offset => bytes.readUInt16LE(offset);
  const u32 = offset => bytes.readUInt32LE(offset);
  const u64 = offset => Number(bytes.readBigUInt64LE(offset));
  const machine = u16(18);
  let interpreter = null;
  for (let i = 0, ph = u64(32); i < u16(56); i++, ph += u16(54)) {
    if (u32(ph) === PT_INTERP) interpreter = cString(bytes, u64(ph + 8));
  }
  const sections = Array.from({ length: u16(60) }, (_, i) => {
    const sh = u64(40) + i * u16(58);
    return { type: u32(sh + 4), offset: u64(sh + 24), link: u32(sh + 40), info: u32(sh + 44) };
  });
  const versionNeeds = [];
  for (const need of sections.filter(section => section.type === SHT_GNU_VERNEED)) {
    const strings = sections[need.link].offset;
    for (let i = 0, entry = need.offset; i < need.info; i++, entry += u32(entry + 12)) {
      for (let j = 0, aux = entry + u32(entry + 8); j < u16(entry + 2); j++, aux += u32(aux + 12)) {
        versionNeeds.push(cString(bytes, strings + u32(aux + 8)));
      }
    }
  }
  return { format: 'elf', arch: ELF_MACHINE[machine] ?? `e_machine ${machine}`, interpreter, versionNeeds };
}

/** Format and CPU of an executable's bytes; ELF adds its interpreter and version needs. */
function readHeader(bytes) {
  if (bytes.length >= 8 && bytes.readUInt32LE(0) === 0xfeedfacf) {
    const cpu = bytes.readUInt32LE(4);
    return { format: 'mach-o', arch: MACHO_CPU[cpu] ?? `cputype 0x${cpu.toString(16)}` };
  }
  if (bytes.length >= 64 && bytes.toString('latin1', 0, 4) === '\x7fELF') return readElf(bytes);
  if (bytes.length >= 64 && bytes.toString('latin1', 0, 2) === 'MZ') {
    const pe = bytes.readUInt32LE(0x3c);
    if (bytes.toString('latin1', pe, pe + 4) === 'PE\0\0') {
      const machine = bytes.readUInt16LE(pe + 4);
      return { format: 'pe', arch: PE_MACHINE[machine] ?? `machine 0x${machine.toString(16)}` };
    }
  }
  return { format: 'unknown', arch: 'unknown' };
}

/** What a platform's libc demands of an ELF: glibc's loader at or under the floor, or musl. */
function libcCheck(libc, header) {
  const glibc = maxGlibc(header.versionNeeds);
  if (libc === 'gnu') {
    const ok = /\/ld-linux[-\w.]*\.so/.test(header.interpreter ?? '') && glibc !== null && compareVersions(glibc, GLIBC_FLOOR) <= 0;
    return { ok, detail: `interpreter ${header.interpreter}; newest GLIBC_${glibc} (floor ${GLIBC_FLOOR})` };
  }
  const ok = glibc === null && (header.interpreter === null || /\/ld-musl-/.test(header.interpreter));
  return { ok, detail: header.interpreter === null ? 'statically linked' : `interpreter ${header.interpreter}` };
}

/** One platform package dir: `{ platform, version, binary }` from its manifest. */
function readPackage(dir) {
  const manifest = JSON.parse(readFileSync(path.join(dir, 'package.json'), 'utf8'));
  const platform = Object.keys(PLATFORMS).find(suffix => platformPackageName(suffix) === manifest.name);
  if (!platform) throw new Error(`${dir}: ${manifest.name} is not a package of bin/platform.cjs`);
  return { platform, version: manifest.version, binary: path.join(dir, executableName('octocode', PLATFORMS[platform].os)) };
}

/** Static checks of one platform package dir, as `[{ name, ok, detail }]`. */
function identityChecks(dir, fingerprint = expectedFingerprint()) {
  const { platform, version, binary } = readPackage(dir);
  const target = PLATFORMS[platform];
  const stat = statSync(binary, { throwIfNoEntry: false });
  if (!stat?.isFile()) return [{ name: 'binary', ok: false, detail: `${binary} is missing` }];
  const bytes = readFileSync(binary);
  const header = readHeader(bytes);
  const checks = [
    { name: 'format', ok: header.format === FORMAT_BY_OS[target.os], detail: `${header.format}, want ${FORMAT_BY_OS[target.os]}` },
    { name: 'arch', ok: header.arch === target.arch, detail: `${header.arch}, want ${target.arch}` },
    { name: 'embeddedVersion', ok: bytes.includes(version), detail: version },
    { name: 'embeddedFingerprint', ok: bytes.includes(fingerprint), detail: fingerprint },
  ];
  if (header.format === 'elf') checks.push({ name: 'libc', ...libcCheck(target.libc, header) });
  // A zip artifact or a Windows checkout drops the mode bit; npm packs what the file has.
  if (target.os !== 'win32' && process.platform !== 'win32') {
    checks.push({ name: 'executable', ok: (stat.mode & 0o111) !== 0, detail: `mode ${(stat.mode & 0o777).toString(8)}` });
  }
  return checks;
}

module.exports = { compareVersions, expectedFingerprint, identityChecks, maxGlibc, readHeader, readPackage };
