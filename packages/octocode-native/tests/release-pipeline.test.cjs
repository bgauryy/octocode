'use strict';

const assert = require('node:assert/strict');
const { test } = require('node:test');
const { mkdtempSync, readFileSync, rmSync, writeFileSync } = require('node:fs');
const path = require('node:path');
const { tmpdir } = require('node:os');
const { PLATFORMS, platformPackageName } = require('../bin/platform.cjs');
const { identityChecks, maxGlibc, readHeader } = require('../scripts/binary-identity.cjs');
const { FIXTURE, checkCatalog, checkFetch, checkSearch } = require('../scripts/smoke-binary.cjs');
const { RUNNERS, releaseMatrix } = require('../scripts/release-matrix.cjs');

/** A minimal ELF64 executable: one PT_INTERP and one `.gnu.version_r` need on libc.so.6. */
function elf({ machine = 62, interpreter = '/lib64/ld-linux-x86-64.so.2', needs = [] } = {}) {
  const names = ['libc.so.6', ...needs];
  const strtab = Buffer.from(`\0${names.join('\0')}\0`, 'latin1');
  const nameOffset = name => strtab.indexOf(Buffer.from(`\0${name}\0`, 'latin1')) + 1;
  const interp = interpreter ? Buffer.from(`${interpreter}\0`, 'latin1') : Buffer.alloc(0);
  const verneed = Buffer.alloc(needs.length ? 16 * (1 + needs.length) : 0);
  if (needs.length) {
    verneed.writeUInt16LE(1, 0);
    verneed.writeUInt16LE(needs.length, 2);
    verneed.writeUInt32LE(nameOffset('libc.so.6'), 4);
    verneed.writeUInt32LE(16, 8);
    needs.forEach((need, i) => {
      verneed.writeUInt32LE(nameOffset(need), 16 + i * 16 + 8);
      verneed.writeUInt32LE(i + 1 < needs.length ? 16 : 0, 16 + i * 16 + 12);
    });
  }
  const interpAt = 64 + 56;
  const strtabAt = interpAt + interp.length;
  const verneedAt = strtabAt + strtab.length;
  const sectionsAt = verneedAt + verneed.length;
  const header = Buffer.alloc(64 + 56);
  header.write('\x7fELF', 0, 'latin1');
  header[4] = 2;
  header[5] = 1;
  header.writeUInt16LE(machine, 18);
  header.writeBigUInt64LE(64n, 32);
  header.writeBigUInt64LE(BigInt(sectionsAt), 40);
  header.writeUInt16LE(56, 54);
  header.writeUInt16LE(interpreter ? 1 : 0, 56);
  header.writeUInt16LE(64, 58);
  header.writeUInt16LE(3, 60);
  header.writeUInt32LE(3, 64);
  header.writeBigUInt64LE(BigInt(interpAt), 64 + 8);
  header.writeBigUInt64LE(BigInt(interp.length), 64 + 32);
  const sections = Buffer.alloc(3 * 64);
  sections.writeUInt32LE(3, 64 + 4);
  sections.writeBigUInt64LE(BigInt(strtabAt), 64 + 24);
  sections.writeUInt32LE(needs.length ? 0x6ffffffe : 0, 128 + 4);
  sections.writeBigUInt64LE(BigInt(verneedAt), 128 + 24);
  sections.writeUInt32LE(1, 128 + 40);
  sections.writeUInt32LE(needs.length ? 1 : 0, 128 + 44);
  return Buffer.concat([header, interp, strtab, verneed, sections]);
}

test('readHeader reads Mach-O, PE and ELF CPUs, the ELF interpreter and version needs', () => {
  const macho = Buffer.alloc(32);
  macho.writeUInt32LE(0xfeedfacf, 0);
  macho.writeUInt32LE(0x0100000c, 4);
  assert.deepEqual(readHeader(macho), { format: 'mach-o', arch: 'arm64' });
  const pe = Buffer.alloc(256);
  pe.write('MZ', 0, 'latin1');
  pe.writeUInt32LE(128, 0x3c);
  pe.write('PE\0\0', 128, 'latin1');
  pe.writeUInt16LE(0x8664, 132);
  assert.deepEqual(readHeader(pe), { format: 'pe', arch: 'x64' });
  assert.deepEqual(readHeader(elf({ machine: 183, interpreter: '/lib/ld-linux-aarch64.so.1', needs: ['GLIBC_2.17', 'GLIBC_2.28'] })), {
    format: 'elf', arch: 'arm64', interpreter: '/lib/ld-linux-aarch64.so.1', versionNeeds: ['GLIBC_2.17', 'GLIBC_2.28'],
  });
  assert.deepEqual(readHeader(elf({ interpreter: null })), { format: 'elf', arch: 'x64', interpreter: null, versionNeeds: [] });
  assert.equal(readHeader(Buffer.from('#!/bin/sh\n')).format, 'unknown');
});

test('maxGlibc compares versions numerically and ignores other names', () => {
  assert.equal(maxGlibc(['GLIBC_2.9', 'GLIBC_2.28', 'GLIBC_2.3.4', 'GLIBC_PRIVATE', 'GCC_3.0']), '2.28');
  assert.equal(maxGlibc(['GCC_3.0']), null);
});

