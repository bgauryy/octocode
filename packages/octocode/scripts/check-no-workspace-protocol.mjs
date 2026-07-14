#!/usr/bin/env node
/**
 * Publish guard for every published Octocode package.
 *
 * Fails before npm publish/prepack when:
 *   - a published dependency still uses workspace:
 *   - a published Octocode package version differs from the monorepo version
 *   - a published internal dependency points at a stale Octocode version
 *   - engine optional platform packages do not match the engine version
 *   - publish hooks are missing this guard
 */
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(join(scriptDir, '..'));
const repoRoot = resolve(join(packageRoot, '..', '..'));
const rootPkg = readJson(join(repoRoot, 'package.json'));
const rootVersion = rootPkg.version;

const PUBLISHED_PACKAGE_DIRS = [
  'packages/octocode-config',
  'packages/octocode-tools-core',
  'packages/octocode-mcp',
  'packages/octocode-skills',
  'packages/octocode-engine',
  'packages/octocode',
];

const PUBLISHED_DEP_FIELDS = [
  'dependencies',
  'optionalDependencies',
  'peerDependencies',
  'bundledDependencies',
  'bundleDependencies',
];

const INTERNAL_VERSIONED_PACKAGES = new Set([
  '@octocodeai/config',
  '@octocodeai/octocode-engine',
  '@octocodeai/octocode-tools-core',
  '@octocodeai/mcp',
  '@octocodeai/skills',
  'octocode',
]);

const GUARD_SCRIPT = 'check-no-workspace-protocol.mjs';
const ENGINE_DIR = join(repoRoot, 'packages/octocode-engine');
const ENGINE_NPM_DIR = join(ENGINE_DIR, 'npm');
const offenders = [];
const checkedPackages = [];

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function rel(path) {
  return relative(repoRoot, path).replaceAll('\\', '/');
}

function stripRange(spec) {
  return String(spec).replace(/^[\^~>=<\s]+/, '').trim();
}

function fail(path, message) {
  offenders.push(`  ${rel(path)}: ${message}`);
}

function checkPublishedDeps(packagePath, pkg) {
  for (const field of PUBLISHED_DEP_FIELDS) {
    const deps = pkg[field];
    if (!deps || typeof deps !== 'object') continue;

    for (const [name, spec] of Object.entries(deps)) {
      if (typeof spec === 'string' && spec.startsWith('workspace:')) {
        fail(packagePath, `${field}.${name}: "${spec}" (workspace: must not ship to npm)`);
        continue;
      }

      if (INTERNAL_VERSIONED_PACKAGES.has(name) && stripRange(spec) !== rootVersion) {
        fail(packagePath, `${field}.${name}: "${spec}" (expected ${rootVersion})`);
      }
    }
  }
}

function checkPackageVersion(packagePath, pkg, expectedVersion) {
  if (pkg.version !== expectedVersion) {
    fail(packagePath, `version "${pkg.version}" (expected ${expectedVersion})`);
  }
}

function checkPublishHook(packagePath, pkg) {
  const prepublishOnly = pkg.scripts?.prepublishOnly;
  if (typeof prepublishOnly !== 'string' || !prepublishOnly.includes(GUARD_SCRIPT)) {
    fail(packagePath, `scripts.prepublishOnly must run ${GUARD_SCRIPT}`);
  }
}

function checkPackage(packageDir) {
  const packagePath = join(repoRoot, packageDir, 'package.json');
  if (!existsSync(packagePath)) {
    fail(packagePath, 'package.json is missing');
    return null;
  }

  const pkg = readJson(packagePath);
  checkedPackages.push(`${pkg.name}@${pkg.version}`);
  checkPackageVersion(packagePath, pkg, rootVersion);
  checkPublishedDeps(packagePath, pkg);
  checkPublishHook(packagePath, pkg);
  return { packagePath, pkg };
}

function checkEnginePlatforms(enginePkg) {
  const engineVersion = enginePkg.version;
  const optionalDependencies = enginePkg.optionalDependencies ?? {};

  if (!existsSync(ENGINE_NPM_DIR)) {
    fail(ENGINE_NPM_DIR, 'engine npm platform directory is missing');
    return;
  }

  const platformDirs = readdirSync(ENGINE_NPM_DIR, { withFileTypes: true }).filter((entry) =>
    entry.isDirectory()
  );

  for (const dir of platformDirs) {
    const platformPath = join(ENGINE_NPM_DIR, dir.name, 'package.json');
    if (!existsSync(platformPath)) {
      fail(platformPath, 'platform package.json is missing');
      continue;
    }

    const platformPkg = readJson(platformPath);
    checkedPackages.push(`${platformPkg.name}@${platformPkg.version}`);
    checkPackageVersion(platformPath, platformPkg, engineVersion);
    checkPublishedDeps(platformPath, platformPkg);
    checkPublishHook(platformPath, platformPkg);

    const optionalSpec = optionalDependencies[platformPkg.name];
    if (!optionalSpec) {
      fail(join(ENGINE_DIR, 'package.json'), `optionalDependencies.${platformPkg.name} is missing`);
    } else if (stripRange(optionalSpec) !== engineVersion) {
      fail(
        join(ENGINE_DIR, 'package.json'),
        `optionalDependencies.${platformPkg.name}: "${optionalSpec}" (expected ${engineVersion})`
      );
    }
  }

  for (const name of Object.keys(optionalDependencies)) {
    if (!name.startsWith('@octocodeai/octocode-engine-')) continue;
    const found = platformDirs.some((dir) => {
      const platformPath = join(ENGINE_NPM_DIR, dir.name, 'package.json');
      return existsSync(platformPath) && readJson(platformPath).name === name;
    });
    if (!found) {
      fail(join(ENGINE_DIR, 'package.json'), `optionalDependencies.${name} has no npm platform package`);
    }
  }
}

if (!rootVersion) {
  console.error(`\n✗ ${rel(join(repoRoot, 'package.json'))}: version is missing.\n`);
  process.exit(1);
}

let enginePackage = null;
for (const packageDir of PUBLISHED_PACKAGE_DIRS) {
  const result = checkPackage(packageDir);
  if (packageDir === 'packages/octocode-engine') {
    enginePackage = result?.pkg ?? null;
  }
}

if (enginePackage) {
  checkEnginePlatforms(enginePackage);
}

if (offenders.length > 0) {
  console.error(
    `\n✗ Octocode publish guard failed (expected version ${rootVersion}).\n\n` +
      offenders.join('\n') +
      `\n\n  Run the package version sync/prepublish step, update stale package.json files, and retry.\n`
  );
  process.exit(1);
}

console.log(
  `✓ Octocode publish guard passed for ${checkedPackages.length} package(s): version ${rootVersion}, no workspace: published deps, engine platforms aligned.`
);
