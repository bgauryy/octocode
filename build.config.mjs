/**
 * Shared esbuild configuration for all workspace packages.
 *
 * Import in package build scripts:
 *   import { baseOptions, nodeExternals } from '../../build.config.mjs';
 */
import { builtinModules } from 'node:module';

export const nodeExternals = [
  ...builtinModules,
  ...builtinModules.map((n) => `node:${n}`),
];

/**
 * Base esbuild options for Node 22 ESM bundles.
 * Each package extends / overrides as needed (entryPoints, outdir, external, …).
 */
export const baseOptions = {
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node22',
  external: nodeExternals,
  sourcemap: false,
  treeShaking: true,
  logLevel: 'info',
};
