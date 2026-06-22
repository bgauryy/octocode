#!/usr/bin/env node
/**
 * Publish guard: fails if any dependency that ships in the published package
 * still uses the `workspace:` protocol.
 *
 * npm does not understand `workspace:` (yarn/pnpm-only) and aborts install with
 * `EUNSUPPORTEDPROTOCOL`. Yarn berry rewrites the protocol to a real version
 * during `yarn npm publish`, but a plain `npm publish` (or a missed pin step)
 * leaks it into the tarball and breaks every npm consumer. This guard runs on
 * prepack/prepublish so the leak is caught before the artifact is built.
 *
 * Only checks the dependency sections a consumer actually installs. A consumer
 * of a published package never installs its devDependencies, so a `workspace:`
 * ref there cannot trigger EUNSUPPORTEDPROTOCOL — and @octocodeai/octocode-tools-core
 * legitimately lives in devDependencies now: it is bundled into the build output
 * (esbuild) and never published, so it is a build-time-only workspace link.
 * (npm also auto-corrects any leftover workspace: ref on publish.)
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

const UNPUBLISHED_RUNTIME_PACKAGES = new Set([
  '@octocodeai/octocode-tools-core',
]);

const offenders = [];
for (const field of PUBLISHED_DEP_FIELDS) {
  const deps = pkg[field];
  if (!deps || typeof deps !== 'object') continue;
  for (const [name, spec] of Object.entries(deps)) {
    if (typeof spec === 'string' && spec.startsWith('workspace:')) {
      offenders.push(`  ${field}.${name}: "${spec}"`);
      continue;
    }
    if (UNPUBLISHED_RUNTIME_PACKAGES.has(name)) {
      offenders.push(
        `  ${field}.${name}: "${spec}" (unpublished package must be bundled, not installed)`
      );
    }
  }
}

if (offenders.length > 0) {
  console.error(
    `\n✗ ${pkg.name}@${pkg.version}: workspace: protocol must not ship to the registry.\n` +
      `  npm cannot install it (EUNSUPPORTEDPROTOCOL). Pin to a real version before publishing.\n\n` +
      offenders.join('\n') +
      `\n\n  Run \`yarn sync:version:publish\` (or set an explicit semver) and retry.\n`
  );
  process.exit(1);
}

console.log(
  `✓ ${pkg.name}@${pkg.version}: no workspace: protocol or unpublished packages in published deps.`
);
