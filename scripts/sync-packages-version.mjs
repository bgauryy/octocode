#!/usr/bin/env node

/**
 * Syncs the version of all packages in the monorepo to match
 * packages/octocode-mcp/package.json.
 *
 * Updates:
 *  - version field in every main package
 *  - version field in every npm sub-package (npm/* directories)
 *  - pinned internal dependency versions (optionalDependencies referencing
 *    sibling native packages, and non-workspace deps on internal packages)
 *
 * Workspace protocol references (workspace:* / workspace:^) are left unchanged.
 */

import { readFileSync, writeFileSync } from 'fs';
import { resolve, dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { readdirSync, statSync } from 'fs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

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
    if (internalPackageNames.has(name) && !String(val).startsWith('workspace:')) {
      deps[name] = version;
      changed = true;
    }
  }
  return changed;
}

// ── discover paths ─────────────────────────────────────────────────────────

/** Main package directories (have src/ or are explicit packages) */
const MAIN_PACKAGES = [
  'packages/octocode-mcp',
  'packages/octocode-cli',
  'packages/octocode-lsp',
  'packages/octocode-security',
  'packages/octocode-shared',
  'packages/octocode-vscode',
  'packages/octocode-context-utils',
];

/** npm sub-package roots — discovered automatically */
function findNpmSubPackages() {
  const results = [];
  for (const pkg of ['packages/octocode-context-utils', 'packages/octocode-security']) {
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
  return results;
}

// ── main ───────────────────────────────────────────────────────────────────

const sourcePkgPath = join(ROOT, 'packages/octocode-mcp/package.json');
const { version } = readJson(sourcePkgPath);

console.log(`\nSyncing all packages to version: ${version}\n`);

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
