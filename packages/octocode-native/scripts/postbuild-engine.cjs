'use strict';

const { copyFileSync, existsSync, rmSync } = require('fs');
const { join } = require('path');

const root = join(__dirname, '..');
const generatedTypes = join(root, '.engine-generated.d.ts');
if (existsSync(generatedTypes)) {
  copyFileSync(generatedTypes, join(root, '.napi-abi-snapshot.d.ts'));
  console.log('snapshotted engine N-API declarations');
}

for (const generated of ['.engine-generated.cjs', '.engine-generated.d.ts']) {
  rmSync(join(root, generated), { force: true });
}

console.log('kept canonical js/engine.{cjs,js,d.ts} entrypoints');
