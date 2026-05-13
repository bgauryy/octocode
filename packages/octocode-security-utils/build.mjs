import * as esbuild from 'esbuild';
import { rm } from 'fs/promises';

await rm('dist', { recursive: true, force: true });

await esbuild.build({
  entryPoints: {
    index: 'src/index.ts',
    pathValidator: 'src/pathValidator.ts',
    commandValidator: 'src/commandValidator.ts',
    contentSanitizer: 'src/contentSanitizer.ts',
    withSecurityValidation: 'src/withSecurityValidation.ts',
    mask: 'src/mask.ts',
    ignoredPathFilter: 'src/ignoredPathFilter.ts',
    workspaceRoot: 'src/workspaceRoot.ts',
    executionContextValidator: 'src/executionContextValidator.ts',
    pathUtils: 'src/pathUtils.ts',
    types: 'src/types.ts',
    paramExtractors: 'src/paramExtractors.ts',
    registry: 'src/registry.ts',
    'regexes/index': 'src/regexes/index.ts',
  },
  bundle: true,
  platform: 'node',
  target: 'node18',
  format: 'esm',
  outdir: 'dist',
  packages: 'external',
  treeShaking: true,
  logLevel: 'info',
});

console.log('✓ esbuild complete');
