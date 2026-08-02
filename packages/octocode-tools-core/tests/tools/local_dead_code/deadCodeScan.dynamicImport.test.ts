import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { scanForDeadCode } from '../../../src/tools/local_dead_code/deadCodeScan.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

async function createTempDir(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-dead-code-dynamic-'));
  tempDirs.push(dir);
  return dir;
}

function deadNames(result: { deadExports: { file: string; name: string }[] }) {
  return result.deadExports.map(d => `${d.file}::${d.name}`);
}

describe('scanForDeadCode dynamic import reachability', () => {
  it('does not report a file reached only via a string-literal dynamic import as dead', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'entry.js'),
      "export async function main() {\n  const mod = await import('./plugin.js');\n  return mod;\n}\n"
    );
    await writeFile(
      join(dir, 'plugin.js'),
      'export function run() {\n  return 1;\n}\n'
    );

    const result = scanForDeadCode(dir, { entrypoints: ['entry.js'] });

    expect(result.entrypointsResolved).toEqual(['entry.js']);
    expect(deadNames(result)).not.toContain('plugin.js::run');
  });

  it('does not silently mark a file behind a computed dynamic import specifier as reachable', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'entry.js'),
      'export async function main(name) {\n  return await import(name);\n}\n'
    );
    await writeFile(
      join(dir, 'orphan.js'),
      'export function unused() {\n  return 1;\n}\n'
    );

    const result = scanForDeadCode(dir, { entrypoints: ['entry.js'] });

    // orphan.js is genuinely unreachable (nothing imports it, static or
    // dynamic) — a computed specifier must not accidentally suppress this by
    // being treated as a match-anything wildcard.
    expect(deadNames(result)).toContain('orphan.js::unused');
  });

  it('warns when a file is reachable only through a dynamic import, not through any static path', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'entry.js'),
      "export async function main() {\n  const mod = await import('./plugin.js');\n  return mod;\n}\n"
    );
    await writeFile(
      join(dir, 'plugin.js'),
      'export function run() {\n  return 1;\n}\n'
    );

    const result = scanForDeadCode(dir, { entrypoints: ['entry.js'] });

    expect(
      result.warnings.some(w => w.includes('dynamic import') && w.includes('plugin.js'))
    ).toBe(true);
  });
});
