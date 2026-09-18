#!/usr/bin/env node

import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const packageRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = JSON.parse(readFileSync(join(packageRoot, 'package.json'), 'utf8'));

// The outer verifier invokes yarn pack; its prepack rebuilds and recursively
// invokes this script. Stop the inner invocation before packing recursively.
if (process.env.OCTOCODE_VERIFY_PACKAGE_INNER === '1') {
  process.exit(0);
}

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

function assertStrictOperationSchema(schema, operation) {
  const branches = schema?.type === 'object' ? [schema] : schema?.oneOf ?? schema?.anyOf;
  assert(Array.isArray(branches) && branches.length > 0, `${operation} must expose an object schema`);
  for (const branch of branches) {
    assert(branch?.type === 'object' && branch.additionalProperties === false, `${operation} must expose only strict object-schema branches`);
  }
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
  'out/types/src/schema-api.d.ts',
  'out/host-api.js',
  'out/types/src/host-api.d.ts',
  'out/admin-api.js',
  'out/types/src/admin-api.d.ts',
  'out/docs/README.md',
  'out/assets/logo.png',
]) {
  assert(files.includes(required), `packed artifact is missing ${required}`);
}
// Publish only built runtime assets and npm root metadata. The package-owned
// skills/ tree is development input; build stages the runtime copy under out/.
const topLevelGroups = new Set(files.map((path) => path.split('/')[0]));
for (const group of topLevelGroups) {
  assert(
    ['out', 'LICENSE', 'README.md', 'package.json'].includes(group),
    `unexpected top-level published path "${group}" — everything but out/, LICENSE, README.md, package.json must nest under out/`,
  );
}
assert(pkg.types === './out/types/src/index.d.ts', `package types must point at the verified declaration entry, got ${String(pkg.types)}`);
assert(readFileSync(join(packageRoot, 'out/types/src/index.d.ts'), 'utf8').includes('export'), 'declaration entry is empty or malformed');
assert(Object.keys(pkg.dependencies ?? {}).length === 0, 'Awareness must keep zero mandatory npm runtime dependencies');
assert(Object.keys(pkg.optionalDependencies ?? {}).join(',') === '@octocodeai/octocode-extension-rust',
  'file evidence must use the separately installed optional extension native package');
