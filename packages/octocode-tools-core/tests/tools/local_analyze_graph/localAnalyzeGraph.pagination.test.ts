import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import {
  analyzeGraph,
  summarizeEntrypoints,
} from '../../../src/tools/local_analyze_graph/analyzeGraph.js';
import { executeAnalyzeGraph } from '../../../src/tools/local_analyze_graph/execution.js';
import {
  finalizeGraphOutput,
  paginateGraphResults,
} from '../../../src/tools/local_analyze_graph/pagination.js';
import { LocalAnalyzeGraphQuerySchema } from '../../../src/tools/local_analyze_graph/scheme.js';
import { buildToolResultMeta } from '../../../src/utils/response/bulk/response.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

async function createWideGraphFixture(count = 3): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-graph-page-'));
  tempDirs.push(dir);
  await Promise.all(
    Array.from({ length: count }, (_, index) =>
      writeFile(
        join(dir, `entry-${index}.js`),
        `export const value${index} = ${index};\n`
      )
    )
  );
  return dir;
}

async function createLargeCycleFixture(count = 55): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-graph-large-cycle-'));
  tempDirs.push(dir);
  await Promise.all(
    Array.from({ length: count }, (_, index) =>
      writeFile(
        join(dir, `node-${index}.js`),
        `import './node-${(index + 1) % count}.js';\nexport const value${index} = ${index};\n`
      )
    )
  );
  return dir;
}

function firstRow(result: Awaited<ReturnType<typeof executeAnalyzeGraph>>) {
  return (
    result.structuredContent as {
      results?: Array<{
        meta?: { diagnostics?: { codes?: string[]; partial?: boolean } };
        data?: Record<string, unknown>;
      }>;
    }
  ).results?.[0];
}

