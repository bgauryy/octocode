#!/usr/bin/env node
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
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
