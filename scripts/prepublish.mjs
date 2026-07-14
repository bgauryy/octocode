#!/usr/bin/env node
/**
 * prepublish.mjs — publish guard: workspace:* resolutions + version alignment.
 *
 * Runs two checks before any package in this monorepo is published:
 *
 *   1. RESOLUTIONS CHECK — root package.json must not have workspace:* entries
 *      for the four managed internal packages.  Publishing with workspace:*
 *      resolutions active causes Yarn to rewrite consumer deps via the local
 *      registry, producing incorrect pinned versions in published tarballs.
 *
 *   2. VERSION ALIGNMENT — every workspace package that depends on one of the
 *      four managed packages must declare a version spec that matches the
 *      package's current version in this repo (format: "^<version>", or the
 *      exact version string).  workspace:* in devDependencies is always valid
 *      and is left untouched.
 *
 * Usage:
 *   node ./scripts/prepublish.mjs          # check only (exit 1 on issues)
 *   node ./scripts/prepublish.mjs --fix    # fix issues and write files
 *   node ./scripts/prepublish.mjs --dry-run  # preview fixes without writing
 *
 * Wire into each package's prepublishOnly:
 *   "prepublishOnly": "node ../../scripts/prepublish.mjs && node scripts/check-no-workspace-protocol.mjs"
 *
 * Wire into the root prepublish:
 *   "prepublish": "node ./scripts/prepublish.mjs && yarn readme:sync"
 */

