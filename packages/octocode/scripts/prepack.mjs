#!/usr/bin/env node
import { statSync } from 'node:fs';
import manifest from '../package.json' with { type: 'json' };
import { fileURLToPath } from 'node:url';
import { stagePackage } from '../../../skills-dev/octocode-dev/scripts/package-build.mjs';

for (const entry of [manifest.bin, manifest.exports['./native']]) {
  if (!statSync(new URL(entry, new URL('../', import.meta.url)), { throwIfNoEntry: false })?.isFile()) {
    throw new Error(`Build octocode before packing: missing ${entry}`);
  }
}

// Skills and docs come from the repository on every pack, so a pack after any build ships both.
const packageRoot = fileURLToPath(new URL('../', import.meta.url));
const { skills, docs } = stagePackage(packageRoot, 'out');
console.log(`✓ skills staged → ${skills}\n✓ docs bundled → ${docs}`);