assert(!files.some((path) => path.endsWith('.node')), 'Awareness must not bundle a platform-native addon');
assert(!files.some((path) => path.startsWith('dist/')), 'legacy dist/ artifacts must not ship');
assert(packageSkills.length > 0, 'skill discovery found zero skills under package skills/');
for (const skill of packageSkills) {
  assert(
    files.includes(`out/skills/${skill}/SKILL.md`),
    `packed artifact must ship out/skills/${skill}/SKILL.md`,
  );
  const packagedSkillManifests = files.filter((path) => path.endsWith(`/skills/${skill}/SKILL.md`));
  assert(
    packagedSkillManifests.length === 1,
    `packed artifact must contain exactly one ${skill} SKILL.md, got ${packagedSkillManifests.join(', ') || 'none'}`,
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
  const installedOptions = { cwd: installed, env: { ...process.env, NODE_PATH: '', OCTOCODE_HOME: join(isolated, 'home') } };
  for (const tree of ['out/skills']) {
    const skill = join(installed, tree, 'octocode-awareness');
    for (const file of ['SKILL.md', 'references/architecture.md', 'scripts/awareness.mjs', 'scripts/hook-runner.mjs']) {
      assert(readFileSync(join(skill, file)).equals(readFileSync(join(packageRoot, 'skills/octocode-awareness', file))),
        `published ${tree}/octocode-awareness/${file} differs from the current package skill`);
    }
    const runner = join(skill, 'scripts/awareness.mjs');
    const help = run(process.execPath, [runner, '--help'], installedOptions);
    assert(help.includes('instructions') && help.includes('context'), `${tree} runner must expose canonical agent discovery`);
  }
  const help = run(process.execPath, [cli, '--help'], installedOptions);
  assert(help.includes('octocode-awareness'), 'published CLI must name its bundled skill');
  assert(help.includes('ONE SURFACE') && help.includes('FIVE CONCEPTS'), 'published CLI must advertise the canonical concepts');
  assert(
    help.includes('maintenance retention') && help.includes('maintenance store-retire'),
    'published CLI must advertise both explicit operator maintenance commands',
  );
  for (const removed of ['docs list', 'skill install', 'refinement']) {
    assert(!help.includes(removed), `published CLI help still advertises removed surface: ${removed}`);
  }

  const surface = JSON.parse(run(process.execPath, [cli, 'schema', 'commands', '--compact'], installedOptions));
  assert(surface.ok === true, 'schema commands failed');
  assert(Object.keys(surface.concepts ?? {}).join(',') === 'context,work,message,memory,history', 'schema commands returned the wrong concepts');
  assert(Array.isArray(surface.operations), 'schema commands must return an operation list');
  for (const operation of ['context.observe', 'context.feedback', 'memory.set', 'memory.get', 'memory.revalidate', 'history.experience']) {
    assert(surface.operations.includes(operation), `${operation} must be callable from the packed CLI`);
  }
  for (const operation of surface.operations) {
    const [concept, action] = operation.split('.');
    const schema = JSON.parse(run(process.execPath, [cli, 'schema', 'command', concept, action, '--compact'], installedOptions));
    assertStrictOperationSchema(schema, operation);
    assert(schema['x-awareness-operation'] === operation, `${operation} descriptor identity drifted`);
  }
  for (const operation of ['maintenance.retention', 'maintenance.store-retire']) {
    const [concept, action] = operation.split('.');
    const schema = JSON.parse(run(process.execPath, [cli, 'schema', 'command', concept, action, '--compact'], installedOptions));
    assertStrictOperationSchema(schema, operation);
    assert(schema['x-awareness-operation'] === operation, `${operation} operator descriptor identity drifted`);
    assert(!surface.operations.includes(operation), `${operation} must remain outside routine agent discovery`);
  }
  const entities = JSON.parse(run(process.execPath, [cli, 'schema', 'entities', '--compact'], installedOptions));
  assert(entities.ok === true && Array.isArray(entities.families) && entities.families.length > 0, 'schema entities must expose canonical storage entities');
  assert(!existsSync(join(installed, 'out/schemas')), 'static out/schemas must not ship — schemas are served dynamically');

  const evidenceWorkspace = join(isolated, 'evidence-workspace');
  mkdirSync(evidenceWorkspace);
  const standaloneRunner = join(installed, 'out/skills/octocode-awareness/scripts/awareness.mjs');
  for (const [name, entry] of [['CLI', cli], ['standalone skill', standaloneRunner]]) {
    const binding = ['--workspace', evidenceWorkspace, '--db', join(isolated, `${name.replaceAll(' ', '-')}.sqlite3`), '--agent-id', 'pack-check', '--compact'];
    const oriented = JSON.parse(run(process.execPath, [entry, 'context', 'orient', ...binding], installedOptions));
    assert(oriented.self?.actorId === 'pack-check' && oriented.partial === false, `${name} canonical context orient failed`);
    const recorded = JSON.parse(run(process.execPath, [entry, 'memory', 'record', '--task-context', 'package verification', '--observation', `${name} executed the canonical surface`, '--importance', '5', ...binding], installedOptions));
    assert(recorded.ok === true, `${name} canonical memory record failed`);
    const recalled = JSON.parse(run(process.execPath, [entry, 'memory', 'recall', ...binding], installedOptions));
    assert(recalled.count === 1, `${name} canonical memory recall did not return the recorded fact`);
  }

  const libraryImport = run(process.execPath, [
    '--input-type=module',
    '--eval',
    `
      import assert from 'node:assert/strict';
      import { createRequire } from 'node:module';
      const entry = ${JSON.stringify(pathToFileURL(join(installed, 'out/index.js')).href)};
      const require = createRequire(entry);
      assert.throws(() => require.resolve('@octocodeai/octocode-extension-rust'), { code: 'MODULE_NOT_FOUND' });
      const root = await import(entry);
      assert.deepEqual(root.ROUTINE_AWARENESS_OPERATIONS, ${JSON.stringify(surface.operations)}, 'packed CLI and root operation catalogs must agree');
      assert.equal(new Set(root.ROUTINE_AWARENESS_OPERATIONS).size, root.ROUTINE_AWARENESS_OPERATIONS.length, 'operation catalog must not contain duplicates');
      assert.deepEqual(Object.keys(root).sort(), [
        'AWARENESS_AGENT_INSTRUCTION_SECTIONS',
        'AWARENESS_CONCEPTS',
        'AWARENESS_MESSAGE_PARAMETER_GUIDANCE',
        'ROUTINE_AWARENESS_OPERATIONS',
        'createAwarenessClient',
        'getAwarenessAgentInstructions',
        'getAwarenessOperationDescriptor',
        'listAwarenessOperationDescriptors',
      ]);
      const schema = await import(${JSON.stringify(pathToFileURL(join(installed, 'out/schema-api.js')).href)});
      assert.deepEqual(schema.ROUTINE_AWARENESS_OPERATIONS, root.ROUTINE_AWARENESS_OPERATIONS, 'schema and root operation catalogs must agree');
      assert.deepEqual(Object.keys(schema).sort(), [
        'AWARENESS_CONCEPTS',
        'ROUTINE_AWARENESS_OPERATIONS',
        'getAwarenessOperationDescriptor',
        'listAwarenessOperationDescriptors',
      ]);
      const host = await import(${JSON.stringify(pathToFileURL(join(installed, 'out/host-api.js')).href)});
      assert.equal(typeof host.createAwarenessHost, 'function');
      assert.equal(typeof host.claimNativeHookOwner, 'function');
      const admin = await import(${JSON.stringify(pathToFileURL(join(installed, 'out/admin-api.js')).href)});
      assert.deepEqual(Object.keys(admin).sort(), [
        'StoreRetirementError',
        'applyDatabaseMigration',
        'applyStoreRetirement',
        'previewDatabaseMigration',
        'reportStoreRetirement',
        'verifyDatabaseMigration',
      ]);
    `,
  ], installedOptions);
  assert(libraryImport === '', 'importing the library entry must not run the CLI or write output');
} finally {
  rmSync(isolated, { recursive: true, force: true });
}

console.log(`✓ ${pkg.name}@${pkg.version}: isolated package verified without its optional native dependency (${files.length} files).`);
