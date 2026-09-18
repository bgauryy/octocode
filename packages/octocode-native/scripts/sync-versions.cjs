'use strict';

const { existsSync, readFileSync, readdirSync, writeFileSync } = require('fs');
const { join } = require('path');
const { execFileSync } = require('child_process');

const root = join(__dirname, '..');
const rootPackagePath = join(root, 'package.json');
const rootPackage = JSON.parse(readFileSync(rootPackagePath, 'utf8'));
const version = rootPackage.version;
const writeJson = (path, value) =>
  writeFileSync(path, JSON.stringify(value, null, 2) + '\n');

for (const name of Object.keys(rootPackage.optionalDependencies ?? {})) {
  rootPackage.optionalDependencies[name] = version;
}
writeJson(rootPackagePath, rootPackage);

for (const crate of ['engine', 'runtime']) {
  const path = join(root, 'crates', crate, 'Cargo.toml');
  const source = readFileSync(path, 'utf8').replace(
    /^version\s*=\s*"[^"]+"/m,
    `version = "${version}"`
  );
  writeFileSync(path, source);
}

for (const suffix of readdirSync(join(root, 'npm'))) {
  const path = join(root, 'npm', suffix, 'package.json');
  if (!existsSync(path)) continue;
  const platform = JSON.parse(readFileSync(path, 'utf8'));
  platform.version = version;
  writeJson(path, platform);
}

const compatibilityPath = join(root, '..', 'octocode-engine', 'package.json');
const compatibility = JSON.parse(readFileSync(compatibilityPath, 'utf8'));
compatibility.version = version;
compatibility.dependencies[rootPackage.name] = version;
writeJson(compatibilityPath, compatibility);

execFileSync('cargo', ['generate-lockfile', '--manifest-path', join(root, 'Cargo.toml')], {
  stdio: 'inherit',
});
execFileSync('node', [join(__dirname, 'check-version-consistency.cjs')], {
  stdio: 'inherit',
});
