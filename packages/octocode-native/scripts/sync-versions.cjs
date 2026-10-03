'use strict';

const { existsSync, readFileSync, readdirSync, writeFileSync } = require('node:fs');
const { join } = require('node:path');
const { execFileSync } = require('node:child_process');

const root = join(__dirname, '..');
const rootPackagePath = join(root, 'package.json');
const rootPackage = JSON.parse(readFileSync(rootPackagePath, 'utf8'));
const version = rootPackage.version;
const writeJson = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2) + '\n');
const manifestPath = join(root, 'Cargo.toml');
const manifest = readFileSync(manifestPath, 'utf8');
const workspacePackage = /(^\[workspace\.package\]\s*\n)([\s\S]*?)(?=^\[|$(?![\s\S]))/m;
const section = manifest.match(workspacePackage);
if (!section || !/^version\s*=\s*"[^"]+"/m.test(section[2])) {
  throw new Error('Cargo.toml must declare workspace.package.version before version synchronization');
}
writeFileSync(manifestPath, manifest.replace(workspacePackage, (_, heading, body) =>
  heading + body.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`)));

for (const name of Object.keys(rootPackage.optionalDependencies ?? {})) {
  rootPackage.optionalDependencies[name] = version;
}
writeJson(rootPackagePath, rootPackage);
for (const suffix of readdirSync(join(root, 'npm'))) {
  const path = join(root, 'npm', suffix, 'package.json');
  if (!existsSync(path)) continue;
  const platform = JSON.parse(readFileSync(path, 'utf8'));
  platform.version = version;
  writeJson(path, platform);
}

// Update workspace versions using the existing lockfile, without broadly regenerating it.
// Offline resolution avoids fetching; review any dependency changes before building.
execFileSync('cargo', ['metadata', '--manifest-path', manifestPath, '--format-version', '1', '--offline'], {
  stdio: ['ignore', 'ignore', 'inherit'],
});
execFileSync(process.execPath, [join(__dirname, 'check-version-consistency.cjs')], { stdio: 'inherit' });
