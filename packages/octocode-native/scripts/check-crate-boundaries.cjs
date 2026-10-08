'use strict';

const { join } = require('node:path');
const { workspacePackages } = require('./workspace-metadata.cjs');

function checkBoundaries(packages) {
  const crates = new Map(packages.map(pkg => [pkg.name, pkg]));
  const allowed = new Map([
    ['octocode-engine', []],
    ['octocode-github', []],
    ['octocode-native', ['octocode-engine', 'octocode-github']],
    ['octocode-cli', ['octocode-native', 'octocode-engine']],
    ['octocode-runtime-napi', ['octocode-native', 'octocode-engine']],
  ]);
  const failures = [];
  for (const [name, dependencies] of allowed) {
    const pkg = crates.get(name);
    if (!pkg) { failures.push(`missing workspace crate ${name}`); continue; }
    if (!Array.isArray(pkg.publish) || pkg.publish.length !== 0) failures.push(`${name} must set publish = false`);
    for (const dependency of pkg.dependencies) {
      if (crates.has(dependency.name) && !dependencies.includes(dependency.name)) {
        failures.push(`${name} must not depend on ${dependency.name}`);
      }
      if ((name === 'octocode-native' || name === 'octocode-engine') && /^napi(?:-|$)/.test(dependency.name)) {
        failures.push(`${name} must not depend on N-API`);
      }
      if (name === 'octocode-github' && /^(?:napi(?:-|$)|octocode-(?:native|engine|config)$)|keyring|^config$/.test(dependency.name)) {
        failures.push(`GitHub protocol crate must not depend on ${dependency.name}`);
      }
    }
  }
  for (const name of crates.keys()) if (!allowed.has(name)) failures.push(`undeclared workspace crate ${name}; review its boundary`);
  const runtimeTargets = crates.get('octocode-native')?.targets ?? [];
  const runtimeLibraries = runtimeTargets.filter(target => target.kind.includes('rlib') || target.kind.includes('lib') || target.kind.includes('cdylib'));
  if (runtimeLibraries.length !== 1 || runtimeLibraries[0].crate_types.join(',') !== 'rlib' || runtimeTargets.some(target => target.kind.includes('bin'))) {
    failures.push('runtime must own only an rlib library and no executable targets');
  }
  const cliBins = (crates.get('octocode-cli')?.targets ?? []).filter(target => target.kind.includes('bin')).map(target => target.name).sort();
  if (JSON.stringify(cliBins) !== JSON.stringify(['octocode', 'octocode-regex-worker'])) failures.push('CLI must own both executable targets');
  if (!(crates.get('octocode-runtime-napi')?.targets ?? []).some(target => target.crate_types.includes('cdylib'))) failures.push('runtime N-API adapter must own a cdylib');
  return failures;
}

try {
  const failures = checkBoundaries(workspacePackages(join(__dirname, '..')));
  if (failures.length) throw new Error(failures.join('\n'));
  console.log('crate boundaries: five internal crates follow the declared dependency direction');
} catch (error) {
  console.error(`crate boundaries failed: ${error.message}`);
  process.exitCode = 1;
}
