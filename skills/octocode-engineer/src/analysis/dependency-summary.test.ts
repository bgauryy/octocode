import { describe, expect, it } from 'vitest';

import { trackDependencyEdge } from './dependencies.js';
import {
  buildDependencySummary,
  computeDependencyCriticalPaths,
  computeDependencyCycles,
} from './dependency-summary.js';
import { DEFAULT_OPTS } from '../types/index.js';

import type {
  AnalysisOptions,
  DependencyState,
  FileCriticality,
} from '../types/index.js';

function makeDependencyState(
  overrides: Partial<DependencyState> = {}
): DependencyState {
  return {
    files: new Set(),
    outgoing: new Map(),
    incoming: new Map(),
    incomingFromProduction: new Map(),
    incomingFromTests: new Map(),
    externalCounts: new Map(),
    unresolvedCounts: new Map(),
    declaredExportsByFile: new Map(),
    importedSymbolsByFile: new Map(),
    reExportsByFile: new Map(),
    ...overrides,
  };
}

function addEdge(
  state: DependencyState,
  from: string,
  to: string,
  importerIsTest: boolean
): void {
  state.files.add(from);
  state.files.add(to);
  trackDependencyEdge(state, from, to, importerIsTest);
}

function addEdges(
  state: DependencyState,
  edges: Array<[string, string, boolean]>
): void {
  for (const [from, to, isTest] of edges) {
    addEdge(state, from, to, isTest);
  }
}

