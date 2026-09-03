#!/usr/bin/env node
/**
 * Publish guard: fails if any dependency that ships in the published package
 * still uses a local-only protocol (`workspace:` or `file:`).
 *
 * npm does not understand `workspace:` (yarn/pnpm-only), and `file:` specs leak
 * machine-local paths into the tarball. Both break npm consumers. This guard
 * runs on prepack/prepublish so the leak is caught before the artifact is built.
 *
 * Only checks the dependency sections a consumer actually installs. A consumer
 * of a published package never installs its devDependencies, so a local-only ref
 * there cannot trigger install failures. Runtime internal dependencies must be
 * semver-pinned before publishing.
 */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = JSON.parse(readFileSync(join(packageRoot, 'package.json'), 'utf8'));

const PUBLISHED_DEP_FIELDS = [
  'dependencies',
  'optionalDependencies',
  'peerDependencies',
  'bundledDependencies',
  'bundleDependencies',
];

const offenders = [];
for (const field of PUBLISHED_DEP_FIELDS) {
  const deps = pkg[field];
  if (!deps || typeof deps !== 'object') continue;
  for (const [name, spec] of Object.entries(deps)) {
    if (typeof spec !== 'string') continue;
    if (spec.startsWith('workspace:') || spec.startsWith('file:')) {
      offenders.push(`  ${field}.${name}: "${spec}"`);
      continue;
    }
  }
}

if (offenders.length > 0) {
  console.error(
    `\n✗ ${pkg.name}@${pkg.version}: local-only dependency protocols must not ship to the registry.\n` +
      `  npm consumers cannot install workspace: or machine-local file: specs. Pin to real versions before publishing.\n\n` +
      offenders.join('\n') +
      `\n\n  Run \`yarn sync:version:publish\` (or set an explicit semver) and retry.\n`
  );
  process.exit(1);
}

console.log(
  `✓ ${pkg.name}@${pkg.version}: no workspace:/file: protocol in published deps.`
);
