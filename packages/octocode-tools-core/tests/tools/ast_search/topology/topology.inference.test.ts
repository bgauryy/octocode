/**
 * TDD tests for astSearch topology path-inference.
 *
 * Failure contract before the fix:
 *   - Zod rejects `{ operation:'dependencies', file:'/abs/...' }` because path
 *     is required  →  fixed by making path optional in commonFields.
 *   - validateToolPath fires before analyzeTopology, short-circuits with
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

import { analyzeTopology } from '../../../../src/tools/ast_search/topology/analyzeTopology.js';
import { GraphAnalysisQuerySchema } from '../../../../src/tools/ast_search/topology/scheme.js';
import { runPublicTopology } from './publicAdapter.js';

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

describe('GraphAnalysisQuerySchema — optional path', () => {
  it('accepts dependencies with absolute file and no path', () => {
    const r = GraphAnalysisQuerySchema.safeParse({
      operation: 'dependencies',
      file: '/abs/path/to/src/index.ts',
    });
    expect(r.success).toBe(true);
  });

  it('accepts dependents with absolute file and no path', () => {
    const r = GraphAnalysisQuerySchema.safeParse({
      operation: 'dependents',
      file: '/abs/path/to/src/a.ts',
    });
    expect(r.success).toBe(true);
  });

  it('accepts path-op with absolute file + target and no path', () => {
    const r = GraphAnalysisQuerySchema.safeParse({
      operation: 'path',
      file: '/abs/src/index.ts',
      target: '/abs/src/a.ts',
    });
    expect(r.success).toBe(true);
  });

  it('still accepts when explicit path is provided', () => {
    const r = GraphAnalysisQuerySchema.safeParse({
      operation: 'dependencies',
      path: '/abs/repo',
      file: 'src/index.ts',
    });
    expect(r.success).toBe(true);
  });

  it('still rejects dependencies when file is also missing', () => {
    const r = GraphAnalysisQuerySchema.safeParse({
      operation: 'dependencies',
      // no path, no file
    });
    // file is required for dependencies in the discriminated union
    expect(r.success).toBe(false);
  });

  it('rejects deadCode when no absolute root source can be inferred', () => {
    const r = GraphAnalysisQuerySchema.safeParse({
      operation: 'deadCode',
    });
    expect(r.success).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// 2. analyzeTopology() direct — impl-level inference + resolveFileForGraph
// ---------------------------------------------------------------------------

describe('analyzeTopology() — path inference from absolute file', () => {
  it('infers root and resolves absolute file for dependencies', async () => {
    const { indexFile } = await createInferenceFixture();

    const result = await analyzeTopology({
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

    const result = await analyzeTopology({
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

    const result = await analyzeTopology({
      operation: 'path',
      file: indexFile,
      target: aFile,
    });

    expect(result.status).not.toBe('error');
  });

  it('infers root from an absolute target when the source file is relative', async () => {
    const { aFile } = await createInferenceFixture();

    const result = await analyzeTopology({
      operation: 'path',
      file: 'src/index.js',
      target: aFile,
    });

    expect(result.status).not.toBe('error');
    expect(result.results?.[0]?.files).toEqual(['src/index.js', 'src/a.js']);
  });

  it('still works: explicit path + relative file (existing contract)', async () => {
    const { dir } = await createInferenceFixture();

    const result = await analyzeTopology({
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

    const result = await analyzeTopology({
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
    const result = await analyzeTopology({
      operation: 'dependencies',
      file: 'src/relative.ts', // relative — cannot infer root
    });

    expect(result.status).toBe('error');
    expect(result.error).toMatch(/path is required/i);
  });
});

// ---------------------------------------------------------------------------
// 3. runPublicTopology() — public astSearch topology path
// ---------------------------------------------------------------------------

function firstRow(result: Awaited<ReturnType<typeof runPublicTopology>>) {
  return (
    result.structuredContent as {
      results?: Array<{
        data?: Record<string, unknown>;
        status?: string;
      }>;
    }
  ).results?.[0];
}

describe('runPublicTopology() — public topology path inference', () => {
  it('infers path from absolute file for dependencies', async () => {
    const { indexFile } = await createInferenceFixture();

    const result = await runPublicTopology({
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

    const result = await runPublicTopology({
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

  it('rejects a relative file with no inferable public root', async () => {
    await expect(
      runPublicTopology({
        queries: [{ operation: 'dependencies', file: 'src/relative.ts' }],
      })
    ).rejects.toThrow(/path is required/i);
  });

  it('explicit path + relative file still works (no regression)', async () => {
    const { dir } = await createInferenceFixture();

    const result = await runPublicTopology({
      queries: [{ operation: 'dependencies', path: dir, file: 'src/index.js' }],
    });

    expect(result.isError).toBeFalsy();
    const row = firstRow(result);
    expect(row?.data?.filesScanned).toBeGreaterThan(0);
  });

  it('explicit path + absolute file still works (no regression)', async () => {
    const { dir, indexFile } = await createInferenceFixture();

    const result = await runPublicTopology({
      queries: [{ operation: 'dependencies', path: dir, file: indexFile }],
    });

    expect(result.isError).toBeFalsy();
    const row = firstRow(result);
    expect(row?.data?.filesScanned).toBeGreaterThan(0);
  });
});
