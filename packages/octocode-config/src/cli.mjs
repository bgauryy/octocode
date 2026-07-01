#!/usr/bin/env node
// @octocodeai/config CLI — inspect and verify Octocode env state.
//
// Usage:
//   npx @octocodeai/config                  # print home dir, file paths, key count
//   npx @octocodeai/config --keys           # list loaded key names (no values ever printed)
//   npx @octocodeai/config --check KEY      # exit 0 if KEY is set, exit 1 if not
//   npx @octocodeai/config --help

import path from 'node:path';
import { getOctocodeHome, propagateOctocodeEnv, loadOctocodeEnv, loadOctocoderc } from './index.mjs';

const args = process.argv.slice(2);

if (args.includes('--help') || args.includes('-h')) {
  console.log(`@octocodeai/config — Octocode env inspector

Usage:
  npx @octocodeai/config                  Print home dir, .env paths, key count
  npx @octocodeai/config --keys           List loaded key names (never values)
  npx @octocodeai/config --check KEY      Exit 0 if KEY is set, 1 if not
  npx @octocodeai/config --help           Show this help

Env files loaded (in precedence order):
  <home>/.env                             Global keys (OCTOCODE_HOME or platform default)
  <cwd>/.octocode/.env                    Project keys (trusted mode only)
`);
  process.exit(0);
}

const home = getOctocodeHome();

if (args.includes('--check')) {
  const key = args[args.indexOf('--check') + 1];
  if (!key) {
    process.stderr.write('--check requires a KEY name\n');
    process.exit(1);
  }
  propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });
  if (process.env[key]) {
    console.log(`${key}: set`);
    process.exit(0);
  } else {
    console.log(`${key}: not set`);
    process.exit(1);
  }
}

if (args.includes('--keys')) {
  const { map } = loadOctocodeEnv({ home, cwd: process.cwd(), trusted: true });
  const keys = Object.keys(map);
  if (keys.length === 0) {
    console.log('No keys loaded.');
  } else {
    keys.forEach(k => console.log(k));
  }
  process.exit(0);
}

// Default: status
const { map, sources } = loadOctocodeEnv({ home, cwd: process.cwd(), trusted: true });
const rc = loadOctocoderc(home);
const globalKeys = Object.values(sources).filter(s => s === 'global').length;
const projectKeys = Object.values(sources).filter(s => s === 'project').length;

console.log(`Octocode home:   ${home}`);
console.log(`Global .env:     ${path.join(home, '.env')}`);
console.log(`Project .env:    ${path.join(process.cwd(), '.octocode', '.env')}`);
console.log(`Keys loaded:     ${Object.keys(map).length} (${globalKeys} global, ${projectKeys} project)`);
if (Object.keys(rc).length > 0) {
  console.log(`Config (.octocoderc): ${Object.keys(rc).join(', ')}`);
}
