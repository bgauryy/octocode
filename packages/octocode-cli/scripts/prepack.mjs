#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
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

verifyCliRuntimeAssets();
copyBundledSkills();

function verifyCliRuntimeAssets() {
  const result = spawnSync(
    process.execPath,
    ['../octocode-mcp/scripts/verify-runtime-assets.mjs', '--dist', 'out'],
    {
      cwd: packageRoot,
      stdio: 'inherit',
    }
  );

  if (result.status !== 0) {
    console.error(
      '\nRefusing to pack octocode-cli without octocode-mcp all-platform runtime assets. ' +
        'Build octocode-mcp with build:publish, then build octocode-cli before packing.'
    );
    process.exit(result.status ?? 1);
  }
}

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
