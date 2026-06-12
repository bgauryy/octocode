import * as esbuild from 'esbuild';
import { rm } from 'fs/promises';

await rm('dist', { recursive: true, force: true });

await esbuild.build({
  entryPoints: {
    index:                   'src/index.ts',
    native:                  'src/native.ts',
    types:                   'src/types.ts',
    contentSanitizer:        'src/contentSanitizer.ts',
    mask:                    'src/mask.ts',
    pathValidator:           'src/pathValidator.ts',
    commandValidator:        'src/commandValidator.ts',
    withSecurityValidation:  'src/withSecurityValidation.ts',
    ignoredPathFilter:       'src/ignoredPathFilter.ts',
    pathUtils:               'src/pathUtils.ts',
    paramExtractors:         'src/paramExtractors.ts',
    registry:                'src/registry.ts',
    securityConstants:       'src/securityConstants.ts',
    pathPatterns:            'src/pathPatterns.ts',
    filePatterns:            'src/filePatterns.ts',
    'regexes/index':         'src/regexes/index.ts',
  },
  bundle: true,
  platform: 'node',
  target: 'node20',
  format: 'esm',
  outdir: 'dist',
  packages: 'external',
  // Keep .node require() calls as-is so the binary loads at runtime
  external: ['*.node'],
  treeShaking: true,
  logLevel: 'info',
});

console.log('✓ esbuild complete');