describe('buildDependencySummary', () => {
  it('handles empty dependency state (no files)', () => {
    const state = makeDependencyState();
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.totalModules).toBe(0);
    expect(summary.totalEdges).toBe(0);
    expect(summary.unresolvedEdgeCount).toBe(0);
    expect(summary.externalDependencyFiles).toBe(0);
    expect(summary.rootsCount).toBe(0);
    expect(summary.leavesCount).toBe(0);
    expect(summary.roots).toEqual([]);
    expect(summary.leaves).toEqual([]);
    expect(summary.criticalModules).toEqual([]);
    expect(summary.testOnlyModules).toEqual([]);
    expect(summary.unresolvedSample).toEqual([]);
    expect(summary.cycles).toEqual([]);
    expect(summary.criticalPaths).toEqual([]);
  });

  it('handles single file with no deps', () => {
    const state = makeDependencyState({
      files: new Set(['src/standalone.ts']),
      outgoing: new Map([['src/standalone.ts', new Set()]]),
      incoming: new Map([['src/standalone.ts', new Set()]]),
    });
    const criticality = new Map<string, FileCriticality>([
      ['src/standalone.ts', { score: 5 } as FileCriticality],
    ]);
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.totalModules).toBe(1);
    expect(summary.totalEdges).toBe(0);
    expect(summary.rootsCount).toBe(1);
    expect(summary.leavesCount).toBe(1);
    expect(summary.roots).toContain('src/standalone.ts');
    expect(summary.leaves).toContain('src/standalone.ts');
  });

  it('handles multiple files with outgoing/incoming edges', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['a.ts', 'c.ts', false],
      ['b.ts', 'c.ts', false],
    ]);
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.totalModules).toBe(3);
    expect(summary.totalEdges).toBe(3);
    expect(summary.rootsCount).toBe(1);
    expect(summary.leavesCount).toBe(1);
    expect(summary.roots).toContain('a.ts');
    expect(summary.leaves).toContain('c.ts');
    expect(summary.outgoingTop[0].file).toBe('a.ts');
    expect(summary.outgoingTop[0].count).toBe(2);
    expect(summary.inboundTop[0].file).toBe('c.ts');
    expect(summary.inboundTop[0].count).toBe(2);
  });

  it('detects test-only modules (no production imports, only test imports)', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['src/foo.test.ts', 'src/helper.ts', true],
    ]);
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.testOnlyModules.length).toBe(1);
    expect(summary.testOnlyModules[0].file).toBe('src/helper.ts');
  });

  it('excludes test files from test-only modules list', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['src/foo.test.ts', 'src/bar.test.ts', true],
    ]);
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.testOnlyModules).toEqual([]);
  });

  it('sorts test-only modules by file name', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['src/foo.test.ts', 'src/z-helper.ts', true],
      ['src/foo.test.ts', 'src/a-helper.ts', true],
    ]);
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.testOnlyModules.length).toBe(2);
    expect(summary.testOnlyModules[0].file).toBe('src/a-helper.ts');
    expect(summary.testOnlyModules[1].file).toBe('src/z-helper.ts');
  });

  it('filters critical nodes by score > 12', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'c.ts', false],
    ]);
    const criticality = new Map<string, FileCriticality>([
      ['a.ts', { score: 5 } as FileCriticality],
      ['b.ts', { score: 15 } as FileCriticality],
      ['c.ts', { score: 3 } as FileCriticality],
    ]);
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.criticalModules.length).toBe(1);
    expect(summary.criticalModules[0].file).toBe('b.ts');
    expect(summary.criticalModules[0].score).toBe(15);
  });

  it('filters critical nodes by outbound > 5', () => {
    const state = makeDependencyState();
    const hub = 'hub.ts';
    state.files.add(hub);
    state.files.add('a.ts');
    state.files.add('b.ts');
    state.files.add('c.ts');
    state.files.add('d.ts');
    state.files.add('e.ts');
    state.files.add('f.ts');
    state.files.add('g.ts');
    state.outgoing.set(
      hub,
      new Set(['a.ts', 'b.ts', 'c.ts', 'd.ts', 'e.ts', 'f.ts'])
    );
    state.incoming.set('a.ts', new Set([hub]));
    state.incoming.set('b.ts', new Set([hub]));
    state.incoming.set('c.ts', new Set([hub]));
    state.incoming.set('d.ts', new Set([hub]));
    state.incoming.set('e.ts', new Set([hub]));
    state.incoming.set('f.ts', new Set([hub]));
    state.incomingFromProduction.set('a.ts', new Set([hub]));
    state.incomingFromProduction.set('b.ts', new Set([hub]));
    state.incomingFromProduction.set('c.ts', new Set([hub]));
    state.incomingFromProduction.set('d.ts', new Set([hub]));
    state.incomingFromProduction.set('e.ts', new Set([hub]));
    state.incomingFromProduction.set('f.ts', new Set([hub]));

    const criticality = new Map<string, FileCriticality>([
      ['hub.ts', { score: 1 } as FileCriticality],
    ]);
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.criticalModules.some(m => m.file === 'hub.ts')).toBe(true);
  });

  it('filters critical nodes by inbound > 8', () => {
    const state = makeDependencyState();
    const hub = 'hub.ts';
    const importers = [
      'a.ts',
      'b.ts',
      'c.ts',
      'd.ts',
      'e.ts',
      'f.ts',
      'g.ts',
      'h.ts',
      'i.ts',
    ];
    state.files.add(hub);
    for (const imp of importers) {
      state.files.add(imp);
      addEdge(state, imp, hub, false);
    }
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.criticalModules.some(m => m.file === 'hub.ts')).toBe(true);
  });

  it('assigns riskBand high/medium/low by score', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'c.ts', false],
    ]);
    const criticality = new Map<string, FileCriticality>([
      ['a.ts', { score: 70 } as FileCriticality],
      ['b.ts', { score: 40 } as FileCriticality],
      ['c.ts', { score: 15 } as FileCriticality],
    ]);
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    const high = summary.criticalModules.find(m => m.riskBand === 'high');
    const medium = summary.criticalModules.find(m => m.riskBand === 'medium');
    const low = summary.criticalModules.find(m => m.riskBand === 'low');
    expect(high?.file).toBe('a.ts');
    expect(medium?.file).toBe('b.ts');
    expect(low?.file).toBe('c.ts');
  });

  it('counts unresolved edges and includes unresolvedSample when > 0', () => {
    const state = makeDependencyState();
    addEdges(state, [['a.ts', 'b.ts', false]]);
    state.unresolvedCounts.set('a.ts', new Set(['./missing1', './missing2']));
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.unresolvedEdgeCount).toBe(2);
    expect(summary.unresolvedSample).toContain('a.ts');
  });

  it('excludes unresolvedSample when unresolvedEdgeCount is 0', () => {
    const state = makeDependencyState();
    addEdges(state, [['a.ts', 'b.ts', false]]);
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.unresolvedEdgeCount).toBe(0);
    expect(summary.unresolvedSample).toEqual([]);
  });

  it('counts external dependency files', () => {
    const state = makeDependencyState();
    addEdges(state, [['a.ts', 'b.ts', false]]);
    state.externalCounts.set('a.ts', new Set(['lodash', 'express']));
    state.externalCounts.set('b.ts', new Set(['react']));
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.externalDependencyFiles).toBe(2);
  });

  it('uses default score 1 when file not in criticality map', () => {
    const state = makeDependencyState();
    addEdges(state, [['a.ts', 'b.ts', false]]);
    const criticality = new Map<string, FileCriticality>();
    const summary = buildDependencySummary(state, criticality, DEFAULT_OPTS);

    expect(summary.outgoingTop[0].score).toBe(1);
    expect(summary.inboundTop[0].score).toBe(1);
  });

  it('respects deepLinkTopN for criticalPaths', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'c.ts', false],
      ['x.ts', 'y.ts', false],
    ]);
    const criticality = new Map<string, FileCriticality>([
      ['a.ts', { score: 10 } as FileCriticality],
      ['b.ts', { score: 10 } as FileCriticality],
      ['c.ts', { score: 10 } as FileCriticality],
    ]);
    const opts = { ...DEFAULT_OPTS, deepLinkTopN: 1 };
    const summary = buildDependencySummary(state, criticality, opts);

    expect(summary.criticalPaths.length).toBe(1);
  });
});

