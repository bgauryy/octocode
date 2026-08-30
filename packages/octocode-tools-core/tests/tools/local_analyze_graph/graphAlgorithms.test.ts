import { describe, expect, it } from 'vitest';

import {
  computeImmediateDominators,
  condenseGraph,
  findTransitiveEdges,
  findWeightedShortestPath,
} from '../../../src/graph/advancedOperations.js';
import { findCycleWitness } from '../../../src/graph/cycleOperations.js';
import { findShortestPath } from '../../../src/graph/operations.js';
import { findStronglyConnectedComponents } from '../../../src/graph/reachability.js';
import { computeLiveExportedNames } from '../../../src/tools/local_analyze_graph/retention.js';
import type { FileGraphEdgeKind, FileNode } from '../../../src/graph/types.js';

function graphOf(edges: Record<string, string[]>): Map<string, FileNode> {
  const files = new Set([
    ...Object.keys(edges),
    ...Object.values(edges).flat(),
  ]);
  return new Map(
    [...files].map(file => {
      const targets = edges[file] ?? [];
      return [
        file,
        {
          relativePath: file,
          importsFiles: new Set(targets),
          dynamicImportsFiles: new Set<string>(),
          edgeKinds: new Map(
            targets.map(target => [
              target,
              new Set<FileGraphEdgeKind>(['static-import']),
            ])
          ),
        },
      ];
    })
  );
}

describe('advanced file-graph algorithms', () => {
  it('does not present a truncated path prefix as a complete source-to-target path', () => {
    const edges: Record<string, string[]> = {};
    for (let index = 0; index < 105; index++) {
      edges[`f${index}.ts`] = index < 104 ? [`f${index + 1}.ts`] : [];
    }
    const result = findShortestPath(graphOf(edges), 'f0.ts', 'f104.ts');
    expect(result).toMatchObject({
      found: true,
      complete: false,
      files: [],
      target: 'f104.ts',
      length: 105,
      totalFileCount: 105,
      omittedMiddleFileCount: 5,
    });
    expect(result.prefix).toHaveLength(50);
    expect(result.suffix).toHaveLength(50);
  });

  it('retains singleton self-loops as real SCC cycles', () => {
    const graph = graphOf({ 'self.ts': ['self.ts'] });
    expect(findStronglyConnectedComponents(graph)).toEqual([
      { files: ['self.ts'] },
    ]);
    expect(findCycleWitness(graph, new Set(['self.ts']))).toEqual([
      { from: 'self.ts', to: 'self.ts' },
    ]);
  });

  it('returns a deterministic directed cycle witness', () => {
    const graph = graphOf({
      'a.ts': ['b.ts'],
      'b.ts': ['c.ts'],
      'c.ts': ['a.ts'],
      'outside.ts': ['a.ts'],
    });
    expect(findCycleWitness(graph, new Set(['a.ts', 'b.ts', 'c.ts']))).toEqual([
      { from: 'a.ts', to: 'b.ts' },
      { from: 'b.ts', to: 'c.ts' },
      { from: 'c.ts', to: 'a.ts' },
    ]);
  });

  it('condenses SCCs into deterministic topological layers', () => {
    const condensed = condenseGraph(
      graphOf({ a: ['b'], b: ['a', 'c'], c: ['d'], d: [] })
    );
    expect(condensed.components).toEqual([['a', 'b'], ['c'], ['d']]);
    expect(condensed.layers).toEqual([[0], [1], [2]]);
  });

  it('finds edges removable without changing DAG reachability', () => {
    const condensed = condenseGraph(
      graphOf({ a: ['b', 'c'], b: ['c'], c: [] })
    );
    expect(findTransitiveEdges(condensed.edges)).toEqual(new Set(['0:2']));
  });

  it('computes immediate dominators for mandatory dependency chokepoints', () => {
    const dominators = computeImmediateDominators(
      graphOf({
        root: ['a', 'b'],
        a: ['join'],
        b: ['join'],
        join: ['leaf'],
        leaf: [],
      }),
      'root'
    );
    expect(dominators.get('join')).toBe('root');
    expect(dominators.get('leaf')).toBe('join');
  });

  it('uses Dijkstra only when a caller supplies non-negative edge costs', () => {
    const graph = graphOf({ a: ['b', 'c'], b: ['d'], c: ['d'], d: [] });
    const costs = new Map([
      ['a:b', 10],
      ['b:d', 1],
      ['a:c', 2],
      ['c:d', 2],
    ]);
    expect(
      findWeightedShortestPath(
        graph,
        'a',
        'd',
        (from, to) => costs.get(`${from}:${to}`) as number
      )
    ).toEqual({ found: true, files: ['a', 'c', 'd'], cost: 4 });
    expect(() => findWeightedShortestPath(graph, 'a', 'd', () => -1)).toThrow(
      'non-negative'
    );
  });

  it('handles a deep re-export liveness chain without recursive stack growth', () => {
    const depth = 20_000;
    const reexports = new Map<
      string,
      Array<{ file: string; localName: string }>
    >();
    for (let index = 0; index < depth; index++) {
      reexports.set(`f${index}::value`, [
        { file: `f${index + 1}`, localName: 'value' },
      ]);
    }
    const live = computeLiveExportedNames(
      'f0',
      {
        relativePath: 'f0',
        declarations: [
          {
            id: 'value',
            name: 'value',
            kind: 'constant',
            line: 1,
            exported: true,
          },
        ],
        imports: [],
        namedReexports: [],
        calls: [],
        referenceCounts: new Map([['value', 1]]),
      },
      new Set([`f${depth}`]),
      new Set(),
      reexports,
      new Map()
    );
    expect(live).toEqual(new Set(['value']));
  });
});
