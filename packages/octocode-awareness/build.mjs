#!/usr/bin/env node

import * as esbuild from 'esbuild';
import { builtinModules } from 'node:module';
import { execFileSync, execSync } from 'node:child_process';
import {
  chmodSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync,
  rmSync, writeFileSync,
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const packageRoot = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(packageRoot, '../..');
const outDir = join(packageRoot, 'out');
const legacyDistDir = join(packageRoot, 'dist');
const skillStageDir = join(outDir, '.skill-build');
const canonicalSkillsRoot = join(repoRoot, 'skills');
const canonicalAwarenessSkill = join(canonicalSkillsRoot, 'octocode-awareness');
const canonicalAwarenessScripts = join(canonicalAwarenessSkill, 'scripts');
const agentSkillsRoot = join(repoRoot, '.agents', 'skills');
const tscBin = resolve(packageRoot, '../../node_modules/.bin/tsc');

process.chdir(packageRoot);
rmSync(outDir, { recursive: true, force: true });
rmSync(legacyDistDir, { recursive: true, force: true });
rmSync(join(packageRoot, 'skills'), { recursive: true, force: true });

const external = [
  ...builtinModules,
  ...builtinModules.map((name) => `node:${name}`),
];

const shared = {
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node22',
  external,
  sourcemap: false,
  treeShaking: true,
};

const coreEntries = {
  index: 'src/index.ts',
  'octocode-awareness': 'bin/awareness.ts',
  'hook-runner': 'bin/hook-runner.ts',
  'extract-hook-files': 'bin/extract-hook-files.ts',
  schema: 'src/schema/cli.ts',
};

// One Awareness-owned output graph. Shared domain modules become chunks; the
// schema lane stays lazy from the main CLI and carries the bundled Zod runtime.
await esbuild.build({
  ...shared,
  entryPoints: coreEntries,
  outdir: 'out',
  entryNames: '[name]',
  chunkNames: 'chunks/[name]-[hash]',
  splitting: true,
  minify: true,
  logLevel: 'info',
});

// The installed Agent Skill must remain runnable after it is copied away from
// the npm package. Build standalone Awareness helpers from the same TS sources;
// no octocode CLI or tools-core source participates in these bundles.
mkdirSync(skillStageDir, { recursive: true });
await Promise.all([
  esbuild.build({ ...shared, entryPoints: ['bin/awareness.ts'], outfile: join(skillStageDir, 'awareness.mjs'), minify: true }),
  esbuild.build({ ...shared, entryPoints: ['bin/hook-runner.ts'], outfile: join(skillStageDir, 'hook-runner.mjs'), minify: true }),
  esbuild.build({ ...shared, entryPoints: ['bin/extract-hook-files.ts'], outfile: join(skillStageDir, 'extract-hook-files.mjs'), minify: true }),
  esbuild.build({ ...shared, entryPoints: ['src/schema/cli.ts'], outfile: join(skillStageDir, 'schema.mjs'), minify: true }),
]);

execSync(`${tscBin} --emitDeclarationOnly --outDir out/types -p tsconfig.build.json`, {
  cwd: packageRoot,
  stdio: 'inherit',
});

const warningGuard = [
  '#!/usr/bin/env node',
  "process.removeAllListeners('warning');",
  "process.on('warning', (warning) => {",
  "  if (warning?.name === 'ExperimentalWarning' && String(warning?.message).includes('SQLite')) return;",
  '  console.error(warning?.stack ?? String(warning));',
  '});',
].join('\n');

function makeExecutable(path, banner = '#!/usr/bin/env node') {
  const source = readFileSync(path, 'utf8');
  writeFileSync(path, `${banner}\n${source.replace(/^#![^\n]*\n?/, '')}`);
  chmodSync(path, 0o755);
}

for (const name of ['octocode-awareness.js', 'hook-runner.js']) {
  makeExecutable(join(outDir, name), warningGuard);
}
for (const name of ['extract-hook-files.js', 'schema.js']) {
  makeExecutable(join(outDir, name));
}
for (const name of ['awareness.mjs', 'hook-runner.mjs']) {
  makeExecutable(join(skillStageDir, name), warningGuard);
}
for (const name of ['extract-hook-files.mjs', 'schema.mjs']) {
  makeExecutable(join(skillStageDir, name));
}

// Generate one static JSON Schema per Zod contract. Both the package CLI and a
// copied skill expose these exact files through `schema path <name>`.
const packageSchemaDir = join(outDir, 'schemas');
mkdirSync(packageSchemaDir, { recursive: true });
const schemaEntry = join(outDir, 'schema.js');
const schemaNames = JSON.parse(execFileSync(
  process.execPath,
  [schemaEntry, 'list', '--compact'],
  { cwd: packageRoot, encoding: 'utf8' },
));
for (const name of schemaNames) {
  const jsonSchema = JSON.parse(execFileSync(
    process.execPath,
    [schemaEntry, 'json-schema', name, '--compact'],
    { cwd: packageRoot, encoding: 'utf8' },
  ));
  const example = JSON.parse(execFileSync(
    process.execPath,
    [schemaEntry, 'example', name, '--compact'],
    { cwd: packageRoot, encoding: 'utf8' },
  ));
  jsonSchema.$id = `urn:octocode-awareness:schema:${name}`;
  jsonSchema.examples = [example];
  writeFileSync(
    join(packageSchemaDir, `${name}.schema.json`),
    `${JSON.stringify(jsonSchema, null, 2)}\n`,
  );
}

// Refresh only generated artifacts inside the canonical Awareness skill.
mkdirSync(canonicalAwarenessScripts, { recursive: true });
for (const name of [
  'awareness.mjs', 'hook-runner.mjs', 'extract-hook-files.mjs', 'schema.mjs',
]) {
  cpSync(join(skillStageDir, name), join(canonicalAwarenessScripts, name));
}
rmSync(join(canonicalAwarenessScripts, 'runtime'), { recursive: true, force: true });
rmSync(join(canonicalAwarenessScripts, 'schemas'), { recursive: true, force: true });
cpSync(packageSchemaDir, join(canonicalAwarenessScripts, 'schemas'), { recursive: true });
rmSync(skillStageDir, { recursive: true, force: true });

const retiredSkills = new Set(['octocode-agent-communication', 'octocode-reflection']);
const skipGeneratedConfig = (path) => !path.endsWith('octocode-config.mjs');
const bundledSkills = readdirSync(canonicalSkillsRoot, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && !retiredSkills.has(entry.name))
  .filter((entry) => existsSync(join(canonicalSkillsRoot, entry.name, 'SKILL.md')))
  .map((entry) => entry.name)
  .sort();

const packagedSkillsRoot = join(outDir, 'skills');
rmSync(packagedSkillsRoot, { recursive: true, force: true });
mkdirSync(packagedSkillsRoot, { recursive: true });
mkdirSync(agentSkillsRoot, { recursive: true });

for (const skillName of retiredSkills) {
  rmSync(join(agentSkillsRoot, skillName), { recursive: true, force: true });
}
for (const skillName of bundledSkills) {
  const source = join(canonicalSkillsRoot, skillName);
  const packaged = join(packagedSkillsRoot, skillName);
  const mirrored = join(agentSkillsRoot, skillName);
  rmSync(mirrored, { recursive: true, force: true });
  cpSync(source, packaged, {
    recursive: true,
    filter: (path) => !path.includes('node_modules') && skipGeneratedConfig(path),
  });
  cpSync(source, mirrored, {
    recursive: true,
    filter: (path) => !path.includes('node_modules') && skipGeneratedConfig(path),
  });
}

console.log('✓ @octocodeai/octocode-awareness built → out/');
console.log(`✓ Awareness CLI → out/octocode-awareness.js (${readdirSync(join(outDir, 'chunks')).length} shared chunks)`);
console.log(`✓ generated ${schemaNames.length} standalone Zod schema files`);
console.log(`✓ bundled skills → out/skills/ (${bundledSkills.join(', ')})`);
