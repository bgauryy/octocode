#!/usr/bin/env node

/**
 * Syncs the version of all packages in the monorepo to match
 * packages/octocode-mcp/package.json.
 *
 * Updates:
 *  - version field in every main package
 *  - version field in every npm sub-package (npm/* directories)
 *  - internal dependency refs (see modes below)
 *
 * Default (local dev): non-workspace internal deps → workspace:* (Yarn links siblings).
 * --pin-for-publish: all internal deps → exact version (npm publish; no workspace:).
 *
 * Usage:
 *   node scripts/sync-packages-version.mjs
 *   node scripts/sync-packages-version.mjs --pin-for-publish
 */

import { readFileSync, writeFileSync } from 'fs';
import { resolve, dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { readdirSync, statSync } from 'fs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

const PIN_FOR_PUBLISH = process.argv.includes('--pin-for-publish');
const WORKSPACE_PROTOCOL = 'workspace:*';

// Internal deps that live OUTSIDE this monorepo (sibling repo), referenced via a
// file: path in local dev. They are NOT workspace members, so bumpDeps() skips
// them. file: refs are invalid on npm, so --pin-for-publish must rewrite them to
// a registry semver; local dev reverts to the file: ref so Yarn links the sibling
// checkout. publishVersion tracks the version published from octocode-mcp-host.
const EXTERNAL_FILE_DEPS = {
  '@octocodeai/octocode-core': {
    publishVersion: '^16.1.1',
    localRef: 'file:../../../octocode-mcp-host/packages/octocode-core',
  },
};

// Internal workspace packages that are BUNDLED into their consumers' build
// output and never published to npm. They appear only in consumers'
// devDependencies as build-time workspace links, so they must stay on the
// workspace protocol even in --pin-for-publish mode: pinning them to a registry
// version would reference a package that does not exist on npm, and would break
// `yarn install` if anyone re-installed while pinned. npm auto-corrects the
// harmless leftover devDependency ref on publish; consumers never install it.
const UNPUBLISHED_INTERNAL = new Set(['@octocodeai/octocode-tools-core']);

function bumpExternalFileDeps(deps) {
  if (!deps) return false;
  let changed = false;
  for (const [name, cfg] of Object.entries(EXTERNAL_FILE_DEPS)) {
    if (!(name in deps)) continue;
    const target = PIN_FOR_PUBLISH ? cfg.publishVersion : cfg.localRef;
    if (deps[name] !== target) {
      deps[name] = target;
      changed = true;
    }
  }
  return changed;
}

// ── helpers ────────────────────────────────────────────────────────────────

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, 'utf8'));
}

function writeJson(filePath, data) {
  writeFileSync(filePath, JSON.stringify(data, null, 2) + '\n', 'utf8');
}

function bumpDeps(deps, version, internalPackageNames) {
  if (!deps) return false;
  let changed = false;
  for (const [name, val] of Object.entries(deps)) {
    if (!internalPackageNames.has(name)) continue;

    // Bundled-not-published packages stay on the workspace protocol in BOTH
    // modes — they are never pinned to a registry version (see above).
    if (UNPUBLISHED_INTERNAL.has(name)) {
      if (!String(val).startsWith('workspace:')) {
        deps[name] = WORKSPACE_PROTOCOL;
        changed = true;
      }
      continue;
    }

    if (PIN_FOR_PUBLISH) {
      if (val !== version) {
        deps[name] = version;
        changed = true;
      }
      continue;
    }

    if (!String(val).startsWith('workspace:')) {
      deps[name] = WORKSPACE_PROTOCOL;
      changed = true;
    }
  }
  return changed;
}

// ── discover paths ─────────────────────────────────────────────────────────

/** Main package roots — every direct packages/* package with a package.json. */
function findMainPackages() {
  const packagesDir = join(ROOT, 'packages');
  return readdirSync(packagesDir)
    .map(entry => join('packages', entry))
    .filter(pkgDir => {
      try {
        return statSync(join(ROOT, pkgDir)).isDirectory();
      } catch {
        return false;
      }
    })
    .filter(pkgDir => {
      try {
        readJson(join(ROOT, pkgDir, 'package.json'));
        return true;
      } catch {
        return false;
      }
    })
    .sort();
}

/** npm sub-package roots under each package's npm directory. */
function findNpmSubPackages() {
  const results = [];
  for (const pkg of MAIN_PACKAGES) {
    const npmDir = join(ROOT, pkg, 'npm');
    try {
      for (const entry of readdirSync(npmDir)) {
        const full = join(npmDir, entry);
        if (statSync(full).isDirectory()) {
          const pjPath = join(full, 'package.json');
          try {
            readJson(pjPath); // verify it exists and is valid
            results.push(pjPath);
          } catch {
            // skip
          }
        }
      }
    } catch {
      // npm dir doesn't exist yet
    }
  }
  return results.sort();
}

// ── main ───────────────────────────────────────────────────────────────────

const sourcePkgPath = join(ROOT, 'packages/octocode-mcp/package.json');
const { version } = readJson(sourcePkgPath);
const MAIN_PACKAGES = findMainPackages();

console.log(`\nSyncing all packages to version: ${version}`);
console.log(
  PIN_FOR_PUBLISH
    ? 'Mode: --pin-for-publish (internal deps → exact version)\n'
    : 'Mode: local dev (internal deps → workspace:*)\n'
);
console.log('Main packages:', MAIN_PACKAGES.join(', '));
console.log();

// Collect every internal package name so we can update pinned dep refs
const internalNames = new Set();
for (const pkgDir of MAIN_PACKAGES) {
  try {
    const { name } = readJson(join(ROOT, pkgDir, 'package.json'));
    if (name) internalNames.add(name);
  } catch {
    // package might not exist in this checkout
  }
}
const npmSubPaths = findNpmSubPackages();
for (const p of npmSubPaths) {
  const { name } = readJson(p);
  if (name) internalNames.add(name);
}

console.log('Internal package names:', [...internalNames].sort().join(', '));
console.log();

const updated = [];

// 1. Main packages
for (const pkgDir of MAIN_PACKAGES) {
  const pjPath = join(ROOT, pkgDir, 'package.json');
  let data;
  try {
    data = readJson(pjPath);
  } catch {
    console.warn(`  SKIP (not found): ${pjPath}`);
    continue;
  }

  let changed = data.version !== version;
  data.version = version;

  changed |= bumpDeps(data.dependencies, version, internalNames);
  changed |= bumpDeps(data.devDependencies, version, internalNames);
  changed |= bumpDeps(data.peerDependencies, version, internalNames);
  changed |= bumpDeps(data.optionalDependencies, version, internalNames);
  changed |= bumpExternalFileDeps(data.dependencies);

  writeJson(pjPath, data);
  console.log(`  ${changed ? '✓' : '~'} ${pkgDir}/package.json  (${data.name}@${version})`);
  updated.push(pjPath);
}

// 2. npm sub-packages
for (const pjPath of npmSubPaths) {
  let data;
  try {
    data = readJson(pjPath);
  } catch {
    console.warn(`  SKIP (not found): ${pjPath}`);
    continue;
  }

  let changed = data.version !== version;
  data.version = version;

  changed |= bumpDeps(data.dependencies, version, internalNames);
  changed |= bumpDeps(data.optionalDependencies, version, internalNames);

  writeJson(pjPath, data);
  const rel = pjPath.replace(ROOT + '/', '');
  console.log(`  ${changed ? '✓' : '~'} ${rel}  (${data.name}@${version})`);
  updated.push(pjPath);
}

console.log(`\nDone. Updated ${updated.length} package.json files.\n`);
