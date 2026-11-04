#!/usr/bin/env node
// Builds the sibling octocode-core when its source is newer than its build: `contracts:regen`
// copies core's dist, so a stale dist would regenerate the contract from old text, silently.
import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { ROOT } from './lib/workspaces.mjs';

const CORE = path.resolve(ROOT, '..', 'octocode-mcp-host', 'packages', 'octocode-core');

function newest(dir, skip = () => false) {
  let latest = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (skip(entry.name)) continue;
    latest = Math.max(latest, entry.isDirectory() ? newest(full, skip) : statSync(full).mtimeMs);
  }
  return latest;
}

if (existsSync(path.join(CORE, 'src'))) {
  const dist = path.join(CORE, 'dist');
  const built = existsSync(dist) ? newest(dist) : 0;
  if (newest(path.join(CORE, 'src')) > built) {
    console.log('octocode-core source is newer than its build: building it first.');
    const { status } = spawnSync('yarn', ['build'], { cwd: CORE, stdio: 'inherit' });
    process.exit(status ?? 1);
  }
}
