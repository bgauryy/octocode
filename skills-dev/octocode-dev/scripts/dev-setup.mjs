#!/usr/bin/env node
// Link the sibling core for local contract development; reset returns to the npm pin.
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isLocalResolution, managedResolutionPackages } from './dev-resolution-contract.mjs';
import { ROOT } from './lib/workspaces.mjs';

const argv = process.argv.slice(2);
const flags = {
  dryRun: argv.includes('--dry-run') || argv.includes('-n'),
  install: argv.includes('--install') || argv.includes('-i'),
};
const knownArgs = new Set([
  '--dry-run',
  '-n',
  '--install',
  '-i',
  '--reset',
  '--unlink',
]);
for (const arg of argv) {
  if (!knownArgs.has(arg)) {
    console.error(`✖ unknown argument: ${arg}`);
    process.exit(1);
  }
}

const manifestPath = join(ROOT, 'package.json');
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
const previous = manifest.resolutions ?? {};
const resolutions = { ...previous };
for (const name of managedResolutionPackages()) {
  if (isLocalResolution(resolutions[name])) delete resolutions[name];
}
const sibling = '../octocode-mcp-host/packages/octocode-core';
if (!argv.includes('--reset') && !argv.includes('--unlink') && existsSync(join(ROOT, sibling, 'package.json'))) {
  resolutions['@octocodeai/octocode-core'] = `file:${sibling}`;
}
if (JSON.stringify(resolutions) !== JSON.stringify(previous)) {
  if (Object.keys(resolutions).length) manifest.resolutions = resolutions;
  else delete manifest.resolutions;
  if (!flags.dryRun) writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
}
console.log(`Core resolution${flags.dryRun ? ' (dry run)' : ''}: ${resolutions['@octocodeai/octocode-core'] ?? 'package dependency'}`);
if (flags.install && !flags.dryRun) {
  const install = spawnSync('yarn', ['install'], { cwd: ROOT, stdio: 'inherit' });
  process.exit(install.status ?? 1);
}
