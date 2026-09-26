'use strict';

const { existsSync, readFileSync, readdirSync } = require('node:fs');
const { join } = require('node:path');
const { workspacePackages } = require('./workspace-metadata.cjs');

const root = join(__dirname, '..');
const json = path => JSON.parse(readFileSync(path, 'utf8'));
const pkg = json(join(root, 'package.json'));
const version = pkg.version;

try {
  for (const crate of workspacePackages(root)) {
    if (crate.version !== version) {
      throw new Error(`${crate.name} crate is ${crate.version}, expected ${version}`);
    }
  }
  for (const [name, dependencyVersion] of Object.entries(pkg.optionalDependencies ?? {})) {
    if (dependencyVersion !== version) throw new Error(`${name} is ${dependencyVersion}, expected ${version}`);
  }
  for (const suffix of readdirSync(join(root, 'npm'))) {
    const path = join(root, 'npm', suffix, 'package.json');
    if (!existsSync(path)) continue;
    const platform = json(path);
    if (platform.version !== version) throw new Error(`${platform.name} is ${platform.version}, expected ${version}`);
  }
  console.log(`version:check ok: consolidated distribution is ${version}`);
} catch (error) {
  console.error(`version:check failed: ${error.message}`);
  process.exitCode = 1;
}
