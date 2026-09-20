'use strict';

const { copyFileSync, chmodSync, mkdirSync } = require('fs');
const { join } = require('path');
const { getPlatformSuffix } = require('../bin/platform.cjs');

const profile = process.argv[2] ?? 'debug';
const suffix = getPlatformSuffix();
if (!suffix) throw new Error(`Unsupported platform ${process.platform}-${process.arch}`);

const root = join(__dirname, '..');
const destination = join(root, 'npm', suffix);
const extension = process.platform === 'win32' ? '.exe' : '';
mkdirSync(destination, { recursive: true });
for (const name of ['octocode', 'octocode-regex-worker']) {
  const filename = `${name}${extension}`;
  const target = join(destination, filename);
  copyFileSync(join(root, 'target', profile, filename), target);
  if (process.platform !== 'win32') chmodSync(target, 0o755);
  console.log(`copied ${filename} -> npm/${suffix}/${filename}`);
}
