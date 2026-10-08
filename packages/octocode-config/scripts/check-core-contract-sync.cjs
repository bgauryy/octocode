'use strict';
// Release gate: the tool contract octocode-config generated (and that
// octocode-native embeds from contract/ at build time) must match the
// @octocodeai/octocode-core the packages depend on.
//
// Usage: node scripts/check-core-contract-sync.cjs
//
// Downloads the npm-published core version pinned in the workspace root
// package.json and checks contract/ against THAT — what a clean install of the
// released packages delivers. (Freshness against the workspace core is
// `check:tool-contract`.) Run it before publishing native.

const { execFileSync } = require('node:child_process');
const { readFileSync, mkdtempSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const { pathToFileURL } = require('node:url');

const configRoot = resolve(__dirname, '..');
const repoRoot = resolve(configRoot, '..', '..');
const provenance = JSON.parse(
  readFileSync(join(configRoot, 'contract', 'provenance.json'), 'utf8')
);

/// Download the npm-published core at the version config ships against (its
/// own dependency pin; the root has only dev resolutions) and return the
/// extracted package directory (caller removes tempRoot).
function fetchPublishedCore(tempRoot) {
  const pin = (file, field) => {
    const pkg = JSON.parse(readFileSync(file, 'utf8'));
    return pkg[field] && pkg[field]['@octocodeai/octocode-core'];
  };
  const version =
    pin(join(configRoot, 'package.json'), 'dependencies') ||
    pin(join(repoRoot, 'package.json'), 'resolutions');
  if (!version) {
    console.error('contract-sync: no @octocodeai/octocode-core version pinned in packages/octocode-config/package.json dependencies.');
    process.exit(1);
  }
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(
      'Published contract verification requires an exact npm version for @octocodeai/octocode-core; local paths, protocols, tags, and ranges cannot prove release parity.'
    );
  }
  const spec = `@octocodeai/octocode-core@${version}`;
  console.log(`contract-sync: checking against published ${spec} …`);
  // A full install (not `npm pack`) so the published dist can import its own
  // runtime dependencies (zod, …) when we load the contract builder.
  execFileSync(
    'npm',
    ['install', spec, '--prefix', tempRoot, '--ignore-scripts', '--no-save', '--no-audit', '--no-fund', '--silent'],
    { encoding: 'utf8' }
  );
  return {
    dir: join(tempRoot, 'node_modules', '@octocodeai', 'octocode-core'),
    spec,
  };
}

async function checkAgainst(coreDir, label) {
  const core = await import(pathToFileURL(join(coreDir, 'dist', 'schema.js')).href);
  const current = core.buildEnforcementContractIr().fingerprint;
  if (current !== provenance.contractFingerprint) {
    console.error('contract-sync: FINGERPRINT MISMATCH');
    console.error(`  octocode-config contract/: ${provenance.contractFingerprint}`);
    console.error(`  ${label} computes:        ${current}`);
    console.error(
      '  Fix: `yarn contracts:regen` against the intended core, align the core dependency version, or publish the matching core first.'
    );
    process.exit(1);
  }
  console.log(
    `contract-sync: OK — ${label} fingerprint ${current.slice(0, 12)}… matches octocode-config contract/ (core ${provenance.sourceVersion}).`
  );
}

async function main() {
  const tempRoot = mkdtempSync(join(tmpdir(), 'octocode-core-sync-'));
  try {
    const { dir, spec } = fetchPublishedCore(tempRoot);
    await checkAgainst(dir, `published ${spec}`);
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error('contract-sync: failed:', error.message);
  process.exit(1);
});
