#!/usr/bin/env node

/**
 * sync-packages-local.mjs
 *
 * Validates (and optionally fixes) that every internal package dependency
 * across the monorepo uses the workspace protocol (`workspace:*` or
 * `workspace:^`) instead of a pinned version string.
 *
 * A dependency is "internal" when its name matches a package that is listed in
 * the yarn workspaces config — including the platform-specific npm sub-packages
 * (`packages/octocode-context-utils/npm/*`, `packages/octocode-security/npm/*`).
 *
 * Usage:
 *   node scripts/sync-packages-local.mjs            # check only — exits 1 on violations
 *   node scripts/sync-packages-local.mjs --fix      # auto-correct to workspace:*
 *   node scripts/sync-packages-local.mjs --fix --protocol "workspace:^"
 *   node scripts/sync-packages-local.mjs --verbose  # print every dep that was checked
 */

import { readFileSync, writeFileSync } from 'fs';
import { resolve, dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { readdirSync, statSync } from 'fs';
import { globSync } from 'fs'; // node 22+ — falls back to manual expand

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

// ── CLI flags ─────────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
const FIX = args.includes('--fix');
const VERBOSE = args.includes('--verbose');
const PROTOCOL_IDX = args.indexOf('--protocol');
const TARGET_PROTOCOL =
  PROTOCOL_IDX !== -1 ? args[PROTOCOL_IDX + 1] : 'workspace:*';

const VALID_PROTOCOLS = ['workspace:*', 'workspace:^', 'workspace:~'];

if (!VALID_PROTOCOLS.includes(TARGET_PROTOCOL)) {
  console.error(
    `Invalid --protocol "${TARGET_PROTOCOL}". Must be one of: ${VALID_PROTOCOLS.join(', ')}`
  );
  process.exit(2);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, 'utf8'));
}

function writeJson(filePath, data) {
  writeFileSync(filePath, JSON.stringify(data, null, 2) + '\n', 'utf8');
}

function isWorkspaceRef(value) {
  return (
    typeof value === 'string' && value.startsWith('workspace:')
  );
}

// ── Workspace glob expansion ──────────────────────────────────────────────────

/**
 * Expand yarn workspace globs (`packages/*`, `packages/foo/npm/*`, etc.)
 * into concrete directory paths. Returns `{ packageJsonPath, dir }[]`.
 */
function expandWorkspaceGlobs(globs) {
  const results = [];

  for (const pattern of globs) {
    // Handle patterns ending in /* — enumerate that directory
    const parts = pattern.replace(/\\/g, '/').split('/');
    const lastPart = parts[parts.length - 1];

    if (lastPart === '*') {
      const parentDir = join(ROOT, ...parts.slice(0, -1));
      let entries;
      try {
        entries = readdirSync(parentDir);
      } catch {
        continue;
      }
      for (const entry of entries) {
        const dir = join(parentDir, entry);
        try {
          if (!statSync(dir).isDirectory()) continue;
        } catch {
          continue;
        }
        const pjPath = join(dir, 'package.json');
        try {
          readJson(pjPath); // verify it parses
          results.push({ packageJsonPath: pjPath, dir });
        } catch {
          // not a package — skip
        }
      }
    } else {
      // Exact path
      const dir = join(ROOT, pattern);
      const pjPath = join(dir, 'package.json');
      try {
        readJson(pjPath);
        results.push({ packageJsonPath: pjPath, dir });
      } catch {
        // doesn't exist
      }
    }
  }

  return results;
}

// ── Collect workspace packages ────────────────────────────────────────────────

const rootPkg = readJson(join(ROOT, 'package.json'));
const workspaceGlobs = Array.isArray(rootPkg.workspaces)
  ? rootPkg.workspaces
  : rootPkg.workspaces?.packages ?? [];

const allWorkspaceEntries = expandWorkspaceGlobs(workspaceGlobs);

/** Map: package-name → dir */
const workspaceByName = new Map();
for (const { packageJsonPath, dir } of allWorkspaceEntries) {
  try {
    const pkg = readJson(packageJsonPath);
    if (pkg.name) {
      workspaceByName.set(pkg.name, { dir, packageJsonPath });
    }
  } catch {
    // skip
  }
}

