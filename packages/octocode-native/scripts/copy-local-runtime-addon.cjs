'use strict';

const { copyFileSync } = require('fs');
const { join } = require('path');
const { getPlatformSuffix } = require('../bin/platform.cjs');

const profile = process.argv[2] ?? 'debug';
const suffix = getPlatformSuffix();
if (!suffix) throw new Error(`Unsupported platform ${process.platform}-${process.arch}`);

const root = join(__dirname, '..');
const library =
  process.platform === 'win32'
    ? 'octocode_native.dll'
    : process.platform === 'darwin'
      ? 'liboctocode_native.dylib'
      : 'liboctocode_native.so';
const destination = `octocode-native.${suffix}.node`;
copyFileSync(join(root, 'target', profile, library), join(root, destination));
console.log(`copied ${library} -> ${destination}`);
