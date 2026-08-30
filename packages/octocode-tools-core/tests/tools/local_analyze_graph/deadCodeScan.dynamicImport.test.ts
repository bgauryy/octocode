import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { scanForDeadCode } from '../../../src/tools/local_analyze_graph/deadCodeScan.js';

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
      result.warnings.some(
        w => w.includes('dynamic import') && w.includes('plugin.js')
      )
    ).toBe(true);
  });

  it('retains named re-exports exposed through a dynamically imported barrel', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'entry.js'),
      "export async function main() {\n  const mod = await import('./barrel.js');\n  return mod.used();\n}\n"
    );
    await writeFile(
      join(dir, 'barrel.js'),
      "export { used } from './leaf.js';\n"
    );
    await writeFile(
      join(dir, 'leaf.js'),
      'export function used() { return 1; }\nexport function trulyDead() { return 2; }\n'
    );

    const result = scanForDeadCode(dir, { entrypoints: ['entry.js'] });
    const names = deadNames(result);

    expect(names).not.toContain('leaf.js::used');
    expect(names).toContain('leaf.js::trulyDead');
  });

  it('omits test-file candidates when tests are not included', async () => {
    const dir = await createTempDir();
    await writeFile(join(dir, 'entry.js'), 'export const live = true;\n');
    await writeFile(
      join(dir, 'unused.test.js'),
      'export function testHelper() { return 1; }\n'
    );

    const result = scanForDeadCode(dir, {
      entrypoints: ['entry.js'],
      includeTests: false,
    });

    expect(deadNames(result)).not.toContain('unused.test.js::testHelper');
    expect(result.deadClusters.flatMap(cluster => cluster.files)).not.toContain(
      'unused.test.js'
    );
  });
});
