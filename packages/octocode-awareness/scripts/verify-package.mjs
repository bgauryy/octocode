#!/usr/bin/env node

import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = JSON.parse(readFileSync(join(packageRoot, 'package.json'), 'utf8'));

// The outer verifier invokes yarn pack; its prepack recursively invokes this
// script. The inner pass only needs the build, so stop before packing again.
if (process.env.OCTOCODE_VERIFY_PACKAGE_INNER === '1') process.exit(0);

// Same discovery rule as build.mjs — kept independent (not imported) so this
// verification catches a real build-vs-source mismatch instead of trivially
// agreeing with whatever build.mjs produced.
function discoverPackageSkills() {
  const skillsRoot = join(packageRoot, 'skills');
  if (!existsSync(skillsRoot)) return [];
  return readdirSync(skillsRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) => name !== 'scripts')
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

const packRunner = process.env.npm_execpath;
assert(packRunner && existsSync(packRunner), 'pack verification must run through a package-manager runtime (yarn or npm)');
const isYarn = /yarn/i.test(packRunner);
// npm_execpath is a JS entry under npm (needs node) but may be an executable
// shell shim under yarn (must be executed directly).
const [packCommand, packPrefixArgs] = /\.[cm]?js$/.test(packRunner)
  ? [process.execPath, [packRunner]]
  : [packRunner, []];
const packOutput = run(packCommand, [...packPrefixArgs, 'pack', '--dry-run', '--json'], {
  env: { ...process.env, OCTOCODE_VERIFY_PACKAGE_INNER: '1' },
});

/**
 * Extract the packed file list from either runner's --json output. Lifecycle
 * (prepack → yarn build) banners are interleaved on the same stdout, so parse
 * defensively:
 * - yarn pack --json: NDJSON rows, one { location } object per line.
 * - npm pack --json: one pretty-printed JSON array [{ files: [{ path }] }],
 *   starting at the first line that begins with '['.
 */
function parsePackedFiles(output, yarn) {
  if (yarn) {
    return output.trim().split('\n').flatMap((line) => {
      const trimmed = line.trim();
      if (!trimmed.startsWith('{')) return [];
      let row;
      try { row = JSON.parse(trimmed); } catch { return []; }
      return row && typeof row === 'object' && row.location ? [String(row.location)] : [];
    });
  }
  const lines = output.split('\n');
  const start = lines.findIndex((line) => line.trimStart().startsWith('['));
  if (start === -1) return [];
  let parsed;
  try { parsed = JSON.parse(lines.slice(start).join('\n')); } catch { return []; }
  const entry = Array.isArray(parsed) ? parsed[0] : parsed;
  const rows = entry && typeof entry === 'object' && Array.isArray(entry.files) ? entry.files : [];
  return rows.flatMap((row) => (row && typeof row === 'object' && row.path ? [String(row.path)] : []));
}

const files = parsePackedFiles(packOutput, isYarn);
assert(files.length > 0, `${isYarn ? 'yarn' : 'npm'} pack --dry-run --json produced no parseable file rows`);
for (const required of [
  'LICENSE',
  'README.md',
  'package.json',
  'out/index.js',
  'out/types/src/index.d.ts',
  'out/octocode-awareness.js',
  'out/schema-api.js',
  'out/docs/README.md',
  'out/assets/logo.png',
]) {
  assert(files.includes(required), `packed artifact is missing ${required}`);
}
// Publish only built runtime assets, standalone skills, and npm root metadata.
const topLevelGroups = new Set(files.map((path) => path.split('/')[0]));
for (const group of topLevelGroups) {
  assert(
    ['out', 'skills', 'LICENSE', 'README.md', 'package.json'].includes(group),
    `unexpected top-level published path "${group}" — everything but out/, skills/, LICENSE, README.md, package.json must nest under out/`,
  );
}
assert(pkg.types === './out/types/src/index.d.ts', `package types must point at the verified declaration entry, got ${String(pkg.types)}`);
assert(readFileSync(join(packageRoot, 'out/types/src/index.d.ts'), 'utf8').includes('export'), 'declaration entry is empty or malformed');
assert(Object.keys(pkg.dependencies ?? {}).length === 0, 'Awareness must keep zero npm runtime dependencies');
assert(!files.some((path) => path.startsWith('dist/')), 'legacy dist/ artifacts must not ship');
assert(packageSkills.length > 0, 'skill discovery found zero skills under package skills/');
for (const skill of packageSkills) {
  // Both copies ship deliberately: skills/ is the user-facing tree (works
  // without the extension); out/skills/ is the runtime-bundled copy.
  assert(
    files.includes(`skills/${skill}/SKILL.md`),
    `packed artifact must ship skills/${skill}/SKILL.md`,
  );
  assert(
    files.includes(`out/skills/${skill}/SKILL.md`),
    `packed artifact must ship out/skills/${skill}/SKILL.md`,
  );
}
assert(!files.some((path) => path.endsWith('.map')), 'source maps must not ship in the package');
assert(
  !files.some((path) => path.endsWith('octocode-config.mjs')),
  'gitignored, machine-generated octocode-config.mjs must never be vendored into the published package',
);
for (const path of files.filter((path) => path.startsWith('out/') && !path.startsWith('out/skills/') && /\.(?:m?js)$/.test(path))) {
  const source = readFileSync(join(packageRoot, path), 'utf8');
  assert(
    !source.includes('@octocodeai/octocode-tools-core') && !source.includes('packages/octocode/out/octocode.js'),
    `${path} must not bundle or delegate to the Octocode research CLI`,
  );
}

