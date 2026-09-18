#!/usr/bin/env node
/**
 * Bundle the published type surface (dist/public.d.ts) into a single, dependency-
 * light declaration file.
 *
 * Why this exists: the MCP package publishes one stable declaration entry point.
 * rollup-plugin-dts resolves the local native-adapter declarations into the
 * published surface while preserving MCP SDK imports as dependencies.
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

const bundle = await rollup({
  input: entry,
  plugins: [dts({ respectExternal: true })],
  external: (id, _importer, isResolved) => {
    if (isResolved) return false; // already-resolved file path → bundle
    if (id.startsWith('.') || id.startsWith('/')) return false; // relative → bundle
    return true; // bare package specifier → external
  },
});

await bundle.write({ file: out, format: 'es' });
await bundle.close();

// The per-file declarations in dist/.types were scratch input for the rollup —
// drop them so the tarball ships only the bundled public.d.ts.
await rm(typesDir, { recursive: true, force: true });

console.log(
  '✓ bundled dist/public.d.ts (native adapter surface, MCP SDK external)'
);
