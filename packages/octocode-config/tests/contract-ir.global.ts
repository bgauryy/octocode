// Vitest global setup: core's contract IR is a pure function of core's build and costs seconds, and
// every test file runs in its own worker, so build it once per core build and keep it on disk.
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { TestProject } from 'vitest/node';

declare module 'vitest' {
  export interface ProvidedContext {
    contractIrPath: string;
  }
}

function coreBuildDigest(dist: string): string {
  const hash = createHash('sha256');
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.js')) hash.update(entry.name).update(readFileSync(path));
    }
  };
  walk(dist);
  return hash.digest('hex');
}

export default async function setup(project: TestProject): Promise<void> {
  const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const modules = createRequire(import.meta.url).resolve.paths('@octocodeai/octocode-core') ?? [];
  const core = modules.map(dir => join(dir, '@octocodeai/octocode-core/dist')).find(dir => existsSync(dir));
  if (!core) throw new Error('@octocodeai/octocode-core is not built');
  const cache = join(packageRoot, 'node_modules', '.cache', 'octocode-config');
  const path = join(cache, `contract-ir-${coreBuildDigest(core)}.json`);
  if (!existsSync(path)) {
    const { buildEnforcementContractIr } = await import('@octocodeai/octocode-core/schema');
    mkdirSync(cache, { recursive: true });
    const temporary = `${path}.${process.pid}`;
    writeFileSync(temporary, JSON.stringify(buildEnforcementContractIr()));
    renameSync(temporary, path);
  }
  project.provide('contractIrPath', path);
}
