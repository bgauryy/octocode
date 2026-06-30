#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const packageRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(packageRoot, '../..');
const distDir = path.join(packageRoot, 'dist');

const SOURCE_PATHS = {
  extension: path.join(packageRoot, 'src', 'index.js'),
  rootSkills: path.join(repoRoot, 'skills'),
  skills: path.join(packageRoot, 'skills'),
  systemPrompt: path.join(packageRoot, 'docs', 'PI', 'APPEND_SYSTEM.md'),
};

const OUTPUT_PATHS = {
  extension: path.join(distDir, 'index.js'),
  skills: path.join(distDir, 'skills'),
  systemPrompt: path.join(distDir, 'system', 'APPEND_SYSTEM.md'),
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
  copyDirectory(SOURCE_PATHS.rootSkills, SOURCE_PATHS.skills);
  assertNoSecretEnvFiles(SOURCE_PATHS.skills);
}

function build() {
  assertRequiredSources();
  refreshPackageSkills();
  clean();
  copyFile(SOURCE_PATHS.extension, OUTPUT_PATHS.extension);
  copyFile(SOURCE_PATHS.systemPrompt, OUTPUT_PATHS.systemPrompt);
  copyDirectory(SOURCE_PATHS.skills, OUTPUT_PATHS.skills);
  assertNoSecretEnvFiles(distDir);

  const skillNames = assertBundledSkills();
  console.log(`Built @octocodeai/pi-extension with ${skillNames.length} skills.`);
  console.log(`Skills: ${skillNames.join(', ')}`);
}

if (process.argv.includes('--clean')) {
  clean();
} else {
  build();
}