const isolated = mkdtempSync(join(tmpdir(), 'octocode-awareness-pack-check-'));
try {
  // Exercise the real archive, not a copy of the development build tree.
  run(packCommand, [...packPrefixArgs, 'pack', ...(isYarn
    ? ['--out', join(isolated, 'package.tgz')]
    : ['--pack-destination', isolated, '--json'])], {
    env: { ...process.env, OCTOCODE_VERIFY_PACKAGE_INNER: '1' },
    timeout: 120_000,
  });
  const archive = readdirSync(isolated).find((name) => name.endsWith('.tgz'));
  assert(archive, 'pack did not create a tarball');
  run('tar', ['-xzf', join(isolated, archive), '-C', isolated]);
  const installed = join(isolated, 'package');
  const cli = join(installed, 'out/octocode-awareness.js');
  const installedOptions = { cwd: installed, env: { ...process.env, OCTOCODE_HOME: join(isolated, 'home') } };
  for (const tree of ['skills', 'out/skills']) {
    const skill = join(installed, tree, 'octocode-awareness');
    for (const file of ['SKILL.md', 'references/architecture.md', 'scripts/awareness.mjs', 'scripts/hook-runner.mjs']) {
      assert(readFileSync(join(skill, file)).equals(readFileSync(join(packageRoot, 'skills/octocode-awareness', file))),
        `published ${tree}/octocode-awareness/${file} differs from the current package skill`);
    }
    const runner = join(skill, 'scripts/awareness.mjs');
    const help = run(process.execPath, [runner, '--help'], installedOptions);
    assert(help.includes(join(installed, tree)), `${tree} runner must discover its own bundled skill`);
    const docs = JSON.parse(run(process.execPath, [runner, 'docs', 'show', 'architecture', '--compact'], installedOptions));
    assert(docs.ok === true, `${tree} runner cannot load its skill references`);
  }
  const help = run(process.execPath, [cli, '--help'], installedOptions);
  assert(help.includes(join(installed, 'out/skills')), 'published CLI must discover its bundled skill tree');
  assert(help.includes('octocode-awareness'), 'published CLI must name its bundled skill');
  assert(help.includes('docs list --compact'), 'published CLI must advertise the command backed by bundled skill docs');
  // Schemas are served dynamically by the CLI — no static out/schemas files.
  const names = JSON.parse(run(process.execPath, [cli, 'schema', 'list', '--compact'], installedOptions));
  assert(Array.isArray(names) && names.length > 0, 'schema list must return a non-empty schema name array');
  assert(!existsSync(join(installed, 'out/schemas')), 'static out/schemas must not ship — schemas are served dynamically');
  for (const name of names) {
    const schema = JSON.parse(run(process.execPath, [cli, 'schema', 'json-schema', name, '--compact'], installedOptions));
    assert(schema && typeof schema === 'object' && schema.type === 'object', `${name} json-schema must be an object schema`);
    const example = run(process.execPath, [cli, 'schema', 'example', name, '--compact'], installedOptions);
    run(process.execPath, [cli, 'schema', 'validate', name, '-', '--compact'], { ...installedOptions, input: example });
  }

  run(process.execPath, [cli, 'maintenance', 'self-test', '--compact'], installedOptions);
  const libraryImport = run(process.execPath, [
    '--input-type=module',
    '--eval',
    `const m = await import(${JSON.stringify(pathToFileURL(join(installed, 'out/index.js')).href)}); if (!Object.keys(m).length) process.exit(1);`,
  ], installedOptions);
  assert(libraryImport === '', 'importing the library entry must not run the CLI or write output');
} finally {
  rmSync(isolated, { recursive: true, force: true });
}

console.log(`✓ ${pkg.name}@${pkg.version}: isolated zero-dependency package artifact verified (${files.length} files).`);
