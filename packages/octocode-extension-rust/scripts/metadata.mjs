import { isDeepStrictEqual } from 'node:util';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { platforms } from '../platforms.cjs';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const manifestPath = join(root, 'package.json');
const pkg = JSON.parse(readFileSync(manifestPath, 'utf8'));
const write = process.argv.includes('--write');
const expected = structuredClone(pkg);
expected.optionalDependencies = Object.fromEntries(platforms.map(value => [value.packageName, pkg.version]));
expected.napi = { binaryName: 'octocode-extension-rust', targets: platforms.map(value => value.target) };
for (const platform of platforms) expected.scripts[`build:${platform.id}`] = `node scripts/build.mjs --release --target ${platform.id}`;
function check(path, actual, desired) {
  if (isDeepStrictEqual(actual, desired)) return;
  if (!write) throw new Error(`Platform metadata drift: ${path}; run yarn version:sync`);
  writeFileSync(path, JSON.stringify(desired, null, 2) + '\n');
}
check(manifestPath, pkg, expected);
for (const platform of platforms) {
  const directory = join(root, 'npm', platform.id);
  if (write) mkdirSync(directory, { recursive: true });
  const path = join(directory, 'package.json');
  const desired = {
    name: platform.packageName, version: pkg.version,
    description: `Octocode extension native operations for ${platform.id}`,
    license: pkg.license, main: platform.binary, files: [platform.binary, 'README.md'],
    os: [platform.os], cpu: [platform.arch], ...(platform.libc ? { libc: [platform.libc] } : {}),
    engines: pkg.engines, publishConfig: pkg.publishConfig,
    repository: { ...pkg.repository, directory: `${pkg.repository.directory}/npm/${platform.id}` },
    scripts: { prepublishOnly: `node ../../scripts/check-platforms.mjs --target ${platform.id}` },
  };
  let actual;
  try { actual = JSON.parse(readFileSync(path, 'utf8')); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  check(path, actual, desired);
}
const cargo = readFileSync(join(root, 'Cargo.toml'), 'utf8');
const cargoVersion = cargo.match(/\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];
if (cargoVersion !== pkg.version) throw new Error(`Cargo version ${cargoVersion} differs from package version ${pkg.version}; update the package version in Cargo.toml.`);
console.log(`Platform metadata verified for ${platforms.length} targets at ${pkg.version}`);
