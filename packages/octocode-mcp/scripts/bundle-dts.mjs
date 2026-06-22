#!/usr/bin/env node
/**
 * Bundle the published type surface (dist/public.d.ts) into a single, dependency-
 * light declaration file.
 *
 * Why this exists: @octocodeai/octocode-tools-core is INLINED into the JS bundle
 * and is NOT published to npm (it is a devDependency). tsc, however, emits
 * `export { … } from '@octocodeai/octocode-tools-core'` into the declarations —
 * a dangling reference for any TypeScript consumer, since the package won't be
 * installed. rollup-plugin-dts resolves those tools-core types and inlines them,
 * while keeping still-published packages (@octocodeai/octocode-core, zod) as
 * normal external `import`s.
 *
 * Flow (driven by the `build:types` script):
 *   1. tsc --emitDeclarationOnly --outDir dist/.types   (per-file .d.ts in a temp dir)
 *   2. this script rolls dist/.types/public.d.ts → dist/public.d.ts
 *   3. the temp dir is removed; only the bundled public.d.ts ships (see `files`/`exports`)
 */
import { rollup } from 'rollup';
import dts from 'rollup-plugin-dts';
import { rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkgRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const distDir = join(pkgRoot, 'dist');
const typesDir = join(distDir, '.types');
const entry = join(typesDir, 'public.d.ts');
const out = join(distDir, 'public.d.ts');

// Inline ONLY @octocodeai/octocode-tools-core (it isn't published); keep every
// other bare specifier — @octocodeai/octocode-core, zod, octokit/@octokit/*,
// the native engine, node builtins — as an external `import`. They are all real
// runtime dependencies of octocode-mcp, so consumers resolve them from npm.
const INLINE = /^@octocodeai\/octocode-tools-core(\/.*)?$/;

const bundle = await rollup({
  input: entry,
  plugins: [dts({ respectExternal: true })],
  external: (id, _importer, isResolved) => {
    if (isResolved) return false; // already-resolved file path → bundle
    if (id.startsWith('.') || id.startsWith('/')) return false; // relative → bundle
    if (INLINE.test(id)) return false; // tools-core → follow & inline
    return true; // any other bare specifier → external
  },
});

await bundle.write({ file: out, format: 'es' });
await bundle.close();

// The per-file declarations in dist/.types were scratch input for the rollup —
// drop them so the tarball ships only the bundled public.d.ts.
await rm(typesDir, { recursive: true, force: true });

console.log('✓ bundled dist/public.d.ts (tools-core types inlined, octocode-core external)');
