#!/usr/bin/env node
// `contracts:regen` copies core's dist. Build it first: timestamps do not prove a
// successful build or account for deleted sources, missing outputs and build inputs.
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { ROOT } from './lib/workspaces.mjs';

const CORE = path.resolve(ROOT, '..', 'octocode-mcp-host', 'packages', 'octocode-core');

if (existsSync(CORE)) {
  console.log('Building sibling octocode-core before regenerating its contract.');
  const { status } = spawnSync('yarn', ['build'], { cwd: CORE, stdio: 'inherit' });
  process.exit(status ?? 1);
}
