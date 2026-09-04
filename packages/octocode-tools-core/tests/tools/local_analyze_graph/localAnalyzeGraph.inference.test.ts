/**
 * TDD tests for localAnalyzeGraph path-inference.
 *
 * Failure contract before the fix:
 *   - Zod rejects `{ operation:'dependencies', file:'/abs/...' }` because path
 *     is required  →  fixed by making path optional in commonFields.
 *   - validateToolPath fires before analyzeGraph, short-circuits with
 *     "path is required" even after Zod accepts the query  →  fixed by adding
 *     inferPathIfMissing() in execution.ts.
 *   - normalizeGraphFile() only normalises separators; it never strips the root
 *     prefix, so `built.fileGraph.has('/abs/.../src/index.js')` is always false
 *     for absolute file paths  →  fixed by resolveFileForGraph() using
 *     path.relative(rootPath, absFile) before normalising.
 */
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';
import { executeAnalyzeGraph } from '../../../src/tools/local_analyze_graph/execution.js';
import { LocalAnalyzeGraphQuerySchema } from '../../../src/tools/local_analyze_graph/scheme.js';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

/**
 * Creates:
 *   <dir>/package.json
 *   <dir>/src/index.js   (imports ./a.js)
 *   <dir>/src/a.js       (leaf export)
 *
 * Returns absolute paths to each file.
 */
async function createInferenceFixture(): Promise<{
  dir: string;
  indexFile: string;
  aFile: string;
}> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-local-graph-infer-'));
  tempDirs.push(dir);
  await mkdir(join(dir, 'src'), { recursive: true });
  await writeFile(
    join(dir, 'package.json'),
    JSON.stringify({ name: 'infer-fixture', main: 'src/index.js' })
  );
  const indexFile = join(dir, 'src', 'index.js');
  const aFile = join(dir, 'src', 'a.js');
  await writeFile(
    indexFile,
    "import { a } from './a.js';\nexport const api = a();\n"
  );
  await writeFile(aFile, 'export function a() { return 1; }\n');
  return { dir, indexFile, aFile };
}

// ---------------------------------------------------------------------------
// 1. Zod schema — path is now optional
// ---------------------------------------------------------------------------

describe('LocalAnalyzeGraphQuerySchema — optional path', () => {
  it('accepts dependencies with absolute file and no path', () => {
    const r = LocalAnalyzeGraphQuerySchema.safeParse({
      operation: 'dependencies',
      file: '/abs/path/to/src/index.ts',
    });
    expect(r.success).toBe(true);
  });

  it('accepts dependents with absolute file and no path', () => {
    const r = LocalAnalyzeGraphQuerySchema.safeParse({
      operation: 'dependents',
      file: '/abs/path/to/src/a.ts',
    });
    expect(r.success).toBe(true);
  });

  it('accepts path-op with absolute file + target and no path', () => {
    const r = LocalAnalyzeGraphQuerySchema.safeParse({
      operation: 'path',
      file: '/abs/src/index.ts',
      target: '/abs/src/a.ts',
    });
    expect(r.success).toBe(true);
  });

  it('still accepts when explicit path is provided', () => {
    const r = LocalAnalyzeGraphQuerySchema.safeParse({
      operation: 'dependencies',
      path: '/abs/repo',
      file: 'src/index.ts',
    });
    expect(r.success).toBe(true);
  });

  it('still rejects dependencies when file is also missing', () => {
    const r = LocalAnalyzeGraphQuerySchema.safeParse({
      operation: 'dependencies',
      // no path, no file
    });
    // file is required for dependencies in the discriminated union
    expect(r.success).toBe(false);
  });

  it('accepts deadCode with neither path nor file (path is schema-optional; runtime will infer or fail)', () => {
    const r = LocalAnalyzeGraphQuerySchema.safeParse({
      operation: 'deadCode',
    });
    // Zod accepts it; validateToolPath will fail at runtime if path can't be inferred
    expect(r.success).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// 2. analyzeGraph() direct — impl-level inference + resolveFileForGraph
// ---------------------------------------------------------------------------

describe('analyzeGraph() — path inference from absolute file', () => {
  it('infers root and resolves absolute file for dependencies', async () => {
    const { indexFile } = await createInferenceFixture();

    const result = await analyzeGraph({
      operation: 'dependencies',
      file: indexFile, // absolute path, no explicit path
    });

    expect(result.status).not.toBe('error');
    expect(result.results).toEqual(
      expect.arrayContaining([expect.objectContaining({ file: 'src/a.js' })])
    );
  });

  it('infers root and resolves absolute file for dependents', async () => {
    const { aFile } = await createInferenceFixture();

    const result = await analyzeGraph({
      operation: 'dependents',
      file: aFile,
    });

    expect(result.status).not.toBe('error');
    expect(result.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ file: 'src/index.js' }),
      ])
    );
  });

  it('infers root from absolute file for path operation', async () => {
    const { indexFile, aFile } = await createInferenceFixture();

    const result = await analyzeGraph({
      operation: 'path',
      file: indexFile,
      target: aFile,
    });

    expect(result.status).not.toBe('error');
  });

  it('still works: explicit path + relative file (existing contract)', async () => {
    const { dir } = await createInferenceFixture();

    const result = await analyzeGraph({
      operation: 'dependencies',
      path: dir,
      file: 'src/index.js',
    });

    expect(result.status).not.toBe('error');
    expect(result.results).toEqual(
      expect.arrayContaining([expect.objectContaining({ file: 'src/a.js' })])
    );
  });

  it('still works: explicit path + absolute file (both provided)', async () => {
    const { dir, indexFile } = await createInferenceFixture();

    const result = await analyzeGraph({
      operation: 'dependencies',
      path: dir,
      file: indexFile, // absolute but path is also explicit
    });

    expect(result.status).not.toBe('error');
    expect(result.results).toEqual(
      expect.arrayContaining([expect.objectContaining({ file: 'src/a.js' })])
    );
  });

  it('returns a clear error when file is relative and path is omitted', async () => {
    const result = await analyzeGraph({
      operation: 'dependencies',
      file: 'src/relative.ts', // relative — cannot infer root
    });

    expect(result.status).toBe('error');
    expect(result.error).toMatch(/path is required/i);
  });
});

