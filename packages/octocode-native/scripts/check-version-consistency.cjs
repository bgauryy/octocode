'use strict';

const { existsSync, readFileSync, readdirSync } = require('fs');
const { join } = require('path');

const root = join(__dirname, '..');
const fail = message => {
  console.error(`version:check failed: ${message}`);
  process.exit(1);
};
const json = path => JSON.parse(readFileSync(path, 'utf8'));
const pkg = json(join(root, 'package.json'));
const version = pkg.version;

for (const crate of ['engine', 'runtime']) {
  const manifest = readFileSync(join(root, 'crates', crate, 'Cargo.toml'), 'utf8');
  const crateVersion = manifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (crateVersion !== version) fail(`${crate} crate is ${crateVersion}, expected ${version}`);
}

for (const [name, dependencyVersion] of Object.entries(pkg.optionalDependencies ?? {})) {
  if (dependencyVersion !== version) fail(`${name} is ${dependencyVersion}, expected ${version}`);
}

for (const suffix of readdirSync(join(root, 'npm'))) {
  const path = join(root, 'npm', suffix, 'package.json');
  if (!existsSync(path)) continue;
  const platform = json(path);
  if (platform.version !== version) fail(`${platform.name} is ${platform.version}, expected ${version}`);
}

const compatibility = json(join(root, '..', 'octocode-engine', 'package.json'));
if (compatibility.version !== version) {
  fail(`compatibility package is ${compatibility.version}, expected ${version}`);
}
if (compatibility.dependencies?.[pkg.name] !== version) {
  fail(`compatibility dependency must pin ${pkg.name}@${version}`);
}

console.log(`version:check ok: consolidated distribution is ${version}`);
