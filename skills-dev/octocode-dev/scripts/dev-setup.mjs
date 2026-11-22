#!/usr/bin/env node
// Migrate old checkouts to manifest-based dependency resolution.
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
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

const guard = join(ROOT, 'skills-dev/octocode-dev/scripts/prepublish.mjs');
const cleanup = spawnSync(process.execPath, [guard, flags.dryRun ? '--dry-run' : '--fix'], {
  cwd: ROOT,
  stdio: 'inherit',
});
if (cleanup.status !== 0) process.exit(cleanup.status ?? 1);
if (flags.install && !flags.dryRun) {
  const install = spawnSync('yarn', ['install'], { cwd: ROOT, stdio: 'inherit' });
  process.exit(install.status ?? 1);
}