if (workspaceByName.size === 0) {
  console.error('No workspace packages found. Check your package.json workspaces config.');
  process.exit(2);
}

// ── Check / fix each package ──────────────────────────────────────────────────

const DEP_FIELDS = [
  'dependencies',
  'devDependencies',
  'peerDependencies',
  'optionalDependencies',
];

/** @type {{ packageJsonPath: string, field: string, name: string, current: string, fixed: string }[]} */
const violations = [];

/** @type {{ packageJsonPath: string, count: number }[]} */
const fixed = [];

let totalChecked = 0;

for (const { packageJsonPath, dir } of allWorkspaceEntries) {
  let pkg;
  try {
    pkg = readJson(packageJsonPath);
  } catch {
    continue;
  }

  const relativePath = packageJsonPath.replace(ROOT + '/', '');
  let pkgViolations = 0;

  for (const field of DEP_FIELDS) {
    const deps = pkg[field];
    if (!deps) continue;

    for (const [depName, depValue] of Object.entries(deps)) {
      if (!workspaceByName.has(depName)) continue; // not an internal package

      totalChecked++;

      if (isWorkspaceRef(depValue)) {
        if (VERBOSE) {
          console.log(`  ✓  ${relativePath}  [${field}]  ${depName}  = ${depValue}`);
        }
        continue;
      }

      // Violation found
      violations.push({
        packageJsonPath,
        relativePath,
        field,
        name: depName,
        current: depValue,
        fixed: TARGET_PROTOCOL,
      });
      pkgViolations++;

      if (FIX) {
        deps[depName] = TARGET_PROTOCOL;
      }
    }
  }

  if (FIX && pkgViolations > 0) {
    writeJson(packageJsonPath, pkg);
    fixed.push({ relativePath, count: pkgViolations });
  }
}

// ── Report ────────────────────────────────────────────────────────────────────

const COL_W = 60;

console.log('\n');
console.log('━'.repeat(COL_W));
console.log('  sync-packages-local — workspace dependency check');
console.log('━'.repeat(COL_W));
console.log(`  Workspace packages : ${workspaceByName.size}`);
console.log(`  Internal deps checked: ${totalChecked}`);
console.log(`  Violations found   : ${violations.length}`);
if (FIX) {
  console.log(`  Fixed              : ${violations.length}`);
  console.log(`  Target protocol    : ${TARGET_PROTOCOL}`);
}
console.log('━'.repeat(COL_W));

if (violations.length === 0) {
  console.log('\n  ✅  All internal dependencies use the workspace protocol.\n');
  process.exit(0);
}

// Print violations table
console.log('\n  ❌  Violations:\n');
console.log(
  `  ${'File'.padEnd(45)}  ${'Field'.padEnd(22)}  ${'Package'.padEnd(48)}  Current → Expected`
);
console.log('  ' + '─'.repeat(150));

for (const v of violations) {
  const file = v.relativePath.padEnd(45);
  const field = v.field.padEnd(22);
  const name = v.name.padEnd(48);
  const change = FIX
    ? `${v.current}  →  ${v.fixed}  (fixed)`
    : `${v.current}  →  ${v.fixed}  ← MISSING`;
  console.log(`  ${file}  ${field}  ${name}  ${change}`);
}

if (FIX) {
  console.log('\n  ✅  All violations fixed.\n');
  console.log('  Files updated:');
  for (const f of fixed) {
    console.log(`    ${f.relativePath}  (${f.count} dep${f.count > 1 ? 's' : ''} updated)`);
  }
  console.log('\n  Re-run yarn install to sync the lockfile.\n');
  process.exit(0);
} else {
  console.log(
    `\n  Run with --fix to auto-correct all ${violations.length} violation${violations.length > 1 ? 's' : ''}:\n`
  );
  console.log('    node scripts/sync-packages-local.mjs --fix\n');
  process.exit(1);
}
