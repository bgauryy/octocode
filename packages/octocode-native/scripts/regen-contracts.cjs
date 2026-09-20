'use strict';
// One-command contract regeneration:
//   core clean-check → core build → generator → pin-literal update → contract tests.
// Replaces the manual ritual (commit core, build, generate, harvest the new
// digest from a failing pin test, hand-edit contracts/mod.rs) that concurrent
// sessions repeatedly raced on.
//
// Usage: node scripts/regen-contracts.cjs [--allow-dirty]
//   OCTOCODE_CORE_DIR overrides sibling-core discovery.

const { execFileSync } = require('node:child_process');
const { existsSync, readFileSync, realpathSync, writeFileSync } = require('node:fs');
const { join, resolve } = require('node:path');

const nativeRoot = resolve(__dirname, '..');
const generatedDir = join(nativeRoot, 'crates', 'runtime', 'src', 'contracts', 'generated');
const pinnedFile = join(nativeRoot, 'crates', 'runtime', 'src', 'contracts', 'mod.rs');
const allowDirty = process.argv.includes('--allow-dirty');

function run(command, args, options = {}) {
  return execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], ...options });
}

function corePackageDir() {
  const explicit = process.env.OCTOCODE_CORE_DIR;
  if (explicit) {
    if (!existsSync(join(explicit, 'scripts', 'generate-native-contracts.ts'))) {
      throw new Error(`OCTOCODE_CORE_DIR does not look like octocode-core: ${explicit}`);
    }
    return realpathSync(explicit);
  }
  const linked = join(nativeRoot, '..', '..', 'node_modules', '@octocodeai', 'octocode-core');
  if (!existsSync(linked)) {
    throw new Error('Cannot locate @octocodeai/octocode-core; set OCTOCODE_CORE_DIR.');
  }
  return realpathSync(linked);
}

const coreDir = corePackageDir();
const coreRepo = resolve(coreDir, '..', '..');
console.log(`core: ${coreDir}`);

const dirty = run('git', ['status', '--porcelain', '--untracked-files=all', '--', '.'], { cwd: coreDir }).trim();
if (dirty && !allowDirty) {
  console.error('Refusing to regenerate from a dirty octocode-core checkout (commit first, or pass --allow-dirty for local iteration):');
  console.error(dirty);
  process.exit(2);
}

console.log('building core…');
run('yarn', ['workspace', '@octocodeai/octocode-core', 'build'], { cwd: coreRepo });

console.log('generating contracts…');
const generatorArgs = [join(coreDir, 'scripts', 'generate-native-contracts.ts'), '--out', generatedDir];
if (allowDirty) generatorArgs.push('--allow-dirty');
run('node', generatorArgs, { cwd: coreRepo });

// The body-hash pin must move together with every legitimate regeneration.
// Harvest the new digest from the pin test itself so the computation can
// never drift from what the test asserts.
console.log('updating body-hash pin…');
const cargoTest = (filter) => {
  try {
    return {
      ok: true,
      out: run('cargo', ['test', '-p', 'octocode-native', '--no-default-features', '--lib', filter], { cwd: nativeRoot }),
    };
  } catch (error) {
    return { ok: false, out: `${error.stdout ?? ''}${error.stderr ?? ''}` };
  }
};

let pin = cargoTest('generated_contract_body_hash_is_pinned_against_hand_edits');
if (!pin.ok) {
  const fresh = pin.out.match(/left: "([0-9a-f]{64})"/)?.[1];
  const stale = pin.out.match(/right: "([0-9a-f]{64})"/)?.[1];
  if (!fresh || !stale) {
    console.error(pin.out);
    throw new Error('Pin test failed but no digest pair was found in its output.');
  }
  const source = readFileSync(pinnedFile, 'utf8');
  if (!source.includes(stale)) {
    throw new Error(`Pinned digest ${stale} not found in ${pinnedFile}.`);
  }
  writeFileSync(pinnedFile, source.replace(stale, fresh));
  console.log(`pin: ${stale.slice(0, 12)}… → ${fresh.slice(0, 12)}…`);
  pin = cargoTest('generated_contract_body_hash_is_pinned_against_hand_edits');
  if (!pin.ok) {
    console.error(pin.out);
    throw new Error('Pin test still failing after digest update.');
  }
} else {
  console.log('pin: unchanged');
}

console.log('running contract tests…');
const contracts = cargoTest('contracts');
if (!contracts.ok) {
  console.error(contracts.out);
  throw new Error('Contract tests failed after regeneration.');
}
const provenance = JSON.parse(readFileSync(join(generatedDir, 'contract-provenance.json'), 'utf8'));
console.log(`done: revision ${provenance.sourceRevision.slice(0, 8)} dirty=${provenance.sourceDirty} fingerprint=${provenance.contractFingerprint.slice(0, 12)}…`);
if (provenance.sourceDirty) {
  console.warn('WARNING: contracts were generated from a dirty core tree (--allow-dirty); the provenance test will fail until a clean regen.');
}
