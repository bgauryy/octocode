import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';
import { executeAnalyzeGraph } from '../../../src/tools/local_analyze_graph/execution.js';
import { contextUtils } from '../../../src/utils/contextUtils.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

async function createGraphFixture(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-local-graph-'));
  tempDirs.push(dir);
  await writeFile(
    join(dir, 'package.json'),
    JSON.stringify({ name: 'fixture', main: 'index.js' })
  );
  await writeFile(
    join(dir, 'index.js'),
    "import { a } from './a.js';\nexport const api = a();\n"
  );
  await writeFile(
    join(dir, 'a.js'),
    "import { b } from './b.js';\nexport function a() { return b(); }\n"
  );
  await writeFile(
    join(dir, 'b.js'),
    "import { a } from './a.js';\nexport function b() { return 1; }\n"
  );
  await writeFile(
    join(dir, 'orphan.js'),
    "import { dead } from './dead.js';\nexport function orphan() { return dead(); }\n"
  );
  await writeFile(
    join(dir, 'dead.js'),
    "import { orphan } from './orphan.js';\nexport function dead() { return orphan(); }\n"
  );
  return dir;
}

async function createWideGraphFixture(count = 55): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-local-graph-wide-'));
  tempDirs.push(dir);
  await writeFile(join(dir, 'package.json'), JSON.stringify({ name: 'wide' }));
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

async function createProvenanceFixture(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-local-graph-edges-'));
  tempDirs.push(dir);
  await writeFile(join(dir, 'package.json'), JSON.stringify({ name: 'edges' }));
  await writeFile(
    join(dir, 'index.js'),
    [
      "import './static.js';",
      "export { named } from './named.js';",
      "export * from './star.js';",
      "export const load = () => import('./dynamic.js');",
      '',
    ].join('\n')
  );
  await Promise.all(
    ['static', 'named', 'star', 'dynamic'].map(name =>
      writeFile(join(dir, `${name}.js`), `export const ${name} = true;\n`)
    )
  );
  return dir;
}

async function createWorkspacePackageFixture(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-local-graph-workspace-'));
  tempDirs.push(dir);
  const library = join(dir, 'packages', 'library');
  const consumer = join(dir, 'packages', 'consumer');
  await Promise.all([
    mkdir(join(library, 'src'), { recursive: true }),
    mkdir(join(consumer, 'src'), { recursive: true }),
  ]);
  await writeFile(
    join(dir, 'package.json'),
    JSON.stringify({ name: 'workspace', workspaces: ['packages/*'] })
  );
  await writeFile(
    join(library, 'package.json'),
    JSON.stringify({
      name: '@fixture/library',
      exports: {
        '.': './dist/index.js',
        './schema': './dist/schema.js',
      },
    })
  );
  await writeFile(
    join(consumer, 'package.json'),
    JSON.stringify({ name: '@fixture/consumer' })
  );
  await writeFile(
    join(library, 'src', 'index.ts'),
    "export { schema } from './schema.js';\n"
  );
  await writeFile(
    join(library, 'src', 'schema.ts'),
    "export const schema = 'workspace-export';\n"
  );
  await writeFile(
    join(consumer, 'src', 'main.ts'),
    [
      "import { schema } from '@fixture/library/schema';",
      "import { alias } from '@fixture-alias/schema';",
      'export const value = schema + String(alias);',
      '',
    ].join('\n')
  );
  return dir;
}

async function createEdgePrecisionFixture(): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-local-graph-precision-'));
  tempDirs.push(dir);
  await writeFile(
    join(dir, 'package.json'),
    JSON.stringify({ name: 'precision', main: 'index.js' })
  );
  await writeFile(
    join(dir, 'index.ts'),
    "import type { Shape } from './types.js';\nexport type { Shape };\n"
  );
  await writeFile(
    join(dir, 'types.ts'),
    "import type { Api } from './index.js';\nexport interface Shape { api?: Api }\n"
  );
  await writeFile(
    join(dir, 'self.ts'),
    "import './self.js';\nexport const self = true;\n"
  );
  return dir;
}

