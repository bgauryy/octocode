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

// Resolve @octocodeai/config source via workspace link — no path hardcoding.
const CONFIG_LOADER_SRC = require.resolve('@octocodeai/config');

const SOURCE_PATHS = {
  // TypeScript source is compiled by tsc (see compileTsc()). Only non-code assets — each is copied flat into dist/.
  // are managed here. @octocodeai/config source injected as octocode-config.mjs into every skill that
  // has a scripts/ directory — zero npm publish dependency for standalone skills.
  configLoader: CONFIG_LOADER_SRC,
  rootSkills: path.join(repoRoot, 'skills'),
  subagents: path.join(packageRoot, 'subagents'),
  skills: path.join(packageRoot, 'skills'),
  systemPrompt: path.join(packageRoot, 'docs', 'PI', 'APPEND_SYSTEM.md'),
  // octocode CLI — bundled at build time so the pi-extension is self-contained.
  // The CLI must be built (yarn workspace octocode build) before building the extension.
  octocodeCLI: path.join(repoRoot, 'packages', 'octocode', 'out'),
};

const OUTPUT_PATHS = {
  extension: path.join(distDir, 'index.js'),
  skills: path.join(distDir, 'skills'),
  subagents: path.join(distDir, 'subagents'),
  systemPrompt: path.join(distDir, 'system', 'APPEND_SYSTEM.md'),
  // bundled octocode CLI — agent uses: node $OCTOCODE_CLI <command>
  cli: path.join(distDir, 'cli'),
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

// Skills excluded from root/skills/ → packages/octocode-pi-extension/skills/ sync:
//   octocode-awareness — owned by @octocodeai/octocode-awareness; consumed by direct import, not skill copy.
//   octocode / octocode-stats — architecture docs and utilities, not user-facing skills.
//   browser-agent — canonical source lives in packages/octocode-pi-extension/skills/browser-agent/;
//                   it is NOT sourced from root/skills/ so refreshPackageSkills must not overwrite it.
const SKIPPED_SKILLS = new Set(['octocode', 'octocode-awareness', 'octocode-stats', 'browser-agent']);

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

/**
 * Bundle the octocode CLI into dist/cli/.
 *
 * Copies the pre-built packages/octocode/out/ directory wholesale.
 * The CLI is self-contained JS (rollup bundles); native engine addons are
 * resolved from the pi-extension's own node_modules at runtime.
 *
 * Prerequisite: `yarn workspace octocode build` must run before this step.
 */
function bundleOctocodeCLI() {
  const src = SOURCE_PATHS.octocodeCLI;
  const dest = OUTPUT_PATHS.cli;

  if (!fs.existsSync(src)) {
    throw new Error(
      `octocode CLI output not found at ${src}.\n` +
      `Run: yarn workspace octocode build   (then rebuild the pi-extension)`,
    );
  }

  const entry = path.join(src, 'octocode.js');
  if (!fs.existsSync(entry)) {
    throw new Error(
      `octocode.js not found in ${src}. The CLI may not have been built yet.\n` +
      `Run: yarn workspace octocode build`,
    );
  }

  copyDirectory(src, dest);
  console.log(`Octocode CLI bundled: ${dest}/octocode.js`);
  return dest;
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
  // Copy subagents/ to dist/subagents/ (SYSTEM_PROMPT.md files loaded at runtime)
  if (fs.existsSync(SOURCE_PATHS.subagents)) {
    copyDirectory(SOURCE_PATHS.subagents, OUTPUT_PATHS.subagents);
  }
  // Inject @octocodeai/config source into every skill scripts/ dir — standalone, no npm needed.
  const configInjected = injectConfigIntoSkills(OUTPUT_PATHS.skills);

  bundleOctocodeCLI();

  assertNoSecretEnvFiles(distDir);

  const skillNames = assertBundledSkills();
  console.log(`Built @octocodeai/pi-extension with ${skillNames.length} skills.`);
  console.log(`Skills: ${skillNames.join(', ')}`);
  console.log(`Config loader: octocode-config.mjs injected into ${configInjected} skill script dirs`);
  console.log('Awareness: consumed via @octocodeai/octocode-awareness direct import');
}

if (process.argv.includes('--clean')) {
  clean();
} else {
  build();
}
