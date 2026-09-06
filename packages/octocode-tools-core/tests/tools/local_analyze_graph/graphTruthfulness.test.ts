import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { analyzeGraph } from '../../../src/tools/local_analyze_graph/analyzeGraph.js';
import { buildFileGraph } from '../../../src/graph/buildFileGraph.js';
import { contextUtils } from '../../../src/utils/contextUtils.js';
import { LocalAnalyzeGraphQuerySchema } from '../../../src/tools/local_analyze_graph/scheme.js';
import * as graphAlgorithms from '../../../src/graph/advancedOperations.js';

const fixtures: string[] = [];
afterEach(async () => {
  vi.restoreAllMocks();
  await Promise.all(
    fixtures.splice(0).map(path => rm(path, { recursive: true, force: true }))
  );
});

async function fixture(sources: Record<string, string>): Promise<string> {
  const path = await mkdtemp(join(process.cwd(), '.tmp-graph-truth-'));
  fixtures.push(path);
  await Promise.all(
    Object.entries(sources).map(([file, source]) =>
      writeFile(join(path, file), source)
    )
  );
  return path;
}

describe('graph edge and completeness truthfulness', () => {
  it('keeps Rust and C compile-time cycles distinct from JavaScript runtime import candidates', async () => {
    const path = await fixture({
      'lib.rs': 'pub mod a;\npub mod b;\n',
      'a.rs': 'use crate::b::B;\npub struct A(pub Option<B>);\n',
      'b.rs': 'use crate::a::A;\npub struct B(pub Option<Box<A>>);\n',
      'a.h': '#ifndef A_H\n#define A_H\n#include "b.h"\n#endif\n',
      'b.h': '#ifndef B_H\n#define B_H\n#include "a.h"\n#endif\n',
      'a.ts': "import { b } from './b.js';\nexport const a = () => b;\n",
      'b.ts': "import { a } from './a.js';\nexport const b = () => a;\n",
    });
    const output = await analyzeGraph({ operation: 'cycles', path });
    const rust = output.results.find(row =>
      (row.files as string[]).includes('a.rs')
    )!;
    const headers = output.results.find(row =>
      (row.files as string[]).includes('a.h')
    )!;
    const javascript = output.results.find(row =>
      (row.files as string[]).includes('a.ts')
    )!;
    expect(rust).toMatchObject({
      edgeKinds: ['rust-use'],
      runtimeCycle: false,
      runtimeCycleCount: 0,
    });
    expect(headers).toMatchObject({
      edgeKinds: ['c-include'],
      runtimeCycle: false,
      runtimeCycleCount: 0,
    });
    expect(javascript).toMatchObject({
      runtimeCycle: true,
      runtimeCycleCount: 1,
    });
    expect(output.summary?.runtimeCycleCount).toBe(1);
    const modules = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'lib.rs',
    });
    expect(modules.results.map(row => row.edgeKinds)).toEqual([
      ['rust-module'],
      ['rust-module'],
    ]);
  });

  it('does not reinterpret absolute JavaScript imports as scan-root-relative files', async () => {
    const path = await fixture({
      'index.ts': "import '/target.js';\n",
      'target.ts': 'export const target = 1;\n',
    });
    const built = await buildFileGraph(path, [], 20);
    expect([...built.fileGraph.get('index.ts')!.importsFiles]).toEqual([]);
    expect(built.coverage?.imports.unsupported).toBe(1);
  });

  it('answers depth-one dominators without traversing the whole reachable graph', async () => {
    const path = await fixture({
      'index.ts': "import './a.js';\nimport './b.js';\n",
      'a.ts': "import './b.js';\n",
      'b.ts': "import './c.js';\n",
      'c.ts': 'export const value = 1;\n',
    });
    const dominators = vi.spyOn(graphAlgorithms, 'computeImmediateDominators');
    const shallow = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.ts',
      depth: 1,
    });
    expect(dominators).not.toHaveBeenCalled();
    const deep = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.ts',
      depth: 3,
    });
    expect(shallow.results).toEqual(
      deep.results.filter(item => item.distance === 1)
    );
    expect(
      deep.results.find(item => item.file === 'c.ts')?.immediateDominator
    ).toBe('b.ts');
  });

  it('keeps named and star type reexports erased while preserving their provenance', async () => {
    const path = await fixture({
      'index.ts':
        "export type { Shape } from './types.js';\nexport type * from './types.js';\nexport interface Api {}\n",
      'types.ts':
        "export type { Api } from './index.js';\nexport interface Shape {}\n",
    });
    const dependencies = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.ts',
    });
    expect(dependencies.results[0]?.edgeKinds).toEqual([
      'type-named-reexport',
      'type-star-reexport',
    ]);
    const cycles = await analyzeGraph({ operation: 'cycles', path });
    expect(cycles.results).toEqual([
      expect.objectContaining({
        files: ['index.ts', 'types.ts'],
        runtimeCycle: false,
        runtimeCycleEdges: [],
      }),
    ]);
  });

  it('retains runtime cycles when the same target has type and value reexports', async () => {
    const path = await fixture({
      'index.ts':
        "export type { Shape } from './types.js';\nexport { value } from './types.js';\nexport const api = 1;\n",
      'types.ts':
        "export { api } from './index.js';\nexport interface Shape {}\nexport const value = 1;\n",
    });
    const cycles = await analyzeGraph({ operation: 'cycles', path });
    expect(cycles.results[0]?.runtimeCycle).toBe(true);
  });

  it('explicitly marks computed CommonJS linking unsupported instead of claiming a complete empty graph', async () => {
    const path = await fixture({
      'index.cjs':
        'const target = require(moduleName);\nmodule.exports = target;\n',
      'target.cjs': 'module.exports = 1;\n',
    });
    const output = await analyzeGraph({
      operation: 'dependencies',
      path,
      file: 'index.cjs',
    });
    expect(output.coverage?.diagnostics).toContainEqual(
      expect.objectContaining({
        file: 'index.cjs',
        line: 1,
        code: 'unsupported-linking',
        message: expect.stringContaining('CommonJS require'),
      })
    );
    expect(output.partialReasons).toContain('unsupportedLinking');
    expect(output.terminalLimit).toBe(true);
  });

  it('deduplicates per-binding diagnostics but retains distinct source occurrences', async () => {
    vi.spyOn(contextUtils, 'scanGraphFacts').mockResolvedValue({
      candidatePaths: ['index.ts'],
      filesSkipped: 0,
      truncated: false,
      entries: [
        {
          relativePath: 'index.ts',
          referenceCounts: [],
          factsJson: JSON.stringify({
            imports: [
              {
                specifier: './missing.js',
                line: 1,
                localName: 'a',
                importedName: 'a',
              },
              {
                specifier: './missing.js',
                line: 1,
                localName: 'b',
                importedName: 'b',
              },
              {
                specifier: './missing.js',
                line: 2,
                localName: 'c',
                importedName: 'c',
              },
            ],
          }),
        },
      ],
    });
    const built = await buildFileGraph('/missing-graph-fixture', [], 10);
    expect(built.coverage?.diagnostics).toHaveLength(2);
    expect(built.coverage?.diagnostics.map(item => item.line)).toEqual([1, 2]);
    expect(built.coverage?.imports.unresolvedInternal).toBe(3);
  });

  it('expands cap-caused unresolved imports to a complete graph without a terminal signal', async () => {
    const path = await fixture({
      'index.ts': "import './target.js';\n",
      'target.ts': 'export const target = 1;\n',
    });
    const full = await buildFileGraph(path, [], 20);
    const partial = {
      ...full,
      truncated: true,
      coverage: {
        ...full.coverage!,
        imports: {
          resolved: 0,
          external: 0,
          unresolvedInternal: 1,
          unsupported: 0,
        },
        diagnostics: [
          {
            file: 'index.ts',
            line: 1,
            code: 'unresolved-internal' as const,
            message: 'Target outside capped scan.',
          },
        ],
      },
    };
    const first = await analyzeGraph(
      { operation: 'cycles', path, maxFiles: 1 },
      { getGraph: () => partial }
    );
    expect(first.terminalLimit).toBeUndefined();
    const next = first.next?.expandScan as { query: unknown };
    const expanded = await analyzeGraph(
      LocalAnalyzeGraphQuerySchema.parse(next.query),
      { getGraph: () => full }
    );
    expect(expanded.truncated).toBeUndefined();
    expect(expanded.terminalLimit).toBeUndefined();
    expect(expanded.coverage?.imports.unresolvedInternal).toBe(0);
  });
});
