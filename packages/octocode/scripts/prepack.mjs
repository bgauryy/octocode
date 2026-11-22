#!/usr/bin/env node
import { statSync } from 'node:fs';
import manifest from '../package.json' with { type: 'json' };
import { fileURLToPath } from 'node:url';
import { stageSkills } from './stage-skills.mjs';

for (const entry of [manifest.bin, manifest.exports['./native']]) {
  if (!statSync(new URL(entry, new URL('../', import.meta.url)), { throwIfNoEntry: false })?.isFile()) {
    throw new Error(`Build octocode before packing: missing ${entry}`);
  }
}

stageSkills(
  fileURLToPath(new URL('../../../skills/', import.meta.url)),
  fileURLToPath(new URL('../skills/', import.meta.url))
);
console.log('✓ skills staged from monorepo root');
