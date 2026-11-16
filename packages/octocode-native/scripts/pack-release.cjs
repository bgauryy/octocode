#!/usr/bin/env node
/**
 * Packs the release line's npm tarballs, the bytes an owner publishes: one per
 * `bin/platform.cjs` platform package (`npm/<platform>`), then the
 * `octocode-mcp` and `octocode` launchers (build them first).
 *
 *   pack-release.cjs --out DIR
 *
 * Refuses to pack when the manifests or launcher pins drift from PLATFORMS
 * (`platform-packages.cjs --check`), a launcher is not at the native version
 * or not built, or a platform binary fails its identity checks
 * (binary-identity.cjs). Each platform tarball must hold exactly its manifest's
 * files, with the binary executable outside Windows. Writes DIR/SHA256SUMS in
 * `sha256sum -c` format.
 */
'use strict';

const { execFileSync } = require('child_process');
const { createHash } = require('crypto');
const { closeSync, existsSync, mkdirSync, openSync, readdirSync, readFileSync, readSync, writeFileSync } = require('fs');
const path = require('path');
const { parseArgs } = require('util');
const { PLATFORMS, executableName } = require('../bin/platform.cjs');
const { identityChecks } = require('./binary-identity.cjs');
const { LAUNCHERS, drift } = require('./platform-packages.cjs');

const ROOT = path.join(__dirname, '..');
const REPO = path.join(ROOT, '..', '..');

const readJson = file => JSON.parse(readFileSync(file, 'utf8'));
const byName = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

/** `npm pack` one package dir into `out`; returns npm's record of the tarball. */
function npmPack(dir, out) {
  const stdout = execFileSync('npm', ['pack', '--json', '--pack-destination', out], {
    cwd: dir,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    shell: process.platform === 'win32',
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  // Lifecycle scripts (the CLI's prepack) print to stdout ahead of npm's JSON array.
  return JSON.parse(stdout.slice(stdout.search(/^\[$/m))).at(-1);
}

function packPlatform(platform, out) {
  const dir = path.join(ROOT, 'npm', platform);
  const failed = identityChecks(dir).filter(check => !check.ok);
  if (failed.length) throw new Error(`${platform}: ${failed.map(check => `${check.name} (${check.detail})`).join('; ')}`);
  const manifest = readJson(path.join(dir, 'package.json'));
  const packed = npmPack(dir, out);
  const files = packed.files.map(file => file.path).toSorted(byName).join(', ');
  const expected = [...manifest.files, 'package.json'].toSorted(byName).join(', ');
  if (files !== expected) throw new Error(`${platform}: tarball holds [${files}], want [${expected}]`);
  const binary = packed.files.find(file => file.path === executableName('octocode', PLATFORMS[platform].os));
  if (PLATFORMS[platform].os !== 'win32' && !(binary.mode & 0o111)) {
    throw new Error(`${platform}: ${binary.path} is packed with mode ${binary.mode.toString(8)}; chmod 755 it first`);
  }
  return packed.filename;
}

const NODE_SHEBANG = '#!/usr/bin/env node';

/** Whether the file starts with the Node shebang: only its first bytes are read, as a native bin is ~100 MB of non-text. */
function startsWithNodeShebang(file) {
  const fd = openSync(file, 'r');
  try {
    const head = Buffer.alloc(NODE_SHEBANG.length);
    const read = readSync(fd, head, 0, head.length, 0);
    return head.toString('latin1', 0, read) === NODE_SHEBANG;
  } finally {
    closeSync(fd);
  }
}

/**
 * What stops a launcher from packing: it ships Node bins, but an install
 * script may have replaced one in this checkout with this machine's native
 * binary, and one that crashed between link and rename left its staging file
 * (`.octocode.<pid>.tmp`) beside the bin, where `npm pack` would ship it.
 */
function launcherBinProblems(dir, bins) {
  const problems = bins.filter(bin => !startsWithNodeShebang(path.join(dir, bin))).map(bin => `${bin} is not the Node bin; rebuild it`);
  for (const bin of new Set(bins.map(entry => path.dirname(entry)))) {
    for (const name of readdirSync(path.join(dir, bin))) {
      if (/^\.octocode\.\d+\.tmp$/.test(name)) problems.push(`${path.posix.join(bin, name)} is a leftover of an interrupted install; remove it`);
    }
  }
  return problems;
}

function packLauncher(manifestPath, version, out) {
  const dir = path.dirname(path.join(REPO, manifestPath));
  const manifest = readJson(path.join(dir, 'package.json'));
  if (manifest.version !== version) throw new Error(`${manifest.name} is ${manifest.version}, the native packages are ${version}`);
  const bins = typeof manifest.bin === 'string' ? [manifest.bin] : Object.values(manifest.bin);
  const unbuilt = bins.filter(bin => !existsSync(path.join(dir, bin)));
  if (unbuilt.length) throw new Error(`${manifest.name}: build it first (${unbuilt.join(', ')} missing)`);
  const problems = launcherBinProblems(dir, bins);
  if (problems.length) throw new Error(`${manifest.name}: ${problems.join('; ')}`);
  return npmPack(dir, out).filename;
}

function main() {
  const { values } = parseArgs({ options: { out: { type: 'string' } } });
  if (!values.out) {
    console.error('Usage: pack-release.cjs --out DIR');
    process.exit(2);
  }
  const out = path.resolve(values.out);
  mkdirSync(out, { recursive: true });
  const differing = drift();
  if (differing.length) throw new Error(`Out of date with bin/platform.cjs (run scripts/platform-packages.cjs):\n  ${differing.join('\n  ')}`);
  const { version } = readJson(path.join(ROOT, 'package.json'));
  const tarballs = [
    ...Object.keys(PLATFORMS).map(platform => packPlatform(platform, out)),
    ...LAUNCHERS.map(manifestPath => packLauncher(manifestPath, version, out)),
  ];
  const sums = tarballs
    .toSorted(byName)
    .map(file => `${createHash('sha256').update(readFileSync(path.join(out, file))).digest('hex')}  ${file}`);
  writeFileSync(path.join(out, 'SHA256SUMS'), `${sums.join('\n')}\n`);
  console.log(`packed ${tarballs.length} tarballs at ${version} into ${out}:\n${sums.join('\n')}`);
}

module.exports = { launcherBinProblems };

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error(`pack-release: ${error.message}`);
    process.exit(1);
  }
}
