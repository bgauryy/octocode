#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const packageRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(packageRoot, '../..');
const distDir = path.join(packageRoot, 'dist');

const require = createRequire(import.meta.url);

// Resolve @octocodeai/octocode-awareness package root via workspace link.
// resolve('@octocodeai/octocode-awareness') → dist/index.js; go up one level for the package root.
const awarenessPkgDir = path.resolve(path.dirname(require.resolve('@octocodeai/octocode-awareness')), '..');

// Resolve @octocodeai/config source via workspace link — no path hardcoding.
const CONFIG_LOADER_SRC = require.resolve('@octocodeai/config');

const SOURCE_PATHS = {
  // TypeScript source is compiled by tsc (see compileTsc()). Only non-code assets — each is copied flat into dist/.
  // are managed here. @octocodeai/config source injected as octocode-config.mjs into every skill that
  // has a scripts/ directory — zero npm publish dependency for standalone skills.
  configLoader: CONFIG_LOADER_SRC,
  rootSkills: path.join(repoRoot, 'skills'),
  skills: path.join(packageRoot, 'skills'),
  systemPrompt: path.join(packageRoot, 'docs', 'PI', 'APPEND_SYSTEM.md'),
  // awareness sources come directly from @octocodeai/octocode-awareness — single source of truth.
  // skills/octocode-awareness/scripts/ = canonical hooks + utilities; dist/bin/ = compiled awareness CLI.
  awarenessScripts: path.join(awarenessPkgDir, 'skills', 'octocode-awareness', 'scripts'),
  awarenessAwarenessMjs: path.join(awarenessPkgDir, 'dist', 'bin', 'awareness.js'),
  awarenessExtractMjs: path.join(awarenessPkgDir, 'dist', 'bin', 'extract-hook-files.js'),
  awarenessSchemaGen: path.join(awarenessPkgDir, 'skills', 'octocode-awareness', 'scripts', 'schema.mjs'),
};

const OUTPUT_PATHS = {
  extension: path.join(distDir, 'index.js'),
  skills: path.join(distDir, 'skills'),
  systemPrompt: path.join(distDir, 'system', 'APPEND_SYSTEM.md'),
  // awareness tooling lives here — scripts for execution, schema.json for alignment
  awarenessScripts: path.join(distDir, 'awareness', 'scripts'),
  awarenessSchema: path.join(distDir, 'awareness', 'schema.json'),
};

const SKIPPED_DIRECTORIES = new Set([
  '.git',
  '.next',
  '.turbo',
  '__pycache__',
  'coverage',
  'dist',
  'node_modules',
  'out',
  'target',
]);

// octocode-awareness is excluded from the skills list: the pi extension exposes
// its memory operations through concise native `memory_*` tools.
// The scripts are still bundled at dist/awareness/scripts/ for tool + hook use.
// octocode (architecture docs) and octocode-stats are excluded as meta-docs/utilities.
const SKIPPED_SKILLS = new Set(['octocode', 'octocode-awareness', 'octocode-stats']);

function isSecretEnvFile(name) {
  return name === '.env' || (name.startsWith('.env.') && name !== '.env.example');
}

function shouldSkipEntry(entry) {
  if (entry.isDirectory()) {
    return SKIPPED_DIRECTORIES.has(entry.name);
  }

  return entry.name === '.DS_Store' || isSecretEnvFile(entry.name);
}

function copyFile(source, target) {
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(source, target);
}

function copyDirectory(sourceDir, targetDir) {
  fs.mkdirSync(targetDir, { recursive: true });

  for (const entry of fs.readdirSync(sourceDir, { withFileTypes: true })) {
    if (shouldSkipEntry(entry)) {
      continue;
    }

    const sourcePath = path.join(sourceDir, entry.name);
    const targetPath = path.join(targetDir, entry.name);
    if (entry.isDirectory()) {
      copyDirectory(sourcePath, targetPath);
    } else if (entry.isFile()) {
      copyFile(sourcePath, targetPath);
    }
  }
}

function assertRequiredSources() {
  const requiredSources = {
    rootSkills: SOURCE_PATHS.rootSkills,
    systemPrompt: SOURCE_PATHS.systemPrompt,
  };

  for (const [label, sourcePath] of Object.entries(requiredSources)) {
    if (!fs.existsSync(sourcePath)) {
      throw new Error(`Missing ${label} source: ${sourcePath}`);
    }
  }
}

function assertNoSecretEnvFiles(targetDir) {
  const violations = [];

  function walk(currentDir) {
    for (const entry of fs.readdirSync(currentDir, { withFileTypes: true })) {
      const entryPath = path.join(currentDir, entry.name);
      if (entry.isDirectory()) {
        walk(entryPath);
      } else if (entry.isFile() && isSecretEnvFile(entry.name)) {
        violations.push(path.relative(targetDir, entryPath));
      }
    }
  }

  walk(targetDir);

  if (violations.length > 0) {
    throw new Error(`Refusing to package secret env files: ${violations.join(', ')}`);
  }
}

/**
 * Copy octocode-config.mjs (the @octocodeai/config source) into every skill's scripts/
 * directory so skills work standalone — no npm publish dependency ever needed.
 * Skill scripts import via: import(new URL('./octocode-config.mjs', import.meta.url).href)
 */
