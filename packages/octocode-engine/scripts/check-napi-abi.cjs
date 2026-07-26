#!/usr/bin/env node
/**
 * NAPI ABI drift guard.
 *
 * `loader/index.d.ts` is the HAND-MAINTAINED type surface for the native addon;
 * `napi build` generates the real ABI types, which postbuild.cjs snapshots to
 * `.napi-abi-snapshot.d.ts` before restoring the hand loader. Nothing otherwise
 * checks that the hand types still match the real ABI — a Rust `#[napi]` rename,
 * added/removed export, or changed arity drifts silently (the same failure class
 * as the patterns.rs and get_file_config drifts).
 *
 * This compares the CALLABLE surface — exported functions, classes + their
 * methods, and consts — between the snapshot and the hand loader. It deliberately
 * ignores interface/type-alias/enum SHAPES, because the hand loader intentionally
 * narrows napi's `any`/`string` (serialized-JSON) returns to richer typed shapes.
 * Return types are not compared for the same reason; names + kind + arity are.
 *
 * Exit 0 = surfaces match. Exit 1 = drift (fail the build). Exit 2 = snapshot
 * missing (run `yarn build:dev` first).
 */
'use strict';

const { readFileSync, existsSync } = require('fs');
const { join } = require('path');
const ts = require('typescript');

const root = join(__dirname, '..');
const SNAPSHOT = join(root, '.napi-abi-snapshot.d.ts');
const LOADER = join(root, 'loader', 'index.d.ts');

// Symbols the hand loader legitimately declares that napi does NOT export as
// callables (e.g. consts synthesized by the CJS loader, not the Rust ABI).
// Anything here is exempt from the "declared in loader but not in the ABI" check.
// Keep this list tight — each entry is a hand-maintained exception.
// These consts are SYNTHESIZED by loader/index.cjs (120-129) from native
// getters (e.g. MINIFY_CONFIG = getMINIFY_CONFIG()), so they appear in the hand
// loader but not as direct napi callables — the getters themselves ARE checked.
const LOADER_ONLY_ALLOWLIST = new Set([
  'MINIFY_CONFIG',
  'SUPPORTED_SIGNATURE_EXTENSIONS',
  'SUPPORTED_GRAPH_FACT_EXTENSIONS',
  'SUPPORTED_STRUCTURAL_EXTENSIONS',
]);

/** Extract the callable ABI surface as a map of `kind:path` -> arity|null. */
function extractSurface(filePath) {
  const src = ts.createSourceFile(
    filePath,
    readFileSync(filePath, 'utf8'),
    ts.ScriptTarget.Latest,
    /* setParentNodes */ true
  );
  const surface = new Map();
  const isExported = node =>
    (ts.getCombinedModifierFlags(node) & ts.ModifierFlags.Export) !== 0;
  const arity = params => (params ? params.length : 0);

  for (const node of src.statements) {
    if (ts.isFunctionDeclaration(node) && node.name && isExported(node)) {
      surface.set(`function:${node.name.text}`, arity(node.parameters));
    } else if (ts.isClassDeclaration(node) && node.name && isExported(node)) {
      const cls = node.name.text;
      surface.set(`class:${cls}`, null);
      for (const member of node.members) {
        if (
          (ts.isMethodDeclaration(member) ||
            ts.isConstructorDeclaration(member)) &&
          member.name
        ) {
          const m = ts.isConstructorDeclaration(member)
            ? 'constructor'
            : member.name.getText(src);
          surface.set(`method:${cls}.${m}`, arity(member.parameters));
        }
      }
    } else if (ts.isVariableStatement(node) && isExported(node)) {
      for (const decl of node.declarationList.declarations) {
        if (ts.isIdentifier(decl.name)) {
          surface.set(`const:${decl.name.text}`, null);
        }
      }
    }
  }
  return surface;
}

function main() {
  if (!existsSync(SNAPSHOT)) {
    console.error(
      `check-napi-abi: no ABI snapshot at ${SNAPSHOT}.\n` +
        `Run \`yarn build:dev\` (or \`yarn build\`) first — postbuild.cjs writes it.`
    );
    process.exit(2);
  }
  if (!existsSync(LOADER)) {
    console.error(`check-napi-abi: missing ${LOADER}`);
    process.exit(2);
  }

  const napi = extractSurface(SNAPSHOT);
  const loader = extractSurface(LOADER);

  const missingFromLoader = []; // real ABI symbol the hand types forgot
  const arityMismatch = [];
  for (const [key, ar] of napi) {
    if (!loader.has(key)) {
      missingFromLoader.push(key);
    } else if (ar !== null && loader.get(key) !== ar) {
      arityMismatch.push(`${key} (abi ${ar} params, loader ${loader.get(key)})`);
    }
  }

  const staleInLoader = []; // hand type declares a callable the ABI doesn't have
  for (const key of loader.keys()) {
    const name = key.split(':')[1].split('.')[0];
    if (!napi.has(key) && !LOADER_ONLY_ALLOWLIST.has(name)) {
      staleInLoader.push(key);
    }
  }

  const problems =
    missingFromLoader.length + arityMismatch.length + staleInLoader.length;
  if (problems === 0) {
    console.log(
      `check-napi-abi: OK — ${napi.size} ABI callables match loader/index.d.ts.`
    );
    process.exit(0);
  }

  console.error('check-napi-abi: NAPI ABI ↔ loader/index.d.ts DRIFT detected.\n');
  if (missingFromLoader.length) {
    console.error(
      'Exported by the native ABI but MISSING from loader/index.d.ts (add them):'
    );
    for (const k of missingFromLoader.sort()) console.error(`  - ${k}`);
    console.error('');
  }
  if (staleInLoader.length) {
    console.error(
      'Declared in loader/index.d.ts but NOT in the native ABI (stale — remove or fix):'
    );
    for (const k of staleInLoader.sort()) console.error(`  - ${k}`);
    console.error('');
  }
  if (arityMismatch.length) {
    console.error('Parameter-count mismatch:');
    for (const k of arityMismatch.sort()) console.error(`  - ${k}`);
    console.error('');
  }
  console.error(
    'Fix loader/index.d.ts (and any hand wrapper in src/*/native.ts) to match the ABI, ' +
      'or add a justified exception to LOADER_ONLY_ALLOWLIST in this script.'
  );
  process.exit(1);
}

main();
