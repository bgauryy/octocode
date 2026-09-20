'use strict';

const { copyFileSync, chmodSync, mkdirSync, renameSync, rmSync } = require('fs');
const { randomUUID } = require('crypto');
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
  const staged = `${target}.${randomUUID()}.tmp`;
  try {
    // Replace the inode: overwriting a running Mach-O can retain a stale
    // kernel code-signature cache and make later launches receive SIGKILL.
    copyFileSync(join(root, 'target', profile, filename), staged);
    if (process.platform !== 'win32') chmodSync(staged, 0o755);
    renameSync(staged, target);
  } finally {
    rmSync(staged, { force: true });
  }
  console.log(`copied ${filename} -> npm/${suffix}/${filename}`);
}