describe('localAnalyzeGraph deadCode pagination', () => {
  it('keeps every resolved entrypoint reachable in the structured summary', () => {
    const entrypoints = Array.from(
      { length: 2_735 },
      (_, index) => `src/entry-${index}.ts`
    );

    expect(summarizeEntrypoints(entrypoints)).toEqual({
      entrypointsResolved: entrypoints,
      entrypointsResolvedCount: entrypoints.length,
    });
  });

  it('keeps every file in a large cycle reachable inside its result item', async () => {
    const dir = await createLargeCycleFixture();
    const output = await analyzeGraph({ operation: 'cycles', path: dir });
    const cycle = output.results.find(result => result.size === 55);

    expect(cycle?.files).toHaveLength(55);
    expect(cycle).not.toHaveProperty('truncated');
  });

  it('applies limit before pagination', async () => {
    const dir = await mkdtemp(join(process.cwd(), '.tmp-dead-code-page-'));
    tempDirs.push(dir);
    await writeFile(
      join(dir, 'package.json'),
      JSON.stringify({ name: 'fixture', main: 'index.js' })
    );
    await writeFile(join(dir, 'index.js'), 'export const publicApi = 1;\n');
    await writeFile(
      join(dir, 'dead.js'),
      'export const first = 1;\nexport const second = 2;\nexport const third = 3;\n'
    );

    const result = await analyzeGraph({
      operation: 'deadCode',
      path: dir,
      includeTests: false,
      limit: 2,
      pageSize: 1,
    });

    expect(result.pagination).toMatchObject({
      totalEntries: 2,
      totalPages: 2,
      entriesPerPage: 1,
    });
    expect(result.results).toHaveLength(1);
  });

  it('emits a schema-valid executable next page without auto-filled metadata', async () => {
    const dir = await createWideGraphFixture();
    const result = await analyzeGraph({
      operation: 'reachability',
      path: dir,
      entrypoints: ['entry-0.js'],
      includeTests: false,
      pageSize: 1,
      goal: 'must not leak into continuations',
      reasoning: 'must not leak into continuations',
    } as never);

    expect(result.pagination?.hasMore).toBe(true);
    const continuation = result.next?.nextPage as
      { tool?: string; query?: Record<string, unknown> } | undefined;
    expect(continuation?.tool).toBe('localAnalyzeGraph');
    expect(continuation?.query).not.toHaveProperty('goal');
    expect(continuation?.query).not.toHaveProperty('reasoning');

    const parsed = LocalAnalyzeGraphQuerySchema.safeParse(continuation?.query);
    expect(parsed.success).toBe(true);
    if (!parsed.success) return;

    const nextPage = await analyzeGraph(parsed.data);
    expect(nextPage.pagination).toMatchObject({ currentPage: 2 });
    expect(nextPage.results).toHaveLength(1);
  });

  it('replays every page continuation and preserves the complete duplicate-free result union', async () => {
    const dir = await createWideGraphFixture(7);
    let query = LocalAnalyzeGraphQuerySchema.parse({
      operation: 'reachability',
      path: dir,
      entrypoints: ['entry-0.js'],
      includeTests: false,
      pageSize: 2,
    });
    const files: string[] = [];

    for (;;) {
      const page = await analyzeGraph(query);
      files.push(...page.results.map(result => result.file as string));
      if (!page.pagination?.hasMore) break;

      const continuation = page.next?.nextPage as
        { query?: Record<string, unknown> } | undefined;
      query = LocalAnalyzeGraphQuerySchema.parse(continuation?.query);
    }

    expect(files).toHaveLength(7);
    expect(new Set(files).size).toBe(7);
    expect(files.sort()).toEqual(
      Array.from({ length: 7 }, (_, index) => `entry-${index}.js`)
    );
  });

  it('expands a capped scan, then replays every page without losing files', async () => {
    const dir = await createWideGraphFixture(7);
    let query = LocalAnalyzeGraphQuerySchema.parse({
      operation: 'reachability',
      path: dir,
      entrypoints: ['entry-0.js'],
      includeTests: false,
      maxFiles: 2,
      pageSize: 2,
    });
    let output = await analyzeGraph(query);

    while (output.partialReasons?.includes('maxFiles')) {
      const continuation = output.next?.expandScan as
        { query?: Record<string, unknown> } | undefined;
      query = LocalAnalyzeGraphQuerySchema.parse(continuation?.query);
      output = await analyzeGraph(query);
    }

    const files: string[] = [];
    for (;;) {
      files.push(...output.results.map(result => result.file as string));
      if (!output.pagination?.hasMore) break;

      const continuation = output.next?.nextPage as
        { query?: Record<string, unknown> } | undefined;
      query = LocalAnalyzeGraphQuerySchema.parse(continuation?.query);
      output = await analyzeGraph(query);
    }

    expect(files).toHaveLength(7);
    expect(new Set(files).size).toBe(7);
    expect(files.sort()).toEqual(
      Array.from({ length: 7 }, (_, index) => `entry-${index}.js`)
    );
  });

  it('expands a result limit, then replays every page without losing files', async () => {
    const dir = await createWideGraphFixture(7);
    let query = LocalAnalyzeGraphQuerySchema.parse({
      operation: 'reachability',
      path: dir,
      entrypoints: ['entry-0.js'],
      includeTests: false,
      limit: 2,
      pageSize: 2,
    });
    let output = await analyzeGraph(query);

    while (output.partialReasons?.includes('limit')) {
      const continuation = output.next?.expandLimit as
        { query?: Record<string, unknown> } | undefined;
      query = LocalAnalyzeGraphQuerySchema.parse(continuation?.query);
      output = await analyzeGraph(query);
    }

    const files: string[] = [];
    for (;;) {
      files.push(...output.results.map(result => result.file as string));
      if (!output.pagination?.hasMore) break;

      const continuation = output.next?.nextPage as
        { query?: Record<string, unknown> } | undefined;
      query = LocalAnalyzeGraphQuerySchema.parse(continuation?.query);
      output = await analyzeGraph(query);
    }

    expect(files).toHaveLength(7);
    expect(new Set(files).size).toBe(7);
    expect(files.sort()).toEqual(
      Array.from({ length: 7 }, (_, index) => `entry-${index}.js`)
    );
  });

  it.each(['reachability', 'deadCode'] as const)(
    'surfaces a maxFiles-truncated %s scan through the built executor and provides an executable expansion',
    async operation => {
      const dir = await createWideGraphFixture();
      const result = await executeAnalyzeGraph({
        queries: [
          {
            operation,
            path: dir,
            entrypoints: ['entry-0.js'],
            includeTests: false,
            maxFiles: 2,
            pageSize: 1,
          },
        ],
      });
      const row = firstRow(result);

      expect(row?.meta?.diagnostics?.partial).toBe(true);
      expect(row?.data).toMatchObject({
        truncated: true,
        partialReasons: ['maxFiles'],
      });
      expect(JSON.stringify(row)).not.toContain('warnings');

      const continuation = (
        row?.data?.next as
          | Record<string, { tool?: string; query?: Record<string, unknown> }>
          | undefined
      )?.expandScan;
      expect(continuation?.tool).toBe('localAnalyzeGraph');

      const parsed = LocalAnalyzeGraphQuerySchema.safeParse(
        continuation?.query
      );
      expect(parsed.success).toBe(true);
      if (!parsed.success) return;

      const replay = await executeAnalyzeGraph({ queries: [parsed.data] });
      const replayRow = firstRow(replay);
      expect(replayRow?.data?.truncated).not.toBe(true);
    }
  );

  it('does not surface maxFiles partial state for an exactly-at-cap scan', async () => {
    const dir = await createWideGraphFixture(2);
    const result = await executeAnalyzeGraph({
      queries: [
        {
          operation: 'reachability',
          path: dir,
          entrypoints: ['entry-0.js'],
          includeTests: false,
          maxFiles: 2,
        },
      ],
    });
    const row = firstRow(result);

    expect(row?.data?.truncated).not.toBe(true);
    expect(row?.data?.partialReasons).toBeUndefined();
    expect(row?.data?.next?.expandScan).toBeUndefined();
    expect(row?.meta?.diagnostics?.partial).not.toBe(true);
  });

  it('marks skipped native graph files as an explicit terminal partial result', async () => {
    const dir = await createWideGraphFixture(1);
    await writeFile(join(dir, 'oversized.js'), 'x'.repeat(1_000_001));
    const result = await executeAnalyzeGraph({
      queries: [
        {
          operation: 'reachability',
          path: dir,
          entrypoints: ['entry-0.js'],
          includeTests: false,
          maxFiles: 10,
        },
      ],
    });
    const row = firstRow(result);

    expect(row?.data).toMatchObject({
      filesSkipped: 1,
      truncated: true,
      partialReasons: ['filesSkipped'],
      terminalLimit: true,
    });
    expect(row?.meta?.diagnostics?.partial).toBe(true);
    expect(row?.meta?.diagnostics?.codes).toContain('terminalLimitReached');
    expect(row?.meta?.diagnostics?.codes).not.toContain('continuationMissing');
  });

  it('surfaces limit truncation and provides an executable expansion after the last limited page', async () => {
    const dir = await createWideGraphFixture(5);
    const result = await analyzeGraph({
      operation: 'reachability',
      path: dir,
      entrypoints: ['entry-0.js'],
      includeTests: false,
      limit: 2,
      pageSize: 2,
    });

    expect(result.pagination).toMatchObject({
      hasMore: false,
      totalEntries: 2,
    });
    expect(result).toMatchObject({
      truncated: true,
      partialReasons: ['limit'],
      totalAvailable: 5,
    });

    const continuation = result.next?.expandLimit as
      { query?: Record<string, unknown> } | undefined;
    const parsed = LocalAnalyzeGraphQuerySchema.safeParse(continuation?.query);
    expect(parsed.success).toBe(true);
    if (!parsed.success) return;

    const expanded = await analyzeGraph(parsed.data);
    expect(expanded.pagination?.totalEntries).toBeGreaterThan(2);
  });

  it('keeps an out-of-range request explicit after warnings are stripped', async () => {
    const dir = await createWideGraphFixture();
    const result = await executeAnalyzeGraph({
      queries: [
        {
          operation: 'reachability',
          path: dir,
          entrypoints: ['entry-0.js'],
          includeTests: false,
          page: 99,
          pageSize: 1,
        },
      ],
    });
    const row = firstRow(result);

    expect(row?.data?.pagination).toMatchObject({
      currentPage: 3,
      totalPages: 3,
      outOfRange: true,
    });
    expect(JSON.stringify(row)).not.toContain('warnings');
  });

  it('labels a maxFiles partial scan at the public maximum as terminal', () => {
    const query = {
      operation: 'cycles' as const,
      path: '/repo',
      maxFiles: 50_000,
    };
    const output = finalizeGraphOutput(
      { operation: 'cycles', path: '/repo', results: [] },
      query,
      true,
      'Continue cycles.'
    );
    const meta = buildToolResultMeta('localAnalyzeGraph', query, output);

    expect(output).toMatchObject({
      truncated: true,
      partialReasons: ['maxFiles'],
      terminalLimit: true,
    });
    expect(output.next?.expandScan).toBeUndefined();
    expect(meta.diagnostics?.codes).toContain('terminalLimitReached');
    expect(meta.diagnostics?.codes).not.toContain('continuationMissing');
  });

  it('labels result truncation at the public limit maximum as terminal', () => {
    const query = {
      operation: 'cycles' as const,
      path: '/repo',
      limit: 5_000,
      page: 100,
      pageSize: 50,
    };
    const page = paginateGraphResults(
      Array.from({ length: 5_001 }, (_, index) => ({ index })),
      query
    );
    const output = finalizeGraphOutput(
      { operation: 'cycles', path: '/repo', ...page },
      query,
      false,
      'Continue cycles.'
    );
    const meta = buildToolResultMeta('localAnalyzeGraph', query, output);

    expect(output).toMatchObject({
      truncated: true,
      partialReasons: ['limit'],
      terminalLimit: true,
    });
    expect(output.next?.expandLimit).toBeUndefined();
    expect(meta.diagnostics?.codes).toContain('terminalLimitReached');
    expect(meta.diagnostics?.codes).not.toContain('continuationMissing');
  });

  it('does not advertise schema-invalid page 1001 at the page ceiling', () => {
    const query = {
      operation: 'cycles' as const,
      path: '/repo',
      page: 1_000,
      pageSize: 1,
    };
    const page = paginateGraphResults(
      Array.from({ length: 1_001 }, (_, index) => ({ index })),
      query
    );
    const output = finalizeGraphOutput(
      { operation: 'cycles', path: '/repo', ...page },
      query,
      false,
      'Continue cycles.'
    );
    const meta = buildToolResultMeta('localAnalyzeGraph', query, output);

    expect(output.pagination).toMatchObject({
      currentPage: 1_000,
      hasMore: true,
    });
    expect(output.pagination).not.toHaveProperty('nextPage');
    expect(output.next?.nextPage).toBeUndefined();
    expect(output.terminalLimit).toBe(true);
    expect(meta.diagnostics?.codes).toContain('terminalLimitReached');
    expect(meta.diagnostics?.codes).not.toContain('continuationMissing');
  });

  it('does not emit an unverifiable LSP continuation without a graph root', () => {
    const output = finalizeGraphOutput(
      {
        operation: 'deadCode',
        path: '',
        results: [{ file: 'dead.ts', name: 'unused', line: 1 }],
      },
      { operation: 'deadCode' },
      false,
      'Continue dead-code candidates.'
    );

    expect(output.next?.verifyReferences).toBeUndefined();
  });
});
