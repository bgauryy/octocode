'use strict';

const { copyFileSync, existsSync, rmSync } = require('fs');
const { join } = require('path');
const { getPlatformSuffix } = require('../bin/platform.cjs');
const { adHocSignDarwinAddon, verifyAddonLoads } = require('./native-addon-utils.cjs');

const root = join(__dirname, '..');
const generatedTypes = join(root, '.engine-generated.d.ts');
if (existsSync(generatedTypes)) {
  copyFileSync(generatedTypes, join(root, '.napi-abi-snapshot.d.ts'));
  console.log('snapshotted engine N-API declarations');
}

for (const generated of ['.engine-generated.cjs', '.engine-generated.d.ts']) {
  rmSync(join(root, generated), { force: true });
}

const suffix = process.argv[2] ?? getPlatformSuffix();
const addonPath = join(root, `octocode-engine.${suffix}.node`);
adHocSignDarwinAddon(addonPath, suffix);
if (suffix === getPlatformSuffix()) {
  verifyAddonLoads(addonPath);
}

console.log(`kept canonical js/engine.{cjs,js,d.ts} entrypoints and verified ${suffix}`);
