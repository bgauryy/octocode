#!/usr/bin/env node
import { cpSync, existsSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageRoot = join(scriptDir, '..');
const sourceDocs = join(packageRoot, '..', '..', 'docs');
const outputDocs = join(packageRoot, 'out', 'docs');

if (!existsSync(sourceDocs)) {
  throw new Error(`Shared documentation directory is missing: ${sourceDocs}`);
}

rmSync(outputDocs, { recursive: true, force: true });
cpSync(sourceDocs, outputDocs, { recursive: true });
console.log(`✓ CLI docs bundled → ${outputDocs}`);
