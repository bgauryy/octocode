/**
 * Verifies that every platform directory under npm/ contains the CLI, regex
 * worker, and runtime addon. The host-platform addon is loaded in a subprocess
 * so a malformed or invalidly signed artifact cannot pass on size.
 *
 * Run: yarn workspace @octocodeai/octocode-native platforms:check
 */
'use strict';

const { statSync } = require('fs');
const { join } = require('path');
const { BINARIES, PLATFORMS, executableName, getPlatformSuffix } = require('../bin/platform.cjs');
const { verifyAddonLoads } = require('./native-addon-utils.cjs');

const root = join(__dirname, '..');
const hostPlatform = getPlatformSuffix();

let allOk = true;

for (const [dir, { os }] of Object.entries(PLATFORMS)) {
  const binaries = BINARIES.map(name => executableName(name, os));
  const addon = `octocode-native.${dir}.node`;
  for (const name of [...binaries, addon]) {
    const p = join(root, 'npm', dir, name);
    try {
      const { size, mode } = statSync(p);
      if (size === 0) {
        console.error(`✗ npm/${dir}/${name} is empty (0 bytes)`);
        allOk = false;
      } else if (os !== 'win32' && binaries.includes(name) && (mode & 0o111) === 0) {
        console.error(`✗ npm/${dir}/${name} is not executable`);
        allOk = false;
      } else {
        console.log(`✓ npm/${dir}/${name} (${size} bytes)`);
      }
    } catch {
      console.error(`✗ npm/${dir}/${name} is MISSING`);
      allOk = false;
    }
  }

  if (dir === hostPlatform) {
    try {
      verifyAddonLoads(join(root, 'npm', dir, addon));
      console.log(`✓ npm/${dir}/${addon} loads`);
    } catch (error) {
      console.error(`✗ npm/${dir}/${addon} failed to load: ${error.message}`);
      allOk = false;
    }
  }
}

if (!allOk) {
  console.error('\nSome platform binaries are missing or empty.');
  console.error('Run: yarn workspace @octocodeai/octocode-native build:all');
  process.exit(1);
}

console.log('\nAll platform binaries present. ✓');
