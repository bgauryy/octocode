#!/usr/bin/env node
import {
  cpSync,
  existsSync,
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

copyBundledSkills();

function copyBundledSkills() {
  const source = join(repoRoot, 'skills');
  const destination = join(packageRoot, 'skills');

  rmSync(destination, { recursive: true, force: true });
  cpSync(source, destination, { recursive: true });
  removeEnvExamples(destination);
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
