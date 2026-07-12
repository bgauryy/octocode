#!/usr/bin/env node
/**
 * sync-version-publish.mjs — pin workspace:* deps to real semver before npm publish.
 *
 * For each workspace:* reference in this package's published dependency fields,
 * finds the corresponding workspace package's current version and replaces the
 * workspace: protocol with ^<version>.
 *
 * Usage:
 *   node scripts/sync-version-publish.mjs            # apply changes
 *   node scripts/sync-version-publish.mjs --dry-run  # preview without writing
 *
 * Run via:
 *   yarn sync:version:publish
 */
import { readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = resolve(join(dirname(fileURLToPath(import.meta.url)), '..'));
const repoRoot = resolve(join(packageRoot, '..', '..'));
const dryRun = process.argv.includes('--dry-run');

/** Build a map of workspace package name → version by scanning workspace globs. */
function buildWorkspaceVersionMap() {
  const map = new Map();
  const rootPkgPath = join(repoRoot, 'package.json');
  if (!existsSync(rootPkgPath)) return map;

  const rootPkg = JSON.parse(readFileSync(rootPkgPath, 'utf8'));
  for (const ws of rootPkg.workspaces ?? []) {
    // Strip glob suffix: "packages/*" → "packages", "packages/foo" → "packages/foo"
    const baseDir = ws.replace(/\/\*.*$/, '');
    const absDir = join(repoRoot, baseDir);
    if (!existsSync(absDir)) continue;

    for (const entry of readdirSync(absDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const pkgPath = join(absDir, entry.name, 'package.json');
      if (!existsSync(pkgPath)) continue;
      try {
        const { name, version } = JSON.parse(readFileSync(pkgPath, 'utf8'));
        if (name && version) map.set(name, version);
      } catch {
        /* skip unparseable entries */
      }
    }
  }
  return map;
}

const PUBLISHED_DEP_FIELDS = [
  'dependencies',
  'optionalDependencies',
  'peerDependencies',
  'bundledDependencies',
  'bundleDependencies',
];

const workspaceVersions = buildWorkspaceVersionMap();
const pkgPath = join(packageRoot, 'package.json');
const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));
const pinned = [];
const errors = [];

for (const field of PUBLISHED_DEP_FIELDS) {
  const deps = pkg[field];
  if (!deps || typeof deps !== 'object') continue;
  for (const [name, spec] of Object.entries(deps)) {
    if (typeof spec !== 'string' || !spec.startsWith('workspace:')) continue;
    const realVersion = workspaceVersions.get(name);
    if (!realVersion) {
      errors.push(`  ${field}.${name}: "workspace:*" — no local version found; pin manually`);
      continue;
    }
    deps[name] = `^${realVersion}`;
    pinned.push(`  ${field}.${name}: "workspace:*" → "^${realVersion}"`);
  }
}

if (errors.length > 0) {
  console.error(`\n✗ ${pkg.name}: could not resolve all workspace: deps:\n${errors.join('\n')}\n`);
  process.exit(1);
}

if (pinned.length === 0) {
  console.log(`✓ ${pkg.name}@${pkg.version}: no workspace:* deps in published fields — nothing to sync`);
  process.exit(0);
}

if (dryRun) {
  console.log(`Dry run for ${pkg.name}@${pkg.version} — would pin:\n${pinned.join('\n')}`);
} else {
  writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');
  console.log(`✓ Synced ${pkg.name}@${pkg.version} — pinned ${pinned.length} dep(s):\n${pinned.join('\n')}`);
}