describe('localAnalyzeGraph operation contract', () => {
  it('distinguishes type-only cycles from runtime cycles and preserves self-loops', async () => {
    const path = await createEdgePrecisionFixture();
    const dependencies = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.ts',
      depth: 1,
    });
    expect(dependencies.results).toEqual([
      expect.objectContaining({ file: 'types.ts', edgeKinds: ['type-import'] }),
    ]);

    const cycles = await analyzeGraph({ operation: 'cycles', path });
    expect(cycles.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          files: ['index.ts', 'types.ts'],
          runtimeCycle: false,
          cycleEdges: [
            {
              from: 'index.ts',
              to: 'types.ts',
              edgeKinds: ['type-import'],
            },
            {
              from: 'types.ts',
              to: 'index.ts',
              edgeKinds: ['type-import'],
            },
          ],
          runtimeCycleEdges: [],
        }),
        expect.objectContaining({
          files: ['self.ts'],
          runtimeCycle: true,
          cycleEdges: [
            {
              from: 'self.ts',
              to: 'self.ts',
              edgeKinds: ['static-import'],
            },
          ],
          runtimeCycleEdges: [
            {
              from: 'self.ts',
              to: 'self.ts',
              edgeKinds: ['static-import'],
            },
          ],
        }),
      ])
    );
  });

  it('keeps the tools-core runtime module load graph acyclic', async () => {
    const result = await analyzeGraph({
      operation: 'cycles',
      path: join(process.cwd(), 'src', 'tools'),
    });
    expect(result.results.filter(item => item.runtimeCycle)).toEqual([]);
  });

  it('resolves declared workspace package exports but never path aliases', async () => {
    const path = await createWorkspacePackageFixture();
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'packages/consumer/src/main.ts',
      depth: 1,
    });

    expect(result.results).toEqual([
      expect.objectContaining({
        file: 'packages/library/src/schema.ts',
        edgeKinds: ['static-import'],
      }),
    ]);
    expect(JSON.stringify(result)).not.toContain('@fixture-alias/schema');
  });

  it('reports exact import and re-export provenance on traversed edges', async () => {
    const path = await createProvenanceFixture();
    const result = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.js',
      depth: 1,
    });

    expect(result.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          file: 'static.js',
          edgeKinds: ['static-import'],
          confidence: 'syntactic',
        }),
        expect.objectContaining({
          file: 'named.js',
          edgeKinds: ['named-reexport'],
          confidence: 'syntactic',
        }),
        expect.objectContaining({
          file: 'star.js',
          edgeKinds: ['star-reexport'],
          confidence: 'syntactic',
        }),
        expect.objectContaining({
          file: 'dynamic.js',
          edgeKinds: ['dynamic-import'],
          confidence: 'syntactic',
        }),
      ])
    );
  });

  it('builds one same-root graph per bulk request', async () => {
    const path = await createGraphFixture();
    const original = contextUtils.extractGraphFacts.bind(contextUtils);
    const extract = vi
      .spyOn(contextUtils, 'extractGraphFacts')
      .mockImplementation((content, filePath) => original(content, filePath));

    await executeAnalyzeGraph({
      queries: [
        { operation: 'cycles', path },
        { operation: 'reachability', path, includeTests: false },
      ],
    });

    expect(extract).toHaveBeenCalledTimes(5);
    extract.mockRestore();
  });

  it('covers all six bounded graph operations through one executor', async () => {
    const path = await createGraphFixture();

    const dependencies = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.js',
      depth: 2,
    });
    expect(dependencies.results).toEqual([
      expect.objectContaining({ file: 'a.js', distance: 1 }),
      expect.objectContaining({ file: 'b.js', distance: 2 }),
    ]);

    const dependents = await analyzeGraph({
      operation: 'dependents',
      path,
      file: 'b.js',
      depth: 2,
    });
    expect(dependents.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ file: 'a.js', distance: 1 }),
        expect.objectContaining({ file: 'index.js', distance: 2 }),
      ])
    );

    const pathResult = await analyzeGraph({
      operation: 'path',
      path,
      file: 'index.js',
      target: 'b.js',
    });
    expect(pathResult.results).toEqual([
      expect.objectContaining({ files: ['index.js', 'a.js', 'b.js'] }),
    ]);

    const cycles = await analyzeGraph({ operation: 'cycles', path });
    expect(cycles.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          files: expect.arrayContaining(['a.js', 'b.js']),
        }),
        expect.objectContaining({
          files: expect.arrayContaining(['dead.js', 'orphan.js']),
        }),
      ])
    );

    const reachability = await analyzeGraph({
      operation: 'reachability',
      path,
      includeTests: false,
    });
    expect(reachability.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ file: 'index.js', reachable: true }),
        expect.objectContaining({ file: 'orphan.js', reachable: false }),
      ])
    );

    const deadCode = await analyzeGraph({
      operation: 'deadCode',
      path,
      includeTests: false,
    });
    expect(deadCode.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ file: 'orphan.js', name: 'orphan' }),
        expect.objectContaining({ file: 'dead.js', name: 'dead' }),
      ])
    );
    expect(deadCode.summary).toEqual(
      expect.objectContaining({ deadClusters: expect.any(Array) })
    );
  });

  it('applies the shared result limit before pagination', async () => {
    const path = await createGraphFixture();
    const result = await analyzeGraph({
      operation: 'reachability',
      path,
      includeTests: false,
      limit: 3,
      pageSize: 2,
      page: 2,
    });

    expect(result.pagination).toMatchObject({
      totalEntries: 3,
      totalPages: 2,
      currentPage: 2,
      entriesPerPage: 2,
    });
    expect(result.results).toHaveLength(1);
  });

  it('uses the schema maximum as its default page size without dropping entrypoints', async () => {
    const path = await createWideGraphFixture();
    const entrypoints = Array.from({ length: 55 }, (_, index) =>
      join(path, `entry-${index}.js`)
    );
    const result = await analyzeGraph({
      operation: 'reachability',
      path,
      entrypoints,
      includeTests: false,
    });

    expect(result.pagination?.entriesPerPage).toBe(50);
    expect(result.results).toHaveLength(50);
    expect(result.summary).toMatchObject({
      entrypointsResolvedCount: 55,
    });
    expect(result.summary).not.toHaveProperty('entrypointsResolvedTruncated');
    expect(result.summary?.entrypointsResolved).toHaveLength(55);
  });

  it('leaves files unclassified when inferred reachability has no roots', async () => {
    const path = await createWideGraphFixture(3);
    const result = await analyzeGraph({
      operation: 'reachability',
      path,
      includeTests: false,
    });

    expect(result).toMatchObject({
      status: 'empty',
      confidence: 'low',
      results: [],
      summary: {
        entrypointsResolvedCount: 0,
        classifiedCount: 0,
        unclassifiedCount: 3,
      },
      warnings: [expect.stringContaining('pass `entrypoints` explicitly')],
    });
    expect(result.summary).not.toHaveProperty('unreachableCount');
  });

  it('maps a compiled bin entry to src/index.ts when basenames differ', async () => {
    const path = await mkdtemp(join(process.cwd(), '.tmp-local-graph-bin-'));
    tempDirs.push(path);
    await mkdir(join(path, 'src'), { recursive: true });
    await writeFile(
      join(path, 'package.json'),
      JSON.stringify({ name: 'cli-fixture', bin: 'out/cli.js' })
    );
    await writeFile(
      join(path, 'src', 'index.ts'),
      "import { live } from './live.js';\nexport const main = live;\n"
    );
    await writeFile(join(path, 'src', 'live.ts'), 'export const live = 1;\n');

    const result = await analyzeGraph({
      operation: 'reachability',
      path,
      includeTests: false,
    });

    expect(result.results).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ file: 'src/index.ts', reachable: true }),
        expect.objectContaining({ file: 'src/live.ts', reachable: true }),
      ])
    );
  });

  it('adds custom exclusions without dropping the safe defaults', async () => {
    const path = await mkdtemp(
      join(process.cwd(), '.tmp-local-graph-excludes-')
    );
    tempDirs.push(path);
    await Promise.all([
      mkdir(join(path, 'src'), { recursive: true }),
      mkdir(join(path, 'out'), { recursive: true }),
      mkdir(join(path, 'fixtures'), { recursive: true }),
    ]);
    await writeFile(
      join(path, 'package.json'),
      JSON.stringify({ name: 'exclude-fixture', main: 'src/index.ts' })
    );
    await writeFile(join(path, 'src', 'index.ts'), 'export const live = 1;\n');
    await writeFile(join(path, 'out', 'noise.js'), 'export const noise = 1;\n');
    await writeFile(
      join(path, 'fixtures', 'noise.ts'),
      'export const fixtureNoise = 1;\n'
    );

    const result = await analyzeGraph({
      operation: 'reachability',
      path,
      includeTests: false,
      excludeDir: ['fixtures'],
    });

    expect(result.filesScanned).toBe(1);
    expect(result.results.map(item => item.file)).toEqual(['src/index.ts']);
  });
});
