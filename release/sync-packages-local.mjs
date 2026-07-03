#!/usr/bin/env node
/**
 * Check or fix local-dev workspace:* dependency links across all packages.
 *
 * Usage:
 *   node release/sync-packages-local.mjs             (--check implied)
 *     Exits 0 if every internal dep that CAN use workspace:* already does.
 *     Exits 1 and lists the offenders otherwise.
 *
 *   node release/sync-packages-local.mjs --fix
 *     Restores every internal dep (in ALL dep fields including devDependencies)
 *     that has been pinned to a semver back to "workspace:*" so local builds
 *     resolve from workspace source rather than the published npm package.
 *
 * Idempotent: already-correct entries are untouched.
 * Scope: only internal deps — packages that exist in this workspace.
 *        External deps (react, zod, …) are never touched.
 */

import { readFileSync, writeFileSync, readdirSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const FIX  = process.argv.includes('--fix');

const ALL_DEP_FIELDS = [
  'dependencies',
  'devDependencies',
  'optionalDependencies',
  'peerDependencies',
];

// Dep ranges that came from a pin step — restore these to workspace:*.
// Excludes portal: / file: refs (those are intentional local overrides).
function isPinnedInternal(spec) {
  if (typeof spec !== 'string') return false;
  if (spec.startsWith('workspace:')) return false;
  if (spec.startsWith('file:')) return false;
  if (spec.startsWith('portal:')) return false;
  return true; // semver range like "^1.0.2"
}

function readJson(p) {
  return JSON.parse(readFileSync(p, 'utf8'));
}

function writeJson(p, data) {
  writeFileSync(p, JSON.stringify(data, null, 2) + '\n', 'utf8');
}

// ---------------------------------------------------------------------------
// 1.  Build workspace package name set.
// ---------------------------------------------------------------------------
const rootPkg = readJson(join(ROOT, 'package.json'));
const workspaceGlobs = rootPkg.workspaces ?? [];
const workspaceNames = new Set();

for (const glob of workspaceGlobs) {
  const baseDir = join(ROOT, glob.replace(/\/\*$/, ''));
  if (!existsSync(baseDir)) continue;
  for (const entry of readdirSync(baseDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const pkgPath = join(baseDir, entry.name, 'package.json');
    if (!existsSync(pkgPath)) continue;
    try {
      const p = readJson(pkgPath);
      if (p.name) workspaceNames.add(p.name);
    } catch { /* skip */ }
  }
}

const skillsPkg = join(ROOT, 'skills', 'package.json');
if (existsSync(skillsPkg)) {
  try { const p = readJson(skillsPkg); if (p.name) workspaceNames.add(p.name); } catch { /* skip */ }
}

// ---------------------------------------------------------------------------
// 2.  Scan / fix all packages.
// ---------------------------------------------------------------------------
const offenders = []; // [{ pkg, field, name, spec }]
let fixed = 0;
let correct = 0;

for (const glob of workspaceGlobs) {
  const baseDir = join(ROOT, glob.replace(/\/\*$/, ''));
  if (!existsSync(baseDir)) continue;
  for (const entry of readdirSync(baseDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const pkgPath = join(baseDir, entry.name, 'package.json');
    if (!existsSync(pkgPath)) continue;

    let pkg;
    try { pkg = readJson(pkgPath); } catch { continue; }

    let dirty = false;
    for (const field of ALL_DEP_FIELDS) {
      const deps = pkg[field];
      if (!deps || typeof deps !== 'object') continue;
      for (const [name, spec] of Object.entries(deps)) {
        if (!workspaceNames.has(name)) continue;
        if (spec.startsWith('workspace:')) { correct++; continue; }
        if (!isPinnedInternal(spec)) { correct++; continue; }
        // Found a pinned internal dep
        if (FIX) {
          deps[name] = 'workspace:*';
          console.log(`  ${pkg.name} ${field}.${name}: ${spec} → workspace:*`);
          dirty = true;
          fixed++;
        } else {
          offenders.push({ pkg: pkg.name, field, name, spec });
        }
      }
    }
    if (dirty) writeJson(pkgPath, pkg);
  }
}

if (FIX) {
  console.log(`\nlocal:fix done: ${fixed} dep(s) restored to workspace:*, ${correct} already correct.`);
  if (fixed > 0) console.log('Run `yarn install` to refresh symlinks before building.');
  process.exit(0);
}

if (offenders.length === 0) {
  console.log(`local:check ✓ — all ${correct} internal deps use workspace:*.`);
  process.exit(0);
}

console.error(`\nlocal:check failed — ${offenders.length} internal dep(s) are pinned (not workspace:*):`);
for (const { pkg, field, name, spec } of offenders) {
  console.error(`  ${pkg} ${field}.${name}: "${spec}"`);
}
console.error('\nRun `yarn local:fix` to restore workspace links for local development.');
process.exit(1);
