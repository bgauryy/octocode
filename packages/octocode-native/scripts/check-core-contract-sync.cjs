'use strict';
// Release gate: the contracts embedded in the native runtime must match the
// @octocodeai/octocode-core this workspace resolves. A clean `npm install`
// otherwise ships a binary whose compiled contracts diverge from the core
// package the interfaces import at runtime.
//
// Usage: node scripts/check-core-contract-sync.cjs

const { existsSync, readFileSync } = require('node:fs');
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

async function main() {
  // Resolved from this workspace exactly like runtime consumers resolve it.
  // The builder is not on the package's public exports map, so import the
  // dist file directly.
  const coreDir = join(nativeRoot, '..', '..', 'node_modules', '@octocodeai', 'octocode-core');
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
    console.error(`  resolved core computes:   ${current}`);
    console.error(
      '  The resolved @octocodeai/octocode-core does not match the contracts compiled into the runtime.'
    );
    console.error(
      '  Fix: `yarn contracts:regen` against the intended core, or align the core dependency version.'
    );
    process.exit(1);
  }
  console.log(
    `contract-sync: OK — core ${provenance.sourceRevision.slice(0, 8)} fingerprint ${current.slice(0, 12)}… matches the embedded contracts.`
  );
}

main().catch((error) => {
  console.error('contract-sync: failed:', error.message);
  process.exit(1);
});
