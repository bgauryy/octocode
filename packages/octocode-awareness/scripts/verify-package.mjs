#!/usr/bin/env node

import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = JSON.parse(readFileSync(join(packageRoot, 'package.json'), 'utf8'));

// Same discovery rule as build.mjs — kept independent (not imported) so this
// verification catches a real build-vs-source mismatch instead of trivially
// agreeing with whatever build.mjs produced.
const RETIRED_PACKAGE_SKILLS = ['octocode-agent-communication', 'octocode-reflection'];
function discoverPackageSkills() {
  const skillsRoot = join(packageRoot, '..', '..', 'skills');
  return readdirSync(skillsRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) => name !== 'scripts' && !RETIRED_PACKAGE_SKILLS.includes(name))
    .filter((name) => existsSync(join(skillsRoot, name, 'SKILL.md')))
    .sort();
}
const packageSkills = discoverPackageSkills();

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: packageRoot,
    encoding: 'utf8',
    timeout: 30_000,
    ...options,
  });
  if (result.status !== 0) {
    const reason = result.error?.message
      || result.stderr
      || result.stdout
      || (result.signal ? `signal ${result.signal}` : 'unknown subprocess failure');
    throw new Error(
      `${command} ${args.join(' ')} failed (${result.status ?? result.signal ?? 'spawn'}):\n${reason}`,
    );
  }
  return result.stdout;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

const pack = JSON.parse(run('npm', ['pack', '--dry-run', '--json', '--ignore-scripts']));
const files = (pack[0]?.files ?? []).map((entry) => entry.path);
for (const required of ['LICENSE', 'README.md', 'package.json', 'dist/index.js', 'out/types/src/index.d.ts', 'out/octocode-awareness.js']) {
  assert(files.includes(required), `packed artifact is missing ${required}`);
}
assert(pkg.types === './out/types/src/index.d.ts', `package types must point at the verified declaration entry, got ${String(pkg.types)}`);
assert(readFileSync(join(packageRoot, 'out/types/src/index.d.ts'), 'utf8').includes('export'), 'declaration entry is empty or malformed');
assert(packageSkills.length > 0, 'skill discovery found zero skills under repo-root skills/');
for (const skill of packageSkills) {
  assert(
    files.filter((path) => path.endsWith(`skills/${skill}/SKILL.md`)).length === 1,
    `packed artifact must contain exactly one ${skill} skill tree`,
  );
}
assert(!files.some((path) => path.startsWith('skills/')), 'source skills/ must not duplicate out/skills/');
assert(!files.some((path) => path.endsWith('.map')), 'source maps must not ship in the package');
assert(
  !files.some((path) => path.endsWith('octocode-config.mjs')),
  'gitignored, machine-generated octocode-config.mjs must never be vendored into the published package',
);

const isolated = mkdtempSync(join(tmpdir(), 'octocode-awareness-pack-check-'));
try {
  cpSync(join(packageRoot, 'dist'), join(isolated, 'dist'), { recursive: true });
  cpSync(join(packageRoot, 'README.md'), join(isolated, 'README.md'));
  cpSync(join(packageRoot, 'LICENSE'), join(isolated, 'LICENSE'));
  writeFileSync(join(isolated, 'package.json'), JSON.stringify(pkg));

  const cli = join(isolated, 'out/octocode-awareness.js');
  const schema = join(isolated, 'out/skills/octocode-awareness/scripts/schema.mjs');
  const names = JSON.parse(run(process.execPath, [cli, 'schema', 'list', '--compact'], { cwd: isolated }));
  for (const name of names) {
    run(process.execPath, [schema, 'json-schema', name, '--compact'], { cwd: isolated });
    const example = run(process.execPath, [schema, 'example', name, '--compact'], { cwd: isolated });
    run(process.execPath, [schema, 'validate', name, '-', '--compact'], { cwd: isolated, input: example });
  }

  run(process.execPath, [cli, 'maintenance', 'self-test', '--compact'], { cwd: isolated });
  run(process.execPath, [
    '--input-type=module',
    '--eval',
    `const m = await import(${JSON.stringify(pathToFileURL(join(isolated, 'dist/index.js')).href)}); if (!Object.keys(m).length) process.exit(1);`,
  ], { cwd: isolated });
} finally {
  rmSync(isolated, { recursive: true, force: true });
}

console.log(`✓ ${pkg.name}@${pkg.version}: isolated zero-dependency package artifact verified (${files.length} files).`);
