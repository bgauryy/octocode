/**
 * Verifies that every platform directory under npm/ contains both the
 * octocode and octocode-regex-worker binaries before publishing.
 *
 * Run: yarn workspace @octocodeai/octocode-native platforms:check
 */
'use strict';

const { statSync } = require('fs');
const { join } = require('path');

const root = join(__dirname, '..');

const PLATFORMS = [
  { dir: 'darwin-arm64', binaries: ['octocode', 'octocode-regex-worker'] },
  { dir: 'darwin-x64', binaries: ['octocode', 'octocode-regex-worker'] },
  { dir: 'linux-arm64-gnu', binaries: ['octocode', 'octocode-regex-worker'] },
  { dir: 'linux-x64-gnu', binaries: ['octocode', 'octocode-regex-worker'] },
  { dir: 'linux-x64-musl', binaries: ['octocode', 'octocode-regex-worker'] },
  {
    dir: 'win32-x64-msvc',
    binaries: ['octocode.exe', 'octocode-regex-worker.exe'],
  },
];

let allOk = true;

for (const { dir, binaries } of PLATFORMS) {
  const artifacts = [
    ...binaries,
    `octocode-native.${dir}.node`,
    `octocode-engine.${dir}.node`,
  ];
  for (const name of artifacts) {
    const p = join(root, 'npm', dir, name);
    try {
      const { size, mode } = statSync(p);
      if (size === 0) {
        console.error(`\u2717 npm/${dir}/${name} is empty (0 bytes)`);
        allOk = false;
      } else if (!dir.startsWith('win32') && binaries.includes(name) && (mode & 0o111) === 0) {
        console.error(`\u2717 npm/${dir}/${name} is not executable`);
        allOk = false;
      } else {
        console.log(`\u2713 npm/${dir}/${name} (${size} bytes)`);
      }
    } catch {
      console.error(`\u2717 npm/${dir}/${name} is MISSING`);
      allOk = false;
    }
  }
}

if (!allOk) {
  console.error('\nSome platform binaries are missing or empty.');
  console.error('Run: yarn workspace @octocodeai/octocode-native build:all');
  process.exit(1);
}

console.log('\nAll platform binaries present. \u2713');
