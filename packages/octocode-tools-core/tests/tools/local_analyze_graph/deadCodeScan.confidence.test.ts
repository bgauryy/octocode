import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { scanForDeadCode } from '../../../src/tools/local_analyze_graph/deadCodeScan.js';
import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

async function createTempDir(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-dead-code-confidence-'));
  tempDirs.push(dir);
  return dir;
}

describe('scanForDeadCode entrypoint confidence signal', () => {
  it('flags confidence:"low" when entrypoints resolve only from the test-file heuristic', async () => {
    const dir = await createTempDir();
    // No package.json at all — nothing for the manifest-derived path to find.
    await writeFile(
      join(dir, 'foo.js'),
      'export function helper() {\n  return 1;\n}\n'
    );
    await writeFile(
      join(dir, 'foo.test.js'),
      "import { helper } from './foo.js';\nhelper();\n"
    );

    const result = await scanForDeadCode(dir, {});

    expect(result.entrypointsResolved).toEqual(['foo.test.js']);
    expect(result.confidence).toBe('low');
    expect(
      result.warnings.some(w => w.includes('no entrypoints resolved'))
    ).toBe(true);
  });

  it('does not flag low confidence when a real package.json main entrypoint resolves', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'foo.js' })
    );
    await writeFile(
      join(dir, 'foo.js'),
      'export function helper() {\n  return 1;\n}\n'
    );
    await writeFile(
      join(dir, 'foo.test.js'),
      "import { helper } from './foo.js';\nhelper();\n"
    );

    const result = await scanForDeadCode(dir, {});

    expect(result.entrypointsResolved).toContain('foo.js');
    expect(result.confidence).toBeUndefined();
  });

  it('propagates confidence:"low" through localAnalyzeGraph — the actual tool entry point the CLI/MCP call, not just scanForDeadCode', async () => {
    // Regression: scanForDeadCode's result was correct, but the public graph
    // operation rebuilds its output object
    // field-by-field and silently dropped `confidence` in the process — a
    // test against scanForDeadCode alone could not catch this.
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'foo.js'),
      'export function helper() {\n  return 1;\n}\n'
    );
    await writeFile(
      join(dir, 'foo.test.js'),
      "import { helper } from './foo.js';\nhelper();\n"
    );

    const output = await analyzeGraph({ operation: 'deadCode', path: dir });

    expect(output.summary?.entrypointsResolved).toEqual(['foo.test.js']);
    expect(output.confidence).toBe('low');
  });

  it('localAnalyzeGraph labels a clamped out-of-range page honestly', async () => {
    // paginate() clamps internally to the last real page, but the envelope
    // used to echo the REQUESTED page — page:99 of a 1-page result returned
    // page-1 items labeled currentPage:99, silently mislabeled.
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'a.js'),
      'export function one() {}\nexport function two() {}\n'
    );

    const output = await analyzeGraph({
      operation: 'deadCode',
      path: dir,
      entrypoints: ['a.js'],
      includeTests: false,
      page: 99,
      itemsPerPage: 5,
    });

    const pagination = output.pagination as {
      currentPage: number;
      totalPages: number;
      outOfRange?: boolean;
    };
    expect(pagination.currentPage).toBe(pagination.totalPages);
    expect(pagination.currentPage).not.toBe(99);
    expect(pagination.outOfRange).toBe(true);
    expect(
      (output.warnings ?? []).some((w: string) => w.includes('page'))
    ).toBe(true);
  });

  it('does not flag low confidence when entrypoints are given explicitly', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'foo.js'),
      'export function helper() {\n  return 1;\n}\n'
    );

    const result = await scanForDeadCode(dir, {
      entrypoints: ['foo.js'],
      includeTests: false,
    });

    expect(result.entrypointsResolved).toEqual(['foo.js']);
    expect(result.confidence).toBeUndefined();
  });

  it('accepts a directory as a dynamic asset root', async () => {
    const dir = await createTempDir();
    await writeFile(join(dir, 'entry.js'), "export const live = true;\n");
    await mkdir(join(dir, 'agents', 'nested'), { recursive: true });
    await writeFile(join(dir, 'agents', 'worker.mjs'), "export const worker = true;\n");
    await writeFile(join(dir, 'agents', 'nested', 'helper.mjs'), "export const helper = true;\n");

    const result = await scanForDeadCode(dir, {
      entrypoints: ['entry.js', 'agents'],
      includeTests: false,
    });

    expect(result.entrypointsResolved.sort()).toEqual([
      'agents/nested/helper.mjs',
      'agents/worker.mjs',
      'entry.js',
    ]);
    expect(result.warnings).toEqual([]);
  });

  it('accepts an ABSOLUTE entrypoint path (the form every other local tool requires)', async () => {
    // Regression for the benchmark-found trap: absolute entrypoint paths were
    // silently dropped ("entrypoint not found in scan") and the scan degraded
    // to a no-entrypoint candidate flood.
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'foo.js'),
      'export function helper() {\n  return 1;\n}\n'
    );

    const result = await scanForDeadCode(dir, {
      entrypoints: [join(dir, 'foo.js')],
      includeTests: false,
    });

    expect(result.entrypointsResolved).toEqual(['foo.js']);
    expect(result.warnings).toEqual([]);
  });

  it('names the accepted path form when an entrypoint cannot be resolved', async () => {
    const dir = await createTempDir();
    await writeFile(
      join(dir, 'foo.js'),
      'export function helper() {\n  return 1;\n}\n'
    );

    const result = await scanForDeadCode(dir, {
      entrypoints: ['does/not/exist.js'],
      includeTests: false,
    });

    expect(
      result.warnings.some(w => w.includes('relative to the scanned path'))
    ).toBe(true);
  });
});
