#!/usr/bin/env node
'use strict';

/**
 * Everything derived from `bin/platform.cjs` (`PLATFORMS`): each
 * `npm/<suffix>/package.json` and `README.md`, and the launchers'
 * (`octocode`, `octocode-mcp`) `optionalDependencies`, pinned exactly at the
 * native package version.
 *
 *   platform-packages.cjs          write them
 *   platform-packages.cjs --check  exit 1 and list what differs (tests run this)
 */

const { existsSync, readdirSync, readFileSync, writeFileSync, mkdirSync } = require('fs');
const nodePath = require('node:path');
const { PLATFORMS, executableName, platformPackageName } = require('../bin/platform.cjs');

const ROOT = nodePath.join(__dirname, '..');
const REPO = nodePath.join(ROOT, '..', '..');
const LAUNCHERS = ['packages/octocode/package.json', 'packages/octocode-mcp/package.json'];

const json = value => `${JSON.stringify(value, null, 2)}\n`;
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));

function platformManifest(suffix, native) {
  const { os, arch, libc } = PLATFORMS[suffix];
  return {
    name: platformPackageName(suffix),
    description: `Octocode native binary — ${suffix}`,
    version: native.version,
    author: native.author,
    license: native.license,
    repository: native.repository,
    os: [os],
    cpu: [arch],
    ...(libc ? { libc: [libc === 'gnu' ? 'glibc' : libc] } : {}),
    files: [executableName('octocode', os), 'README.md'],
    // Yarn PnP cannot execute a binary from a zip archive.
    preferUnplugged: true,
    publishConfig: { access: 'public' },
  };
}

function platformReadme(suffix) {
  return [
    `# ${platformPackageName(suffix)}`,
    '',
    `Pre-compiled \`octocode\` binary (CLI and MCP server) for **${PLATFORMS[suffix].label}**.`,
    '',
    'Installed automatically as an optional dependency of `octocode` and `octocode-mcp`.',
    'Do not install this package directly.',
    '',
  ].join('\n');
}

function launcherPins(version) {
  return Object.fromEntries(Object.keys(PLATFORMS).map(suffix => [platformPackageName(suffix), version]));
}

/** Every derived file as `{ path, content }`, plus npm/ directories PLATFORMS no longer names. */
function derivedFiles() {
  const native = readJson(nodePath.join(ROOT, 'package.json'));
  const files = [];
  for (const suffix of Object.keys(PLATFORMS)) {
    files.push({ path: nodePath.join(ROOT, 'npm', suffix, 'package.json'), content: json(platformManifest(suffix, native)) });
    files.push({ path: nodePath.join(ROOT, 'npm', suffix, 'README.md'), content: platformReadme(suffix) });
  }
  for (const launcher of LAUNCHERS) {
    const path = nodePath.join(REPO, launcher);
    const manifest = readJson(path);
    manifest.optionalDependencies = launcherPins(native.version);
    files.push({ path, content: json(manifest) });
  }
  const stale = readdirSync(nodePath.join(ROOT, 'npm')).filter(name => !Object.hasOwn(PLATFORMS, name));
  return { files, stale };
}

/** Repository-relative paths whose content differs from what PLATFORMS derives. */
function drift() {
  const { files, stale } = derivedFiles();
  const differing = files
    .filter(({ path, content }) => !existsSync(path) || readFileSync(path, 'utf8') !== content)
    .map(({ path }) => nodePath.relative(REPO, path));
  return [...differing, ...stale.map(name => `${nodePath.relative(REPO, nodePath.join(ROOT, 'npm', name))}/ (not in PLATFORMS)`)];
}

function write() {
  const { files, stale } = derivedFiles();
  for (const { path, content } of files) {
    mkdirSync(nodePath.join(path, '..'), { recursive: true });
    if (!existsSync(path) || readFileSync(path, 'utf8') !== content) {
      writeFileSync(path, content);
      console.log(`wrote ${nodePath.relative(REPO, path)}`);
    }
  }
  for (const name of stale) console.error(`npm/${name} is not in PLATFORMS: remove it`);
  return stale.length === 0;
}

if (require.main === module) {
  if (process.argv.includes('--check')) {
    const differing = drift();
    if (differing.length) {
      console.error(`Out of date with bin/platform.cjs (run scripts/platform-packages.cjs):\n  ${differing.join('\n  ')}`);
      process.exit(1);
    }
    console.log('platform packages match PLATFORMS');
  } else if (!write()) {
    process.exit(1);
  }
}

module.exports = { LAUNCHERS, drift, platformManifest };
