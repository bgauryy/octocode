#!/usr/bin/env node
/**
 * scripts/dev.js — force every @octocodeai/* workspace dependency to resolve
 * from local source via root `resolutions` (portal:).
 *
 * For each @octocodeai/* package referenced as a dependency anywhere in the
 * workspace AND present as a workspace member, this writes a
 * `portal:<local-path>` entry under root `resolutions`. That makes every
 * consumer (including transitive ones and portal'd sibling repos) use the local
 * copy instead of a published registry version.
 *
 * External @octocodeai/* deps that are NOT workspace members (e.g.
 * @octocodeai/octocode-core, which lives in a sibling repo) are IGNORED.
 *
 * Usage:
 *   node scripts/dev.js               # write portal: resolutions to package.json
 *   node scripts/dev.js --dry-run     # show the plan, write nothing
 *   node scripts/dev.js --install     # also run `yarn install` afterwards
 *   node scripts/dev.js --reset       # remove the portal:/link: resolutions this adds
 */

import { readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs';
import { join, dirname, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SCOPE = '@octocodeai/';
const DEP_FIELDS = [
  'dependencies',
  'devDependencies',
  'peerDependencies',
  'optionalDependencies',
];

// ---- args -----------------------------------------------------------------
const argv = process.argv.slice(2);
const flags = { dryRun: false, reset: false, install: false };
for (const a of argv) {
  if (a === '--dry-run' || a === '-n') flags.dryRun = true;
  else if (a === '--reset' || a === '--unlink') flags.reset = true;
  else if (a === '--install' || a === '-i') flags.install = true;
  else if (a === '--help' || a === '-h') {
    console.log('Usage: node scripts/dev.js [--dry-run] [--install] [--reset]');
    process.exit(0);
  } else {
    console.error(`✖ unknown argument: ${a}`);
    process.exit(1);
  }
}

// ---- helpers --------------------------------------------------------------
const readJson = file => JSON.parse(readFileSync(file, 'utf8'));
const sortObject = obj =>
  Object.fromEntries(Object.keys(obj).sort().map(k => [k, obj[k]]));

function relFromRoot(absDir) {
  const rel = relative(REPO_ROOT, absDir);
  return rel.startsWith('.') ? rel : `./${rel}`;
}

// Expand the root `workspaces` globs (packages/*, packages/.../npm/*, skills)
// to concrete package dirs, then map package name -> absolute dir.
function findWorkspacePackages(rootPkg) {
  const patterns = Array.isArray(rootPkg.workspaces)
    ? rootPkg.workspaces
    : (rootPkg.workspaces?.packages ?? []);
  const dirs = new Set();
  for (const pattern of patterns) {
    let bases = [REPO_ROOT];
    for (const seg of pattern.split('/')) {
      const next = [];
      for (const base of bases) {
        if (seg === '*') {
          if (!existsSync(base)) continue;
          for (const e of readdirSync(base, { withFileTypes: true })) {
            if (e.isDirectory()) next.push(join(base, e.name));
          }
        } else {
          next.push(join(base, seg));
        }
      }
      bases = next;
    }
    for (const dir of bases) {
      if (existsSync(join(dir, 'package.json'))) dirs.add(dir);
    }
  }
  const map = new Map();
  for (const dir of dirs) {
    try {
      const name = readJson(join(dir, 'package.json')).name;
      if (name) map.set(name, dir);
    } catch {
      /* skip unreadable package.json */
    }
  }
  return map;
}

// Every @octocodeai/* name referenced as a dependency across the workspace.
function collectOctoDeps(memberDirs) {
  const deps = new Set();
  for (const dir of memberDirs) {
    const pkg = readJson(join(dir, 'package.json'));
    for (const field of DEP_FIELDS) {
      for (const name of Object.keys(pkg[field] ?? {})) {
        if (name.startsWith(SCOPE)) deps.add(name);
      }
    }
  }
  return [...deps].sort();
}

// Strip resolutions this script owns (any @octocodeai/* portal:/link: entry),
// so runs are idempotent and stale links (e.g. octocode-core) get cleaned up.
function stripOwned(resolutions) {
  const removed = [];
  for (const [name, spec] of Object.entries(resolutions)) {
    if (
      name.startsWith(SCOPE) &&
      typeof spec === 'string' &&
      (spec.startsWith('portal:') || spec.startsWith('link:'))
    ) {
      delete resolutions[name];
      removed.push(name);
    }
  }
  return removed;
}

function writeResolutions(rootPkg, rootPkgPath, resolutions) {
  if (Object.keys(resolutions).length) rootPkg.resolutions = sortObject(resolutions);
  else delete rootPkg.resolutions;
  writeFileSync(rootPkgPath, `${JSON.stringify(rootPkg, null, 2)}\n`);
}

function runInstall() {
  console.log('\nRunning `yarn install`…');
  return spawnSync('yarn', ['install'], { cwd: REPO_ROOT, stdio: 'inherit' }).status ?? 0;
}

// ---- main -----------------------------------------------------------------
const rootPkgPath = join(REPO_ROOT, 'package.json');
const rootPkg = readJson(rootPkgPath);
const members = findWorkspacePackages(rootPkg);
const octoDeps = collectOctoDeps(members.values());
const resolutions = { ...(rootPkg.resolutions ?? {}) };

if (flags.reset) {
  const removed = stripOwned(resolutions);
  writeResolutions(rootPkg, rootPkgPath, resolutions);
  console.log(
    removed.length
      ? `Removed ${removed.length} dev resolution(s):\n  ${removed.join('\n  ')}`
      : 'No portal:/link: @octocodeai resolutions to remove.'
  );
  console.log('Run `yarn install` to restore registry versions.');
  process.exit(0);
}

stripOwned(resolutions); // clean slate for owned entries (drops octocode-core etc.)

const added = [];
const ignored = [];
for (const dep of octoDeps) {
  const dir = members.get(dep);
  if (dir) {
    resolutions[dep] = `portal:${relFromRoot(dir)}`;
    added.push([dep, resolutions[dep]]);
  } else {
    ignored.push(dep); // external (non-workspace) @octocodeai dep — left as-is
  }
}

// ---- report ---------------------------------------------------------------
console.log(`\nOctocode local-dev resolution — ${REPO_ROOT}\n`);
console.log(`Workspace packages: ${members.size}`);
console.log(`@octocodeai/* dependencies referenced: ${octoDeps.length}\n`);
if (added.length) {
  console.log('  local workspace -> portal resolution:');
  for (const [dep, spec] of added) console.log(`    • ${dep}  ->  ${spec}`);
}
if (ignored.length) {
  console.log('\n  external (non-workspace) — ignored:');
  for (const dep of ignored) console.log(`    • ${dep}`);
}
console.log('');

if (!added.length) {
  console.log('No workspace @octocodeai packages referenced. Nothing to do.');
  process.exit(0);
}

if (flags.dryRun) {
  console.log('--dry-run: package.json not modified.');
  process.exit(0);
}

writeResolutions(rootPkg, rootPkgPath, resolutions);
console.log(`✔ Wrote ${added.length} resolution(s) to package.json.`);

if (flags.install) process.exit(runInstall());
console.log('Run `yarn install` to apply. Undo with `node scripts/dev.js --reset`.');
