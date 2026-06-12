#!/usr/bin/env node
/**
 * gen-tests.mjs
 *
 * Adapts all test files from octocode-security-utils into octocode-security/tests/,
 * rewrites imports where necessary, installs vitest config, and patches package.json.
 *
 * rust-specific.test.ts is written by a separate script (write-rust-tests.mjs)
 * so its template literals don't conflict here.
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const PKG_ROOT    = join(__dirname, '..');
const UTILS_TESTS = join(PKG_ROOT, '..', 'octocode-security-utils', 'tests');
const OUT_TESTS   = join(PKG_ROOT, 'tests');

mkdirSync(OUT_TESTS, { recursive: true });

// ---------------------------------------------------------------------------
// Files to copy + transform
// ---------------------------------------------------------------------------
const FILES_TO_ADAPT = [
  'setup.ts',
  'contentSanitizer.test.ts',
  'coverage-gaps.test.ts',
  'mask.test.ts',
  'mask.branches.test.ts',
  'penetration-test.test.ts',
  'registry.test.ts',
  'pathValidator.test.ts',
  'pathValidator.extended.test.ts',
  'ignoredPathFilter.test.ts',
  'pathUtils.test.ts',
  'commandValidator.test.ts',
  'withSecurityValidation.basic.test.ts',
  'withSecurityValidation.extractRepoOwner.test.ts',
  'withSecurityValidation.extractResearchFields.test.ts',
  'withSecurityValidation.logging.test.ts',
  'local-tools-sanitization.test.ts',
  'investigate-bypasses.test.ts',
  'octocodeHomeAccess.test.ts',
  'readme-examples.test.ts',
  'regexes-all.test.ts',
];

// Regex sub-modules that live only in octocode-security-utils, not in our src/regexes/
const REGEX_SUBMODULES = [
  'ai-providers', 'analytics', 'auth-crypto', 'aws',
  'cloudProviders', 'communications', 'databases', 'devTools',
  'monitoring', 'payments-commerce', 'vcs',
];

function transformImports(src, filename) {
  let out = src;

  // Redirect individual regex sub-module imports → utils source
  for (const mod of REGEX_SUBMODULES) {
    out = out.replaceAll(
      `'../src/regexes/${mod}.js'`,
      `'../../octocode-security-utils/src/regexes/${mod}.js'`
    );
    out = out.replaceAll(
      `"../src/regexes/${mod}.js"`,
      `"../../octocode-security-utils/src/regexes/${mod}.js"`
    );
  }

  // investigate-bypasses.test.ts has an import of safeExec from octocode-mcp — skip it
  if (filename === 'investigate-bypasses.test.ts') {
    out = out.replace(
      /import.*safeExec.*from.*safe\.js['"]\s*;?\n/g,
      '// safeExec import removed — not in octocode-security\n'
    );
  }

  return out;
}

let adaptedCount = 0;
for (const file of FILES_TO_ADAPT) {
  const srcPath = join(UTILS_TESTS, file);
  if (!existsSync(srcPath)) {
    console.warn('  SKIP (not found):', file);
    continue;
  }
  const src = readFileSync(srcPath, 'utf8');
  const transformed = transformImports(src, file);
  writeFileSync(join(OUT_TESTS, file), transformed, 'utf8');
  adaptedCount++;
  console.log('  \u2713', file);
}

console.log('\nAdapted', adaptedCount, 'files\n');

// ---------------------------------------------------------------------------
// vitest.config.ts
// ---------------------------------------------------------------------------
const vitestConfig = [
  "import { defineConfig } from 'vitest/config';",
  '',
  'export default defineConfig({',
  '  test: {',
  '    globals: false,',
  "    environment: 'node',",
  "    setupFiles: ['./tests/setup.ts'],",
  "    include: ['tests/**/*.test.ts'],",
  '    coverage: {',
  "      provider: 'v8',",
  "      include: ['src/**/*.ts'],",
  "      exclude: ['src/native.ts', 'src/regexes/types.ts'],",
  '      thresholds: { statements: 85, branches: 80, functions: 85, lines: 85 },',
  "      reporter: ['text', 'lcov'],",
  '    },',
  '    testTimeout: 15000,',
  '  },',
  '});',
  '',
].join('\n');

writeFileSync(join(PKG_ROOT, 'vitest.config.ts'), vitestConfig, 'utf8');
console.log('\u2713 vitest.config.ts');

// ---------------------------------------------------------------------------
// patch package.json
// ---------------------------------------------------------------------------
const pkgPath = join(PKG_ROOT, 'package.json');
const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));

pkg.scripts = {
  ...pkg.scripts,
  test:          'vitest run --coverage',
  'test:quiet':  'vitest run',
  'test:watch':  'vitest watch',
  'test:ui':     'vitest --ui',
};

pkg.devDependencies = {
  ...pkg.devDependencies,
  '@vitest/coverage-v8': '^4.0.0',
  vitest:                '^4.0.0',
};

writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n', 'utf8');
console.log('\u2713 package.json updated');

console.log('\nNext: node scripts/write-rust-tests.mjs && yarn install && yarn test\n');
