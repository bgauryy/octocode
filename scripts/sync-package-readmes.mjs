#!/usr/bin/env node
import { copyFileSync, existsSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const rootDir = resolve(__dirname, '..');
const rootReadme = join(rootDir, 'README.md');

// Packages with hand-authored READMEs that must never be overwritten by this
// script. They are committed to git and explicitly un-ignored in .gitignore
// via the !/packages/<name>/README.md rules.
const PROTECTED_PACKAGES = new Set([
  'packages/octocode-awareness',
  'packages/octocode-pi-extension',
  'packages/octocode-agent',
]);

const TARGETS = [
  'packages/octocode',
  'packages/octocode-engine',
  'packages/octocode-mcp',
];

for (const target of TARGETS) {
  if (PROTECTED_PACKAGES.has(target)) {
    throw new Error(
      `Cannot sync root README.md to ${target} — it has a hand-authored README.md ` +
      `committed to git. Remove it from TARGETS.`,
    );
  }

  const packageDir = join(rootDir, target);
  const packageJsonPath = join(packageDir, 'package.json');

  if (!existsSync(packageJsonPath) || !statSync(packageDir).isDirectory()) {
    throw new Error(`Expected a package directory with package.json: ${packageDir}`);
  }

  copyFileSync(rootReadme, join(packageDir, 'README.md'));
  console.log(`✓ README.md synced to ${relative(rootDir, packageDir)}`);
}
