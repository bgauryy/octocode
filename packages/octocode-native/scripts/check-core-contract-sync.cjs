'use strict';
// Release gate: the contracts embedded in the native runtime must match the
// @octocodeai/octocode-core this workspace resolves. A clean `npm install`
// otherwise ships a binary whose compiled contracts diverge from the core
// package the interfaces import at runtime.
//
// Usage: node scripts/check-core-contract-sync.cjs [--published]
//
// Default mode resolves core from node_modules (in this workspace that is a
// dev symlink to the core checkout). `--published` instead downloads the
// npm-published version pinned in the workspace root package.json and checks
// the embedded contracts against THAT — what a clean `npm install` of the
// released packages actually delivers. Publishing runs the published mode
// (see prepublishOnly); it blocks publish only, never dev.

const { execFileSync } = require('node:child_process');
const { existsSync, readFileSync, mkdtempSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const { pathToFileURL } = require('node:url');

const nativeRoot = resolve(__dirname, '..');
const provenancePath = join(
  nativeRoot,
  'crates',
  'runtime',
  'src',
  'contracts',
  'generated',
  'contract-provenance.json'
);
const provenance = JSON.parse(readFileSync(provenancePath, 'utf8'));

if (provenance.sourceDirty) {
  console.error(
    `contract-sync: embedded contracts were generated from a DIRTY core checkout (${provenance.sourceRevision.slice(0, 8)}). Run \`yarn contracts:regen\` from a committed core.`
  );
  process.exit(1);
}

/// Download the npm-published core at the version the workspace root pins
/// and return the extracted package directory (caller removes tempRoot).
function fetchPublishedCore(tempRoot) {
  const rootPkg = JSON.parse(
    readFileSync(join(nativeRoot, '..', '..', 'package.json'), 'utf8')
  );
  const version =
    (rootPkg.dependencies && rootPkg.dependencies['@octocodeai/octocode-core']) ||
    (rootPkg.resolutions && rootPkg.resolutions['@octocodeai/octocode-core']);
  if (!version) {
    console.error('contract-sync: no @octocodeai/octocode-core version pinned in root package.json.');
    process.exit(1);
  }
  const spec = `@octocodeai/octocode-core@${version}`;
  console.log(`contract-sync: checking against published ${spec} …`);
  // A full install (not `npm pack`) so the published dist can import its own
  // runtime dependencies (zod, …) when we load the contract builder.
  execFileSync(
    'npm',
    ['install', spec, '--prefix', tempRoot, '--no-save', '--no-audit', '--no-fund', '--silent'],
    { encoding: 'utf8' }
  );
  return {
    dir: join(tempRoot, 'node_modules', '@octocodeai', 'octocode-core'),
    spec,
  };
}

async function checkAgainst(coreDir, label) {
  // The builder is not on the package's public exports map, so import the
  // dist file directly.
  const builderPath = join(coreDir, 'dist', 'toolContract', 'nativeContract.js');
  if (!existsSync(builderPath)) {
    console.error(`contract-sync: core dist not found at ${builderPath} (is core installed and built?).`);
    process.exit(1);
  }
  const core = await import(pathToFileURL(builderPath).href);
  const current = core.buildNativeContractIr().fingerprint;
  if (current !== provenance.contractFingerprint) {
    console.error('contract-sync: FINGERPRINT MISMATCH');
    console.error(`  embedded (native binary): ${provenance.contractFingerprint}`);
    console.error(`  ${label} computes: ${current}`);
    console.error(
      `  The ${label} does not match the contracts compiled into the runtime.`
    );
    console.error(
      '  Fix: `yarn contracts:regen` against the intended core, align the core dependency version, or publish the matching core first.'
    );
    process.exit(1);
  }
  console.log(
    `contract-sync: OK — core ${provenance.sourceRevision.slice(0, 8)} fingerprint ${current.slice(0, 12)}… matches the embedded contracts (${label}).`
  );
}

async function main() {
  if (process.argv.includes('--published')) {
    const tempRoot = mkdtempSync(join(tmpdir(), 'octocode-core-sync-'));
    try {
      const { dir, spec } = fetchPublishedCore(tempRoot);
      await checkAgainst(dir, `published ${spec}`);
    } finally {
      rmSync(tempRoot, { recursive: true, force: true });
    }
    return;
  }
  // Resolved from this workspace exactly like runtime consumers resolve it
  // (in this repo, a dev symlink to the core checkout).
  const coreDir = join(nativeRoot, '..', '..', 'node_modules', '@octocodeai', 'octocode-core');
  await checkAgainst(coreDir, 'resolved core');
}

main().catch((error) => {
  console.error('contract-sync: failed:', error.message);
  process.exit(1);
});
