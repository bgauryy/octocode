import { builtinModules } from 'node:module';

export const nodeExternals = [...builtinModules, ...builtinModules.map((name) => `node:${name}`)];
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