function injectConfigIntoSkills(skillsDir) {
  if (!fs.existsSync(SOURCE_PATHS.configLoader)) {
    throw new Error(`Missing config loader source: ${SOURCE_PATHS.configLoader}`);
  }
  let injected = 0;
  for (const entry of fs.readdirSync(skillsDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const scriptsDir = path.join(skillsDir, entry.name, 'scripts');
    if (fs.existsSync(scriptsDir)) {
      copyFile(SOURCE_PATHS.configLoader, path.join(scriptsDir, 'octocode-config.mjs'));
      injected++;
    }
  }
  return injected;
}

function assertBundledSkills() {
  const skillNames = fs
    .readdirSync(OUTPUT_PATHS.skills, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((skillName) => fs.existsSync(path.join(OUTPUT_PATHS.skills, skillName, 'SKILL.md')))
    .sort();

  if (skillNames.length === 0) {
    throw new Error(`No skills copied to ${OUTPUT_PATHS.skills}`);
  }

  return skillNames;
}

function clean() {
  fs.rmSync(distDir, { recursive: true, force: true });
}

function refreshPackageSkills() {
  fs.rmSync(SOURCE_PATHS.skills, { recursive: true, force: true });
  fs.mkdirSync(SOURCE_PATHS.skills, { recursive: true });
  for (const entry of fs.readdirSync(SOURCE_PATHS.rootSkills, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    if (SKIPPED_SKILLS.has(entry.name)) continue;
    // Only copy skill directories — those that contain a SKILL.md.
    // Non-skill dirs (e.g. scripts/) are skipped to prevent Pi validation noise.
    const src = path.join(SOURCE_PATHS.rootSkills, entry.name);
    if (!fs.existsSync(path.join(src, 'SKILL.md'))) continue;
    copyDirectory(src, path.join(SOURCE_PATHS.skills, entry.name));
  }
  assertNoSecretEnvFiles(SOURCE_PATHS.skills);
}

function bundleAwarenessTools() {
  // Copy canonical scripts (hooks, install, schema, …) from the memory package.
  if (!fs.existsSync(SOURCE_PATHS.awarenessScripts)) {
    throw new Error(`Missing awareness scripts source: ${SOURCE_PATHS.awarenessScripts}`);
  }
  copyDirectory(SOURCE_PATHS.awarenessScripts, OUTPUT_PATHS.awarenessScripts);

  // Copy compiled CLI — same binary the memory package ships and the skill uses.
  copyFile(SOURCE_PATHS.awarenessAwarenessMjs, path.join(OUTPUT_PATHS.awarenessScripts, 'awareness.mjs'));
  copyFile(SOURCE_PATHS.awarenessExtractMjs,   path.join(OUTPUT_PATHS.awarenessScripts, 'extract-hook-files.mjs'));

  // Generate schema.json from the canonical schema.mjs so tool schemas stay aligned.
  const schemas = {};
  for (const name of ['tell_memory', 'get_memory', 'reflect']) {
    const raw = execSync(
      `node ${JSON.stringify(SOURCE_PATHS.awarenessSchemaGen)} json-schema ${name}`,
      { encoding: 'utf8' }
    );
    schemas[name] = JSON.parse(raw);
  }
  fs.mkdirSync(path.dirname(OUTPUT_PATHS.awarenessSchema), { recursive: true });
  fs.writeFileSync(OUTPUT_PATHS.awarenessSchema, JSON.stringify(schemas, null, 2));
  return schemas;
}

function compileTsc() {
  const candidates = [
    path.join(packageRoot, 'node_modules', '.bin', 'tsc'),
    path.join(packageRoot, '..', '..', 'node_modules', '.bin', 'tsc'),
  ];
  const tscBin = candidates.find((p) => fs.existsSync(p));
  if (!tscBin) {
    throw new Error(
      `Could not find tsc binary. Tried:\n${candidates.join('\n')}\nRun: yarn install`,
    );
  }
  execSync(`${JSON.stringify(tscBin)} -p tsconfig.build.json`, {
    stdio: 'inherit',
    cwd: packageRoot,
  });
}

function build() {
  assertRequiredSources();
  refreshPackageSkills();
  clean();

  // 1. Compile TypeScript -> dist/ (generates .js + .d.ts for all src/ modules).
  compileTsc();

  // Inline the @octocodeai/config source AS dist/env.js — index.js imports './env.js', so
  // the published extension carries the loader itself (no runtime dep, nothing to publish).
  // src/env.ts stays a workspace re-export for repo-time (tests, IDE); dist is self-contained.
  copyFile(SOURCE_PATHS.configLoader, path.join(distDir, 'env.js'));
  copyFile(SOURCE_PATHS.systemPrompt, OUTPUT_PATHS.systemPrompt);
  copyDirectory(SOURCE_PATHS.skills, OUTPUT_PATHS.skills);
  // Inject @octocodeai/config source into every skill scripts/ dir — standalone, no npm needed.
  const configInjected = injectConfigIntoSkills(OUTPUT_PATHS.skills);

  const awarenessSchemas = bundleAwarenessTools();
  const schemaNames = Object.keys(awarenessSchemas);
  assertNoSecretEnvFiles(distDir);

  const skillNames = assertBundledSkills();
  console.log(`Built @octocodeai/pi-extension with ${skillNames.length} skills.`);
  console.log(`Skills: ${skillNames.join(', ')}`);
  console.log(`Config loader: octocode-config.mjs injected into ${configInjected} skill script dirs`);
  console.log(`Awareness tools: scripts bundled, schema.json (${schemaNames.join(', ')})`);
}

if (process.argv.includes('--clean')) {
  clean();
} else {
  build();
}
