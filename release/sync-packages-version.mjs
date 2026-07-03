#!/usr/bin/env node
/**
 * Pin or restore workspace:* runtime-dependency refs across publishable packages.
 *
 * Usage:
 *   node release/sync-packages-version.mjs --pin-for-publish
 *     Replaces every workspace:* ref in a publishable package's runtime dep
 *     fields (dependencies / optionalDependencies / peerDependencies) with the
 *     actual workspace package's semver version, prefixed with "^".
 *     Run this BEFORE publishing so npm consumers get a real version.
 *
 *   node release/sync-packages-version.mjs
 *     Restores every pinned internal dep back to "workspace:*".
 *     Run this AFTER publishing to resume local development.
 *
 * Idempotent: re-running with the state already in the target form is a no-op.
 * Safe: only touches dep fields in the workspace packages listed below;
 * devDependencies are untouched (tools-core / config are build-time only).
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const PIN = process.argv.includes('--pin-for-publish');

const PUBLISHED_DEP_FIELDS = [
  'dependencies',
  'optionalDependencies',
  'peerDependencies',
];

// ---------------------------------------------------------------------------
// 1.  Build a name → version map for every workspace package.
// ---------------------------------------------------------------------------
function readJson(p) {
  return JSON.parse(readFileSync(p, 'utf8'));
}

function writeJson(p, data) {
  writeFileSync(p, JSON.stringify(data, null, 2) + '\n', 'utf8');
}

const rootPkg = readJson(join(ROOT, 'package.json'));
const workspaceGlobs = rootPkg.workspaces ?? [];

// Expand simple "packages/*" globs — we only need the top-level packages dirs.
const { readdirSync, existsSync } = await import('node:fs');
const workspacePkgs = new Map(); // name → { version, dir }

for (const glob of workspaceGlobs) {
  // Handle "packages/*" and "packages/octocode-engine/npm/*" patterns.
  const baseDir = join(ROOT, glob.replace(/\/\*$/, ''));
  if (!existsSync(baseDir)) continue;
  for (const entry of readdirSync(baseDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const pkgPath = join(baseDir, entry.name, 'package.json');
    if (!existsSync(pkgPath)) continue;
    try {
      const p = readJson(pkgPath);
      if (p.name && p.version) {
        workspacePkgs.set(p.name, { version: p.version, dir: join(baseDir, entry.name) });
      }
    } catch { /* skip malformed */ }
  }
}

// Also include skills/package.json if it exists.
const skillsPkg = join(ROOT, 'skills', 'package.json');
if (existsSync(skillsPkg)) {
  try {
    const p = readJson(skillsPkg);
    if (p.name && p.version) workspacePkgs.set(p.name, { version: p.version, dir: join(ROOT, 'skills') });
  } catch { /* skip */ }
}

// ---------------------------------------------------------------------------
// 2.  For each workspace package that has runtime deps pointing at siblings,
//     pin or restore as requested.
// ---------------------------------------------------------------------------
let changed = 0;
let alreadyCorrect = 0;

for (const [, { dir }] of workspacePkgs) {
  const pkgPath = join(dir, 'package.json');
  if (!existsSync(pkgPath)) continue;
  const pkg = readJson(pkgPath);
  let dirty = false;

  for (const field of PUBLISHED_DEP_FIELDS) {
    const deps = pkg[field];
    if (!deps || typeof deps !== 'object') continue;

    for (const [name, spec] of Object.entries(deps)) {
      if (!workspacePkgs.has(name)) continue; // not an internal dep

      if (PIN) {
        // Pin: workspace:* → ^version
        if (typeof spec === 'string' && spec.startsWith('workspace:')) {
          const target = `^${workspacePkgs.get(name).version}`;
          deps[name] = target;
          console.log(`  ${pkg.name} ${field}.${name}: ${spec} → ${target}`);
          dirty = true;
        }
      } else {
        // Restore: ^version → workspace:*
        if (typeof spec === 'string' && !spec.startsWith('workspace:') && !spec.startsWith('file:') && !spec.startsWith('portal:')) {
          deps[name] = 'workspace:*';
          console.log(`  ${pkg.name} ${field}.${name}: ${spec} → workspace:*`);
          dirty = true;
        }
      }
    }
  }

  if (dirty) {
    writeJson(pkgPath, pkg);
    changed++;
  } else {
    alreadyCorrect++;
  }
}

const mode = PIN ? 'pinned for publish' : 'restored to workspace:*';
console.log(`\nversion:sync ${mode}: ${changed} package(s) updated, ${alreadyCorrect} already correct.`);
if (changed > 0) {
  console.log(PIN
    ? 'Run `yarn install` after pinning if you need a consistent lock file, then publish.'
    : 'Run `yarn install` to refresh symlinks.');
}