// ---------------------------------------------------------------------------
// 3. executeAnalyzeGraph() — full stack (Zod + validateToolPath + analyzeGraph)
// ---------------------------------------------------------------------------

function firstRow(result: Awaited<ReturnType<typeof executeAnalyzeGraph>>) {
  return (
    result.structuredContent as {
      results?: Array<{
        data?: Record<string, unknown>;
        status?: string;
      }>;
    }
  ).results?.[0];
}

describe('executeAnalyzeGraph() — full-stack path inference', () => {
  it('infers path from absolute file for dependencies', async () => {
    const { indexFile } = await createInferenceFixture();

    const result = await executeAnalyzeGraph({
      queries: [{ operation: 'dependencies', file: indexFile }],
    });

    expect(result.isError).toBeFalsy();
    const row = firstRow(result);
    expect(row?.data?.filesScanned).toBeGreaterThan(0);
    expect(row?.data?.results).toEqual(
      expect.arrayContaining([expect.objectContaining({ file: 'src/a.js' })])
    );
  });

  it('infers path from absolute file for dependents', async () => {
    const { aFile } = await createInferenceFixture();

    const result = await executeAnalyzeGraph({
      queries: [{ operation: 'dependents', file: aFile }],
    });

    expect(result.isError).toBeFalsy();
    const row = firstRow(result);
    expect(row?.data?.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ file: 'src/index.js' }),
      ])
    );
  });

  it('fails with error when relative file is given with no path', async () => {
    const result = await executeAnalyzeGraph({
      queries: [{ operation: 'dependencies', file: 'src/relative.ts' }],
    });

    // validateToolPath (or impl-level) must return an error result
    expect(result.isError).toBe(true);
  });

  it('explicit path + relative file still works (no regression)', async () => {
    const { dir } = await createInferenceFixture();

    const result = await executeAnalyzeGraph({
      queries: [{ operation: 'dependencies', path: dir, file: 'src/index.js' }],
    });

    expect(result.isError).toBeFalsy();
    const row = firstRow(result);
    expect(row?.data?.filesScanned).toBeGreaterThan(0);
  });

  it('explicit path + absolute file still works (no regression)', async () => {
    const { dir, indexFile } = await createInferenceFixture();

    const result = await executeAnalyzeGraph({
      queries: [{ operation: 'dependencies', path: dir, file: indexFile }],
    });

    expect(result.isError).toBeFalsy();
    const row = firstRow(result);
    expect(row?.data?.filesScanned).toBeGreaterThan(0);
  });
});