describe('computeDependencyCycles', () => {
  it('returns empty for no cycles (linear chain A→B→C)', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'c.ts', false],
    ]);
    const cycles = computeDependencyCycles(state);
    expect(cycles).toEqual([]);
  });

  it('detects simple 2-node cycle (A↔B)', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'a.ts', false],
    ]);
    const cycles = computeDependencyCycles(state);
    expect(cycles).toHaveLength(1);
    expect(cycles[0].path).toContain('a.ts');
    expect(cycles[0].path).toContain('b.ts');
    expect(cycles[0].nodeCount).toBe(2);
  });

  it('detects 3-node cycle (A→B→C→A)', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'c.ts', false],
      ['c.ts', 'a.ts', false],
    ]);
    const cycles = computeDependencyCycles(state);
    expect(cycles).toHaveLength(1);
    expect(cycles[0].path).toHaveLength(4);
    expect(cycles[0].nodeCount).toBe(3);
  });

  it('canonicalizes cycles (same cycle from different start produces one result)', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'a.ts', false],
    ]);
    const cycles = computeDependencyCycles(state);
    expect(cycles).toHaveLength(1);
  });

  it('handles self-loops (A→A)', () => {
    const state = makeDependencyState();
    addEdges(state, [['a.ts', 'a.ts', false]]);
    const cycles = computeDependencyCycles(state);
    expect(cycles).toHaveLength(1);
    expect(cycles[0].path).toEqual(['a.ts', 'a.ts']);
    expect(cycles[0].nodeCount).toBe(1);
  });

  it('skips external deps (not in files set)', () => {
    const state = makeDependencyState();
    state.files.add('a.ts');
    state.files.add('b.ts');
    state.outgoing.set('a.ts', new Set(['b.ts', 'external/pkg']));
    state.outgoing.set('b.ts', new Set(['a.ts']));
    state.incoming.set('b.ts', new Set(['a.ts']));
    state.incoming.set('a.ts', new Set(['b.ts']));
    state.incomingFromProduction.set('b.ts', new Set(['a.ts']));
    state.incomingFromProduction.set('a.ts', new Set(['b.ts']));

    const cycles = computeDependencyCycles(state);
    expect(cycles).toHaveLength(1);
    expect(cycles[0].path).not.toContain('external/pkg');
  });

  it('detects multiple cycles in same graph', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'a.ts', false],
      ['c.ts', 'd.ts', false],
      ['d.ts', 'c.ts', false],
    ]);
    const cycles = computeDependencyCycles(state);
    expect(cycles).toHaveLength(2);
  });

  it('sorts cycles by nodeCount descending', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'a.ts', false],
      ['x.ts', 'y.ts', false],
      ['y.ts', 'z.ts', false],
      ['z.ts', 'x.ts', false],
    ]);
    const cycles = computeDependencyCycles(state);
    expect(cycles.length).toBeGreaterThanOrEqual(1);
    if (cycles.length >= 2) {
      expect(cycles[0].nodeCount).toBeGreaterThanOrEqual(cycles[1].nodeCount);
    }
  });
});

