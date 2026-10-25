#!/usr/bin/env node
/**
 * prepublish.mjs — publish guard: no local resolutions for managed packages,
 * and (with --package) no workspace dependency the registry does not serve yet.
 *
 *   RESOLUTIONS CHECK — root package.json must not have local workspace/file/
 *   link/portal entries for managed packages. Publishing with local resolutions
 *   active causes Yarn to rewrite consumer deps via the local registry,
 *   producing incorrect pinned versions in published tarballs. These entries
 *   are added by `node skills-dev/octocode-dev/scripts/dev.mjs setup` for local development and must be removed
 *   before publishing.
 *
 *   PUBLISHED DEPENDENCIES CHECK (--package <name>) — every public workspace
 *   package that <name> depends on must already be on npm at its workspace
 *   version (`npm view <dep>@<version> version`). A range floor such as the
 *   Pi extension's `octocode-mcp@^20.2.0` cannot resolve until then, so
 *   publishing <name> first would ship a package whose dependency fails.
 *
 * This script does NOT enforce version alignment between packages — each
 * package is versioned independently.
 *
 * Usage:
 *   node ./skills-dev/octocode-dev/scripts/prepublish.mjs            # check only (exit 1 on issues)
 *   node ./skills-dev/octocode-dev/scripts/prepublish.mjs --fix      # remove offending resolutions and write
 *   node ./skills-dev/octocode-dev/scripts/prepublish.mjs --dry-run  # preview fixes without writing
 *   node ./skills-dev/octocode-dev/scripts/prepublish.mjs --package @octocodeai/pi-extension
 */

import { isMainModule } from './lib/entrypoint.mjs';
import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { ROOT } from './lib/workspaces.mjs';
import {
  isLocalResolution,
  managedResolutionPackages,
} from './dev-resolution-contract.mjs';

const FIX = process.argv.includes('--fix');
const DRY_RUN = process.argv.includes('--dry-run');
const PACKAGE_INDEX = process.argv.indexOf('--package');
const PACKAGE = PACKAGE_INDEX >= 0 ? process.argv[PACKAGE_INDEX + 1] : undefined;

/** Packages whose root resolutions this script manages. */
const MANAGED_PACKAGES = new Set(managedResolutionPackages());

// ---------------------------------------------------------------------------
// Check: root resolutions must not contain local protocols for managed packages
// ---------------------------------------------------------------------------

function checkAndFixResolutions(rootPkg, rootPkgPath) {
  const issues = [];
  const resolutions = rootPkg.resolutions ?? {};

  for (const name of MANAGED_PACKAGES) {
    if (isLocalResolution(resolutions[name])) {
      issues.push(name);
    }
  }

  if (issues.length === 0) return issues;

  if (FIX || DRY_RUN) {
    for (const name of issues) {
      if (!DRY_RUN) delete resolutions[name];
      console.log(
        `  ${DRY_RUN ? '~' : '-'} resolutions.${name} (local resolution removed)`
      );
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
// Check: workspace dependencies of --package are already on npm
// ---------------------------------------------------------------------------

/** Every workspace package manifest, by npm name. */
function workspaceManifests() {
  const manifests = new Map();
  for (const entry of readdirSync(join(ROOT, 'packages'), { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    try {
      const manifest = JSON.parse(readFileSync(join(ROOT, 'packages', entry.name, 'package.json'), 'utf8'));
      if (manifest.name) manifests.set(manifest.name, manifest);
    } catch {}
  }
  return manifests;
}

/** The published version npm serves for `spec`, or null. */
function npmView(spec) {
  try {
    const version = execFileSync('npm', ['view', spec, 'version'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
      timeout: 30_000,
    }).trim();
    return version || null;
  } catch {
    return null;
  }
}

/**
 * `<dep>@<workspace version>` for each public workspace dependency of `pkg`
 * the registry does not serve at that exact version. `view(spec)` returns the
 * published version or null.
 */
export function unpublishedWorkspaceDependencies(pkg, manifests, view) {
  const names = Object.keys({ ...pkg.dependencies, ...pkg.peerDependencies });
  return names.flatMap(name => {
    const workspace = manifests.get(name);
    if (!workspace || workspace.private) return [];
    const spec = `${name}@${workspace.version}`;
    return view(spec) === workspace.version ? [] : [spec];
  });
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function main() {
  const rootPkgPath = join(ROOT, 'package.json');
  const rootPkg = JSON.parse(readFileSync(rootPkgPath, 'utf8'));

  const mode = DRY_RUN ? ' (dry-run)' : FIX ? ' (--fix)' : '';
  console.log(`\n🔍 Prepublish check${mode}\n`);

  const steps = PACKAGE ? 2 : 1;

  // --- Check: resolutions ---
  console.log(`[ 1/${steps} ] Checking root resolutions…`);
  const resolutionIssues = checkAndFixResolutions(rootPkg, rootPkgPath);
  if (resolutionIssues.length === 0) {
    console.log('  ✓ No local resolutions for managed packages.\n');
  }

  // --- Check: published workspace dependencies ---
  if (PACKAGE) {
    console.log(`[ 2/2 ] Checking that ${PACKAGE}'s workspace dependencies are published…`);
    const manifests = workspaceManifests();
    const pkg = manifests.get(PACKAGE);
    if (!pkg) {
      console.error(`  ✗ No workspace package is named ${PACKAGE}.\n`);
      process.exit(1);
    }
    const unpublished = unpublishedWorkspaceDependencies(pkg, manifests, npmView);
    if (unpublished.length > 0) {
      console.error(`  ✗ Not on npm yet: ${unpublished.join(', ')}. Publish them before ${PACKAGE}.\n`);
      process.exit(1);
    }
    console.log('  ✓ Every workspace dependency is on npm at its workspace version.\n');
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
      console.log(
        `✅ Fixed ${resolutionIssues.length} issue(s). Run \`yarn install\` to apply.\n`
      );
    }
    process.exit(0);
  }

  // Check-only mode: report and exit 1
  console.error('\n✗ Prepublish check failed:\n');
  console.error(`  local resolutions still present in root package.json:`);
  for (const name of resolutionIssues) {
    console.error(`    resolutions.${name}: "${rootPkg.resolutions?.[name]}"`);
  }
  console.error(
    `\n  These were added by \`node skills-dev/octocode-dev/scripts/dev.mjs setup\` for local development.`
  );
  console.error(
    `  Remove them before publishing: \`node ./skills-dev/octocode-dev/scripts/prepublish.mjs --fix\`\n`
  );
  process.exit(1);
}

if (isMainModule(import.meta.url)) main();
