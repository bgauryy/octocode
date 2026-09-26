'use strict';

const { copyFileSync } = require('fs');
const { join } = require('path');
const { getPlatformSuffix } = require('../bin/platform.cjs');
const { adHocSignDarwinAddon, verifyAddonLoads } = require('./native-addon-utils.cjs');

const profile = process.argv[2] ?? 'debug';
const suffix = getPlatformSuffix();
if (!suffix) throw new Error(`Unsupported platform ${process.platform}-${process.arch}`);

const root = join(__dirname, '..');
const library =
  process.platform === 'win32'
    ? 'octocode_runtime_napi.dll'
    : process.platform === 'darwin'
      ? 'liboctocode_runtime_napi.dylib'
      : 'liboctocode_runtime_napi.so';
const destination = `octocode-native.${suffix}.node`;
const destinationPath = join(root, destination);
copyFileSync(join(root, 'target', profile, library), destinationPath);
adHocSignDarwinAddon(destinationPath, suffix);
verifyAddonLoads(destinationPath);
console.log(`copied and verified ${library} -> ${destination}`);