describe('computeDependencyCriticalPaths', () => {
  it('produces one path for linear chain', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'c.ts', false],
    ]);
    const criticality = new Map<string, FileCriticality>([
      ['a.ts', { score: 1 } as FileCriticality],
      ['b.ts', { score: 1 } as FileCriticality],
      ['c.ts', { score: 1 } as FileCriticality],
    ]);
    const paths = computeDependencyCriticalPaths(
      state,
      criticality,
      DEFAULT_OPTS
    );

    expect(paths.length).toBeGreaterThanOrEqual(1);
    const fullPath = paths.find(p => p.path.length === 3);
    expect(fullPath).toBeDefined();
    expect(fullPath?.path).toContain('a.ts');
    expect(fullPath?.path).toContain('b.ts');
    expect(fullPath?.path).toContain('c.ts');
    expect(fullPath?.containsCycle).toBe(false);
  });

  it('picks highest-score path in branching graph', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['root.ts', 'low.ts', false],
      ['root.ts', 'high.ts', false],
      ['low.ts', 'leaf1.ts', false],
      ['high.ts', 'leaf2.ts', false],
    ]);
    const criticality = new Map<string, FileCriticality>([
      ['root.ts', { score: 1 } as FileCriticality],
      ['low.ts', { score: 1 } as FileCriticality],
      ['high.ts', { score: 50 } as FileCriticality],
      ['leaf1.ts', { score: 1 } as FileCriticality],
      ['leaf2.ts', { score: 1 } as FileCriticality],
    ]);
    const paths = computeDependencyCriticalPaths(
      state,
      criticality,
      DEFAULT_OPTS
    );

    expect(paths[0].path).toContain('high.ts');
    expect(paths[0].score).toBeGreaterThan(
      paths.find(p => p.path.includes('low.ts'))?.score ?? 0
    );
  });

  it('sets containsCycle when path has cycle', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'a.ts', false],
    ]);
    const criticality = new Map<string, FileCriticality>([
      ['a.ts', { score: 10 } as FileCriticality],
      ['b.ts', { score: 10 } as FileCriticality],
    ]);
    const paths = computeDependencyCriticalPaths(
      state,
      criticality,
      DEFAULT_OPTS
    );

    expect(paths.length).toBeGreaterThan(0);
    const cyclePath = paths.find(p => p.containsCycle);
    expect(cyclePath).toBeDefined();
  });

  it('filters out single-node paths (length > 1 required)', () => {
    const state = makeDependencyState();
    addEdges(state, [['a.ts', 'b.ts', false]]);
    const criticality = new Map<string, FileCriticality>();
    const paths = computeDependencyCriticalPaths(
      state,
      criticality,
      DEFAULT_OPTS
    );

    const singleNodePaths = paths.filter(p => p.length <= 1);
    expect(singleNodePaths).toHaveLength(0);
  });

  it('respects deepLinkTopN limit', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['b.ts', 'c.ts', false],
      ['x.ts', 'y.ts', false],
      ['y.ts', 'z.ts', false],
    ]);
    const criticality = new Map<string, FileCriticality>();
    const opts: AnalysisOptions = { ...DEFAULT_OPTS, deepLinkTopN: 2 };
    const paths = computeDependencyCriticalPaths(state, criticality, opts);

    expect(paths.length).toBe(2);
  });

  it('returns at least one path when deepLinkTopN is 0 (via Math.max(1, N))', () => {
    const state = makeDependencyState();
    addEdges(state, [['a.ts', 'b.ts', false]]);
    const criticality = new Map<string, FileCriticality>();
    const opts: AnalysisOptions = { ...DEFAULT_OPTS, deepLinkTopN: 0 };
    const paths = computeDependencyCriticalPaths(state, criticality, opts);

    expect(paths.length).toBeGreaterThanOrEqual(1);
  });

  it('uses default score 1 for files not in criticality map', () => {
    const state = makeDependencyState();
    addEdges(state, [['a.ts', 'b.ts', false]]);
    const criticality = new Map<string, FileCriticality>();
    const paths = computeDependencyCriticalPaths(
      state,
      criticality,
      DEFAULT_OPTS
    );

    expect(paths[0].score).toBeGreaterThanOrEqual(1);
  });

  it('sorts by score descending then by length descending', () => {
    const state = makeDependencyState();
    addEdges(state, [
      ['a.ts', 'b.ts', false],
      ['x.ts', 'y.ts', false],
      ['y.ts', 'z.ts', false],
    ]);
    const criticality = new Map<string, FileCriticality>([
      ['a.ts', { score: 100 } as FileCriticality],
      ['b.ts', { score: 1 } as FileCriticality],
      ['x.ts', { score: 1 } as FileCriticality],
      ['y.ts', { score: 1 } as FileCriticality],
      ['z.ts', { score: 1 } as FileCriticality],
    ]);
    const paths = computeDependencyCriticalPaths(
      state,
      criticality,
      DEFAULT_OPTS
    );

    expect(paths[0].start).toBe('a.ts');
    expect(paths[0].score).toBeGreaterThan(paths[1]?.score ?? 0);
  });
});