test('identityChecks hold a gnu binary to the glibc floor and the embedded identity', t => {
  const dir = mkdtempSync(path.join(tmpdir(), 'octocode-identity-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(path.join(dir, 'package.json'), JSON.stringify({ name: platformPackageName('linux-x64-gnu'), version: '9.8.7' }));
  const stamp = Buffer.from('octocode 9.8.7 fingerprint-abc');
  const failing = () => identityChecks(dir, 'fingerprint-abc').filter(check => !check.ok).map(check => check.name);

  writeFileSync(path.join(dir, 'octocode'), Buffer.concat([elf({ needs: ['GLIBC_2.17', 'GLIBC_2.28'] }), stamp]), { mode: 0o755 });
  assert.deepEqual(failing(), []);
  writeFileSync(path.join(dir, 'octocode'), Buffer.concat([elf({ needs: ['GLIBC_2.28', 'GLIBC_2.34'] }), stamp]));
  assert.deepEqual(failing(), ['libc']);
  writeFileSync(path.join(dir, 'octocode'), elf({ machine: 183, needs: ['GLIBC_2.17'] }));
  assert.deepEqual(failing(), ['arch', 'embeddedVersion', 'embeddedFingerprint']);
  rmSync(path.join(dir, 'octocode'));
  assert.deepEqual(failing(), ['binary']);
});

test('identityChecks accept a static musl binary and reject a glibc one', t => {
  const dir = mkdtempSync(path.join(tmpdir(), 'octocode-identity-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(path.join(dir, 'package.json'), JSON.stringify({ name: platformPackageName('linux-x64-musl'), version: '1.0.0' }));
  const stamp = Buffer.from('1.0.0 fp');
  writeFileSync(path.join(dir, 'octocode'), Buffer.concat([elf({ interpreter: null }), stamp]), { mode: 0o755 });
  assert.ok(identityChecks(dir, 'fp').every(check => check.ok));
  writeFileSync(path.join(dir, 'octocode'), Buffer.concat([elf({ needs: ['GLIBC_2.17'] }), stamp]));
  assert.equal(identityChecks(dir, 'fp').find(check => check.name === 'libc').ok, false);
});

test('smoke result checks require the fixture line through each surface', () => {
  const search = file => ({ results: [{ index: 0, data: { files: [{ path: file, matches: [{ line: FIXTURE.line, value: 'const octocodeSmokeNeedle = 1;' }] }] } }] });
  assert.equal(checkSearch(search(FIXTURE.path)), `${FIXTURE.path}:${FIXTURE.line}`);
  assert.equal(checkSearch(search(FIXTURE.path.replace('/', '\\'))), `${FIXTURE.path}:${FIXTURE.line}`);
  assert.throws(() => checkSearch(search('src/other.txt')), /not matched/);
  assert.throws(() => checkSearch({ results: [{ index: 0, error: { code: 'X' } }] }), /no result data/);

  const fetched = { results: [{ data: { content: '2\tconst octocodeSmokeNeedle = 1;\n', matchedLines: [FIXTURE.line] } }] };
  assert.equal(checkFetch(fetched), `matchedLines ${FIXTURE.line}`);
  assert.throws(() => checkFetch({ results: [{ data: { content: '', matchedLines: [] } }] }), /not fetched/);

  const tools = ['localSearch', 'localFetch', 'lspSearch'].map(name => ({ name }));
  assert.equal(checkCatalog({ kind: 'octocode.toolCatalog', toolCount: 3, tools }), '3 tools');
  assert.throws(() => checkCatalog({ kind: 'octocode.toolCatalog', toolCount: 4, tools }), /consistent/);
  assert.throws(() => checkCatalog({ kind: 'octocode.toolCatalog', toolCount: 1, tools: [{ name: 'lspSearch' }] }), /missing localSearch, localFetch/);
});

test('the release matrix builds every platform once and verifies it on its own OS and CPU', () => {
  const { build, verify, platforms } = releaseMatrix();
  const runnerTarget = Object.fromEntries(
    Object.entries(RUNNERS).flatMap(([os, byArch]) => Object.entries(byArch).map(([arch, runner]) => [runner, { os, arch }])),
  );
  assert.deepEqual(platforms, Object.keys(PLATFORMS));
  assert.deepEqual(build.map(entry => entry.platform), platforms);
  for (const { platform, runner } of build) {
    const target = PLATFORMS[platform];
    const host = runnerTarget[runner];
    assert.ok(host, `${platform}: unknown build runner ${runner}`);
    // Linux cross-links (zigbuild, glibc floor) only from another OS or CPU.
    if (target.os === 'linux') assert.notEqual(host.arch, target.arch, platform);
    if (target.os === 'darwin') assert.equal(host.os, 'darwin', platform);
  }
  assert.equal(new Set(verify.map(entry => entry.id)).size, verify.length);
  for (const platform of platforms) {
    const target = PLATFORMS[platform];
    const entries = verify.filter(entry => entry.platform === platform);
    for (const entry of entries) {
      const host = runnerTarget[entry.runner];
      assert.equal(host.arch, target.arch, entry.name);
      assert.equal(entry.arch, target.arch, entry.name);
      assert.equal(host.os, entry.container ? 'linux' : target.os, entry.name);
    }
    if (target.libc === 'musl') assert.deepEqual(entries.map(entry => Boolean(entry.container)), [true], platform);
    else if (target.libc === 'gnu') assert.deepEqual(entries.map(entry => entry.glibc), [null, require('../scripts/build-native.cjs').GLIBC_FLOOR], platform);
    else assert.equal(entries.length, 1, platform);
  }
});

test('the release workflow derives its matrices and never publishes', () => {
  const workflow = readFileSync(path.join(__dirname, '..', '..', '..', '.github', 'workflows', 'release.yml'), 'utf8');
  assert.match(workflow, /^ {2}workflow_dispatch:$/m);
  assert.match(workflow, /scripts\/release-matrix\.cjs/);
  assert.match(workflow, /scripts\/smoke-binary\.cjs/);
  assert.match(workflow, /scripts\/pack-release\.cjs/);
  assert.doesNotMatch(workflow, /npm publish|secrets\.|git push|NODE_AUTH_TOKEN|id-token: write/);
});
