#!/usr/bin/env node

import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = JSON.parse(readFileSync(join(packageRoot, 'package.json'), 'utf8'));

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
for (const required of ['LICENSE', 'README.md', 'package.json', 'dist/index.js', 'dist/src/index.d.ts', 'dist/bin/awareness.js']) {
  assert(files.includes(required), `packed artifact is missing ${required}`);
}
assert(pkg.types === './dist/src/index.d.ts', `package types must point at the verified declaration entry, got ${String(pkg.types)}`);
assert(readFileSync(join(packageRoot, 'dist/src/index.d.ts'), 'utf8').includes('export'), 'declaration entry is empty or malformed');
assert(
  files.filter((path) => path.endsWith('skills/octocode-awareness/SKILL.md')).length === 1,
  'packed artifact must contain exactly one octocode-awareness skill tree',
);
assert(!files.some((path) => path.startsWith('skills/')), 'source skills/ must not duplicate dist/skills/');
assert(!files.some((path) => path.endsWith('.map')), 'source maps must not ship in the package');

const isolated = mkdtempSync(join(tmpdir(), 'octocode-awareness-pack-check-'));
try {
  cpSync(join(packageRoot, 'dist'), join(isolated, 'dist'), { recursive: true });
  cpSync(join(packageRoot, 'README.md'), join(isolated, 'README.md'));
  cpSync(join(packageRoot, 'LICENSE'), join(isolated, 'LICENSE'));
  writeFileSync(join(isolated, 'package.json'), JSON.stringify(pkg));

  const cli = join(isolated, 'dist/bin/awareness.js');
  const schema = join(isolated, 'dist/skills/octocode-awareness/scripts/schema.mjs');
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
