#!/usr/bin/env node
/**
 * prepublish.mjs — publish guard: no workspace:* resolutions for managed packages.
 *
 * A single check runs before any package in this monorepo is published:
 *
 *   RESOLUTIONS CHECK — root package.json must not have workspace:* entries
 *   for managed internal packages. Publishing with workspace:* resolutions
 *   active causes Yarn to rewrite consumer deps via the local registry,
 *   producing incorrect pinned versions in published tarballs. These entries
 *   are added by `yarn devScript` for local development and must be removed
 *   before publishing.
 *
 * This script does NOT enforce version alignment between packages — each
 * package is versioned independently. The complementary npm-publish guard
 * (packages/octocode/scripts/check-no-workspace-protocol.mjs) enforces the
 * matching rule that no published package ships a local
 * (workspace:/file:/link:/portal:) dependency.
 *
 * Usage:
 *   node ./scripts/prepublish.mjs            # check only (exit 1 on issues)
 *   node ./scripts/prepublish.mjs --fix      # remove offending resolutions and write
 *   node ./scripts/prepublish.mjs --dry-run  # preview fixes without writing
 *
 * Root publish flow:
 *   "prepublish": "node ./scripts/prepublish.mjs && node ./packages/octocode/scripts/check-no-workspace-protocol.mjs && yarn readme:sync"
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const FIX = process.argv.includes('--fix');
const DRY_RUN = process.argv.includes('--dry-run');

/** Packages whose root resolutions this script manages. */
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

// ---------------------------------------------------------------------------
// Check: root resolutions must not contain workspace:* for managed packages
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
// Main
// ---------------------------------------------------------------------------

const rootPkgPath = join(ROOT, 'package.json');
const rootPkg = JSON.parse(readFileSync(rootPkgPath, 'utf8'));

const mode = DRY_RUN ? ' (dry-run)' : FIX ? ' (--fix)' : '';
console.log(`\n🔍 Prepublish check${mode}\n`);

// --- Check: resolutions ---
console.log('[ 1/1 ] Checking root resolutions…');
const resolutionIssues = checkAndFixResolutions(rootPkg, rootPkgPath);
if (resolutionIssues.length === 0) {
  console.log('  ✓ No workspace:* resolutions for managed packages.\n');
}

// --- Summary ---
if (resolutionIssues.length === 0) {
  console.log('✅ Prepublish check passed — ready to publish.\n');
  process.exit(0);
}

if (FIX || DRY_RUN) {
  if (DRY_RUN) {
    console.log(
      `📋 Dry-run: ${resolutionIssues.length} fix(es) would be applied. Re-run without --dry-run to apply.\n`
    );
  } else {
    console.log(`✅ Fixed ${resolutionIssues.length} issue(s). Run \`yarn install\` to apply.\n`);
  }
  process.exit(0);
}

// Check-only mode: report and exit 1
console.error('\n✗ Prepublish check failed:\n');
console.error(`  workspace:* resolutions still present in root package.json:`);
for (const name of resolutionIssues) {
  console.error(`    resolutions.${name}: "workspace:*"`);
}
console.error(`\n  These were added by \`yarn devScript\` for local development.`);
console.error(`  Remove them before publishing: \`node ./scripts/prepublish.mjs --fix\`\n`);
process.exit(1);
