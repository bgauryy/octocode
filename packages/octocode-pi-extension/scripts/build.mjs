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

function resolveOctocodeOutDir() {
  const pkgJsonPath = require.resolve('octocode/package.json');
  return path.join(path.dirname(pkgJsonPath), 'out');
}

// Resolve @octocodeai/config source via workspace link — no path hardcoding.
const CONFIG_LOADER_SRC = require.resolve('@octocodeai/config');

const SOURCE_PATHS = {
  extension: path.join(packageRoot, 'src', 'index.js'),
  // extension source modules imported by index.js — each is copied flat into dist/.
  extensionModules: [
    path.join(packageRoot, 'src', 'web.js'),
  ],
  // @octocodeai/config source injected as octocode-config.mjs into every skill that
  // has a scripts/ directory — zero npm publish dependency for standalone skills.
  configLoader: CONFIG_LOADER_SRC,
  rootSkills: path.join(repoRoot, 'skills'),
  skills: path.join(packageRoot, 'skills'),
  systemPrompt: path.join(packageRoot, 'docs', 'PI', 'APPEND_SYSTEM.md'),
  // awareness scripts are bundled separately from skills (tools use them directly)
  awarenessScripts: path.join(repoRoot, 'skills', 'octocode-awareness', 'scripts'),
  awarenessSchemaGen: path.join(repoRoot, 'skills', 'octocode-awareness', 'scripts', 'schema.mjs'),
};

const OUTPUT_PATHS = {
  extension: path.join(distDir, 'index.js'),
  skills: path.join(distDir, 'skills'),
  systemPrompt: path.join(distDir, 'system', 'APPEND_SYSTEM.md'),
  bin: path.join(distDir, 'bin'),
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
// its operations as native tools (memory_record / memory_recall / memory_reflect).
// The scripts are still bundled at dist/awareness/scripts/ for tool + hook use.
const SKIPPED_SKILLS = new Set(['octocode', 'octocode-stats', 'octocode-awareness']);

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
    extension: SOURCE_PATHS.extension,
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

function assertBundledBin() {
  const entry = path.join(OUTPUT_PATHS.bin, 'octocode.js');
  if (!fs.existsSync(entry)) {
    throw new Error(`Missing bundled Octocode CLI entry: ${entry}`);
  }
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
    if (entry.isDirectory() && SKIPPED_SKILLS.has(entry.name)) continue;
    // Only copy skill directories — root-level files (e.g. README.md) are not skills
    // and would trigger validation errors when Pi scans the skills directory.
    if (!entry.isDirectory()) continue;
    const src = path.join(SOURCE_PATHS.rootSkills, entry.name);
    const dst = path.join(SOURCE_PATHS.skills, entry.name);
    copyDirectory(src, dst);
  }
  assertNoSecretEnvFiles(SOURCE_PATHS.skills);
}

function bundleAwarenessTools() {
  // Copy awareness scripts to dist/awareness/scripts/ (used by memory_* tools + file-lock hooks).
  if (!fs.existsSync(SOURCE_PATHS.awarenessScripts)) {
    throw new Error(`Missing awareness scripts source: ${SOURCE_PATHS.awarenessScripts}`);
  }
  copyDirectory(SOURCE_PATHS.awarenessScripts, OUTPUT_PATHS.awarenessScripts);

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

function build() {
  assertRequiredSources();
  refreshPackageSkills();
  clean();
  copyFile(SOURCE_PATHS.extension, OUTPUT_PATHS.extension);
  for (const modulePath of SOURCE_PATHS.extensionModules) {
    copyFile(modulePath, path.join(distDir, path.basename(modulePath)));
  }
  // Inline the @octocodeai/config source AS dist/env.js — index.js imports './env.js', so
  // the published extension carries the loader itself (no runtime dep, nothing to publish).
  // src/env.js stays a workspace re-export for repo-time (tests); dist is self-contained.
  copyFile(SOURCE_PATHS.configLoader, path.join(distDir, 'env.js'));
  copyFile(SOURCE_PATHS.systemPrompt, OUTPUT_PATHS.systemPrompt);
  copyDirectory(SOURCE_PATHS.skills, OUTPUT_PATHS.skills);
  // Inject @octocodeai/config source into every skill scripts/ dir — standalone, no npm needed.
  const configInjected = injectConfigIntoSkills(OUTPUT_PATHS.skills);

  const octocodeOutDir = resolveOctocodeOutDir();
  copyDirectory(octocodeOutDir, OUTPUT_PATHS.bin);
  assertBundledBin();

  const awarenessSchemas = bundleAwarenessTools();
  const schemaNames = Object.keys(awarenessSchemas);

  assertNoSecretEnvFiles(distDir);

  const skillNames = assertBundledSkills();
  const binFiles = fs.readdirSync(OUTPUT_PATHS.bin).length;
  console.log(`Built @octocodeai/pi-extension with ${skillNames.length} skills.`);
  console.log(`Skills: ${skillNames.join(', ')}`);
  console.log(`Config loader: octocode-config.mjs injected into ${configInjected} skill script dirs`);
  console.log(`Bundled Octocode CLI: ${binFiles} entries in dist/bin/`);
  console.log(`Awareness tools: scripts bundled, schema.json (${schemaNames.join(', ')})`);
}

if (process.argv.includes('--clean')) {
  clean();
} else {
  build();
}