import { readFileSync, writeFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const FIX = process.argv.includes('--fix');
const DRY_RUN = process.argv.includes('--dry-run');

/** Packages whose resolutions + dep versions this script manages. */
const ENGINE_PKG_PATH = join(ROOT, 'packages/octocode-engine/package.json');
const enginePkg = JSON.parse(readFileSync(ENGINE_PKG_PATH, 'utf8'));
const enginePlatformPackages = Object.keys(enginePkg.optionalDependencies ?? {}).filter((name) =>
  name.startsWith('@octocodeai/octocode-engine-')
);
const MANAGED_PACKAGES = new Set([
  '@octocodeai/octocode-tools-core',
  '@octocodeai/config',
  '@octocodeai/octocode-core',
  '@octocodeai/octocode-engine',
  ...enginePlatformPackages,
]);

/** Dependency fields that are included in a published package. */
const PUBLISHED_DEP_FIELDS = ['dependencies', 'optionalDependencies', 'peerDependencies'];

/** All dependency fields — also check devDependencies for version alignment (not published, but consistency matters). */
const ALL_DEP_FIELDS = [...PUBLISHED_DEP_FIELDS, 'devDependencies'];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a name→version map for every workspace package in the repo. */
function buildWorkspaceVersionMap() {
  const map = new Map();
  const rootPkgPath = join(ROOT, 'package.json');
  if (!existsSync(rootPkgPath)) return map;

  const rootPkg = JSON.parse(readFileSync(rootPkgPath, 'utf8'));
  for (const wsGlob of rootPkg.workspaces ?? []) {
    const baseDir = wsGlob.replace(/\/\*.*$/, '');
    const absDir = join(ROOT, baseDir);
    if (!existsSync(absDir)) continue;

    for (const entry of readdirSync(absDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const pkgJsonPath = join(absDir, entry.name, 'package.json');
      if (!existsSync(pkgJsonPath)) continue;
      try {
        const { name, version } = JSON.parse(readFileSync(pkgJsonPath, 'utf8'));
        if (name && version) map.set(name, version);
      } catch {
        /* skip unparseable entries */
      }
    }
  }
  return map;
}

/** Collect all workspace package.json paths (root + every packages/<name>). */
function collectPackageJsons(rootPkg, rootPkgPath) {
  const results = [{ file: rootPkgPath, json: rootPkg }];

  for (const wsGlob of rootPkg.workspaces ?? []) {
    const baseDir = wsGlob.replace(/\/\*.*$/, '');
    const absDir = join(ROOT, baseDir);
    if (!existsSync(absDir)) continue;

    for (const entry of readdirSync(absDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const pkgJsonPath = join(absDir, entry.name, 'package.json');
      if (!existsSync(pkgJsonPath)) continue;
      try {
        results.push({ file: pkgJsonPath, json: JSON.parse(readFileSync(pkgJsonPath, 'utf8')) });
      } catch {
        /* skip */
      }
    }
  }

  return results;
}

// ---------------------------------------------------------------------------
// Check 1: root resolutions must not contain workspace:* for managed packages
// ---------------------------------------------------------------------------

function checkAndFixResolutions(rootPkg, rootPkgPath) {
  const issues = [];
  const resolutions = rootPkg.resolutions ?? {};

  for (const name of MANAGED_PACKAGES) {
    if (resolutions[name] === 'workspace:*') {
      issues.push(name);
    }
  }

  if (issues.length === 0) return issues;

  if (FIX || DRY_RUN) {
    for (const name of issues) {
      if (!DRY_RUN) delete resolutions[name];
      console.log(`  ${DRY_RUN ? '~' : '-'} resolutions.${name} (workspace:* removed)`);
    }
    // Drop the resolutions key entirely if it became empty
    if (!DRY_RUN && Object.keys(resolutions).length === 0) {
      delete rootPkg.resolutions;
    }
    if (!DRY_RUN) {
      writeFileSync(rootPkgPath, JSON.stringify(rootPkg, null, 2) + '\n');
      console.log(`  ✓ root package.json updated.\n`);
    }
  }

  return issues;
}

// ---------------------------------------------------------------------------
// Check 2: dep version specs must match the repo version for managed packages
// ---------------------------------------------------------------------------

/**
 * Normalise a dep spec so we can compare it to a bare version string.
 * "^16.6.3" → "16.6.3", "16.6.3" → "16.6.3".
 */
function stripRange(spec) {
  return String(spec).replace(/^[\^~>=<\s]+/, '').trim();
}

function checkAndFixVersions(allPkgs, versionMap) {
  const issues = [];
  const dirtyFiles = new Set();

  for (const { file, json } of allPkgs) {
    if (!json.name || json.name === 'octocode-monorepo') continue; // skip root manifest

    for (const field of ALL_DEP_FIELDS) {
      const deps = json[field];
      if (!deps || typeof deps !== 'object') continue;

      for (const name of MANAGED_PACKAGES) {
        const currentSpec = deps[name];
        if (!currentSpec) continue;
        if (currentSpec === 'workspace:*') continue; // always valid

        const repoVersion = versionMap.get(name);
        if (!repoVersion) {
          // Package not in this workspace (e.g. @octocodeai/octocode-core lives in a sibling repo).
          // Nothing to align against — skip silently.
          continue;
        }

        const expectedSpec = `^${repoVersion}`;
        if (stripRange(currentSpec) !== repoVersion) {
          issues.push({
            pkg: json.name,
            field,
            dep: name,
            current: currentSpec,
            expected: expectedSpec,
            canFix: true,
          });

          if ((FIX || DRY_RUN) && !DRY_RUN) {
            deps[name] = expectedSpec;
            dirtyFiles.add(file);
          }

          console.log(
            `  ${DRY_RUN ? '~' : FIX ? '✓' : '✗'} ${json.name}: ${field}.${name}: "${currentSpec}" → "${expectedSpec}"`
          );
        }
      }
    }
  }

  // Write changed package.json files
  if (FIX && !DRY_RUN) {
    for (const file of dirtyFiles) {
      const entry = allPkgs.find((p) => p.file === file);
      if (entry) {
        writeFileSync(file, JSON.stringify(entry.json, null, 2) + '\n');
      }
    }
    if (dirtyFiles.size > 0) {
      console.log(`  ✓ Updated ${dirtyFiles.size} package.json file(s).\n`);
    }
  }

  return issues;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const rootPkgPath = join(ROOT, 'package.json');
const rootPkg = JSON.parse(readFileSync(rootPkgPath, 'utf8'));
const versionMap = buildWorkspaceVersionMap();
const allPkgs = collectPackageJsons(rootPkg, rootPkgPath);

const mode = DRY_RUN ? ' (dry-run)' : FIX ? ' (--fix)' : '';
console.log(`\n🔍 Prepublish check${mode}\n`);

// --- Check 1: resolutions ---
console.log('[ 1/2 ] Checking root resolutions…');
const resolutionIssues = checkAndFixResolutions(rootPkg, rootPkgPath);
if (resolutionIssues.length === 0) {
  console.log('  ✓ No workspace:* resolutions for managed packages.\n');
}

// --- Check 2: version alignment ---
console.log('[ 2/2 ] Checking dependency version alignment…');
const versionIssues = checkAndFixVersions(allPkgs, versionMap);
const fixableVersionIssues = versionIssues.filter((i) => i.canFix);
const unfixableVersionIssues = versionIssues.filter((i) => !i.canFix);

if (versionIssues.length === 0) {
  console.log('  ✓ All managed dependency versions are aligned.\n');
} else if (unfixableVersionIssues.length > 0 && !FIX) {
  console.log('');
  for (const { pkg, field, dep, current, expected } of unfixableVersionIssues) {
    console.log(`  ⚠  ${pkg}: ${field}.${dep}: "${current}" — ${expected}`);
  }
  console.log('');
}

// --- Summary ---
const totalIssues = resolutionIssues.length + versionIssues.length;

if (totalIssues === 0) {
  console.log('✅ Prepublish check passed — ready to publish.\n');
  process.exit(0);
}

if (FIX || DRY_RUN) {
  const fixedCount = resolutionIssues.length + fixableVersionIssues.length;
  const unfixedCount = unfixableVersionIssues.length;
  if (unfixedCount > 0) {
    console.warn(
      `⚠  ${unfixedCount} issue(s) could not be auto-fixed (external packages not in workspace). Fix manually.\n`
    );
    process.exit(1);
  }
  if (DRY_RUN) {
    console.log(`📋 Dry-run: ${fixedCount} fix(es) would be applied. Re-run without --dry-run to apply.\n`);
  } else {
    console.log(`✅ Fixed ${fixedCount} issue(s). Run \`yarn install\` to apply.\n`);
  }
  process.exit(0);
}

// Check-only mode: report all issues and exit 1
console.error('\n✗ Prepublish checks failed:\n');

if (resolutionIssues.length > 0) {
  console.error(`  workspace:* resolutions still present in root package.json:`);
  for (const name of resolutionIssues) {
    console.error(`    resolutions.${name}: "workspace:*"`);
  }
  console.error(`\n  These were added by \`yarn devScript\` for local development.`);
  console.error(`  Remove them before publishing: \`node ./scripts/prepublish.mjs --fix\`\n`);
}

if (fixableVersionIssues.length > 0) {
  console.error(`  Dependency version mismatches (fixable):`);
  for (const { pkg, field, dep, current, expected } of fixableVersionIssues) {
    console.error(`    ${pkg}: ${field}.${dep}: "${current}" (expected "${expected}")`);
  }
  console.error(`\n  Run \`node ./scripts/prepublish.mjs --fix\` to align all versions.\n`);
}

if (unfixableVersionIssues.length > 0) {
  console.error(`  Dependency version mismatches (manual fix required):`);
  for (const { pkg, field, dep, current, expected } of unfixableVersionIssues) {
    console.error(`    ${pkg}: ${field}.${dep}: "${current}" — ${expected}`);
  }
  console.error('');
}

process.exit(1);
