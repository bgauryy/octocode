#!/usr/bin/env node
/**
 * Publish guard for every published Octocode package.
 *
 * Single responsibility: no published package may ship a LOCAL dependency.
 *
 * Fails before npm publish/prepack when any published dependency field
 * (dependencies, optionalDependencies, peerDependencies, bundled[D]ependencies)
 * references a local protocol — workspace:, file:, link:, or portal: — because
 * those never resolve from the npm registry for consumers.
 *
 * devDependencies are intentionally NOT checked: they are never installed by
 * consumers, so a workspace:* devDependency is valid and normal for local dev.
 *
 * This guard does NOT enforce version alignment between packages — each package
 * is versioned independently. If you want to sync every package version to the
 * monorepo root, run `node ./scripts/prepublish.mjs --fix` explicitly.
 */
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(join(scriptDir, '..'));
const repoRoot = resolve(join(packageRoot, '..', '..'));

const PUBLISHED_PACKAGE_DIRS = [
  'packages/octocode-config',
  'packages/octocode-tools-core',
  'packages/octocode-mcp',
  'packages/octocode-engine',
  'packages/octocode',
];

/** Dependency fields that ship inside a published tarball. */
const PUBLISHED_DEP_FIELDS = [
  'dependencies',
  'optionalDependencies',
  'peerDependencies',
  'bundledDependencies',
  'bundleDependencies',
];

/** Local dependency protocols that must never ship to npm. */
const LOCAL_PROTOCOLS = ['workspace:', 'file:', 'link:', 'portal:'];

const ENGINE_NPM_DIR = join(repoRoot, 'packages/octocode-engine/npm');
const offenders = [];
const checkedPackages = [];

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function rel(path) {
  return relative(repoRoot, path).replaceAll('\\', '/');
}

function fail(path, message) {
  offenders.push(`  ${rel(path)}: ${message}`);
}

/** Return the local protocol prefix a spec uses, or undefined if it's a normal registry spec. */
function localProtocol(spec) {
  return typeof spec === 'string' ? LOCAL_PROTOCOLS.find((proto) => spec.startsWith(proto)) : undefined;
}

function checkPublishedDeps(packagePath, pkg) {
  for (const field of PUBLISHED_DEP_FIELDS) {
    const deps = pkg[field];
    if (!deps || typeof deps !== 'object') continue;

    for (const [name, spec] of Object.entries(deps)) {
      const proto = localProtocol(spec);
      if (proto) {
        fail(packagePath, `${field}.${name}: "${spec}" (${proto} local dep must not ship to npm)`);
      }
    }
  }
}

function checkPackage(packagePath) {
  if (!existsSync(packagePath)) {
    fail(packagePath, 'package.json is missing');
    return;
  }

  const pkg = readJson(packagePath);
  checkedPackages.push(`${pkg.name}@${pkg.version}`);
  checkPublishedDeps(packagePath, pkg);
}

// Published packages.
for (const packageDir of PUBLISHED_PACKAGE_DIRS) {
  checkPackage(join(repoRoot, packageDir, 'package.json'));
}

// Engine optional platform packages (packages/octocode-engine/npm/*).
if (existsSync(ENGINE_NPM_DIR)) {
  for (const entry of readdirSync(ENGINE_NPM_DIR, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    checkPackage(join(ENGINE_NPM_DIR, entry.name, 'package.json'));
  }
}

if (offenders.length > 0) {
  console.error(
    `\n✗ Octocode publish guard failed — local dependencies must not ship to npm.\n\n` +
      offenders.join('\n') +
      `\n\n  Replace local (${LOCAL_PROTOCOLS.join(' / ')}) specs in published dependency fields ` +
      `with real published version ranges, then retry.\n`
  );
  process.exit(1);
}

console.log(
  `✓ Octocode publish guard passed for ${checkedPackages.length} package(s): ` +
    `no local (${LOCAL_PROTOCOLS.join(' / ')}) deps in published dependency fields.`
);
