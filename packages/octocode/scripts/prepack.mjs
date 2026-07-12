#!/usr/bin/env node
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
  unlinkSync,
} from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const packageRoot = join(__dirname, '..');
const repoRoot = join(packageRoot, '..', '..');

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
const SKIPPED_FILES = new Set([
  '.DS_Store',
  'Thumbs.db',
  'npm-debug.log',
  'yarn-error.log',
]);

copyBundledSkills();

function copyBundledSkills() {
  const source = join(repoRoot, 'skills');
  const destination = join(packageRoot, 'skills');

  rmSync(destination, { recursive: true, force: true });
  // Only real skill folders (containing SKILL.md) ship — repo-root skills/
  // Only directories with SKILL.md are treated as shippable skills.
  for (const entry of readdirSync(source, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    if (!existsSync(join(source, entry.name, 'SKILL.md'))) continue;
    copyDirectoryFiltered(
      join(source, entry.name),
      join(destination, entry.name)
    );
  }
  removeEnvExamples(destination);
}

function isHiddenLocalOnlyEntry(name) {
  return name.startsWith('.') && name !== '.env.example';
}

function shouldSkipEntry(entry) {
  return (
    entry.isSymbolicLink() ||
    isHiddenLocalOnlyEntry(entry.name) ||
    SKIPPED_FILES.has(entry.name) ||
    (entry.isDirectory() && SKIPPED_DIRECTORIES.has(entry.name))
  );
}

function copyDirectoryFiltered(sourceDir, targetDir) {
  mkdirSync(targetDir, { recursive: true });

  for (const entry of readdirSync(sourceDir, { withFileTypes: true })) {
    if (shouldSkipEntry(entry)) continue;

    const sourcePath = join(sourceDir, entry.name);
    const targetPath = join(targetDir, entry.name);
    if (entry.isDirectory()) {
      copyDirectoryFiltered(sourcePath, targetPath);
    } else if (entry.isFile()) {
      mkdirSync(dirname(targetPath), { recursive: true });
      copyFileSync(sourcePath, targetPath);
    }
  }
}

function removeEnvExamples(dir) {
  if (!existsSync(dir)) return;

  for (const entry of readdirSync(dir)) {
    const entryPath = join(dir, entry);
    const stats = statSync(entryPath);
    if (stats.isDirectory()) {
      removeEnvExamples(entryPath);
    } else if (entry === '.env.example') {
      unlinkSync(entryPath);
    }
  }
}
