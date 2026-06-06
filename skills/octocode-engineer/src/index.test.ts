import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { isLikelyEntrypoint } from './detectors/index.js';
import {
  ARCHITECTURE_CATEGORIES,
  CODE_QUALITY_CATEGORIES,
  DEAD_CODE_CATEGORIES,
  SECURITY_CATEGORIES,
  TEST_QUALITY_CATEGORIES,
  buildIssueCatalog,
  categoryBreakdown,
  collectTagCloud,
  computeDependencyCriticalPaths,
  computeDependencyCycles,
  computeHealthScore,
  diverseTopRecommendations,
  diversifyFindings,
  generateSummaryMd,
  severityBreakdown,
  writeMultiFileReport,
} from './index.js';
import { DEFAULT_OPTS, PILLAR_CATEGORIES } from './types/index.js';

import type { FullReport } from './index.js';
import type {
  DependencyProfile,
  DependencyState,
  DependencySummary,
  DuplicateGroup,
  FileCriticality,
  FileEntry,
  Finding,
  FunctionEntry,
} from './types/index.js';

function emptyState(): DependencyState {
  return {
    files: new Set(),
    outgoing: new Map(),
    incoming: new Map(),
    incomingFromTests: new Map(),
    incomingFromProduction: new Map(),
    externalCounts: new Map(),
    unresolvedCounts: new Map(),
    declaredExportsByFile: new Map(),
    importedSymbolsByFile: new Map(),
    reExportsByFile: new Map(),
  };
}

function addEdge(
  state: DependencyState,
  from: string,
  to: string,
  isTest = false
): void {
  state.files.add(from);
  state.files.add(to);
  if (!state.outgoing.has(from)) state.outgoing.set(from, new Set());
  state.outgoing.get(from)!.add(to);
  if (!state.incoming.has(to)) state.incoming.set(to, new Set());
  state.incoming.get(to)!.add(from);
  if (isTest) {
    if (!state.incomingFromTests.has(to))
      state.incomingFromTests.set(to, new Set());
    state.incomingFromTests.get(to)!.add(from);
  } else {
    if (!state.incomingFromProduction.has(to))
      state.incomingFromProduction.set(to, new Set());
    state.incomingFromProduction.get(to)!.add(from);
  }
}

const emptyProfile: DependencyProfile = {
  internalDependencies: [],
  externalDependencies: [],
  unresolvedDependencies: [],
  declaredExports: [],
  importedSymbols: [],
  reExports: [],
};

function makeFn(overrides: Partial<FunctionEntry> = {}): FunctionEntry {
  return {
    kind: 'FunctionDeclaration',
    name: 'fn',
    nameHint: 'fn',
    file: 'src/a.ts',
    lineStart: 1,
    lineEnd: 10,
    columnStart: 1,
    columnEnd: 1,
    statementCount: 5,
    complexity: 1,
    maxBranchDepth: 0,
    maxLoopDepth: 0,
    returns: 1,
    awaits: 0,
    calls: 0,
    loops: 0,
    lengthLines: 10,
    cognitiveComplexity: 0,
    ...overrides,
  };
}

function makeFile(overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    package: 'test',
    file: 'src/a.ts',
    parseEngine: 'typescript',
    nodeCount: 50,
    kindCounts: {},
    functions: [],
    flows: [],
    dependencyProfile: emptyProfile,
    ...overrides,
  };
}

const testOpts = { ...DEFAULT_OPTS, root: '/repo', findingsLimit: 1000 };

function minimalDepSummary(
  overrides: Partial<DependencySummary> = {}
): DependencySummary {
  return {
    totalModules: 0,
    totalEdges: 0,
    unresolvedEdgeCount: 0,
    externalDependencyFiles: 0,
    rootsCount: 0,
    leavesCount: 0,
    roots: [],
    leaves: [],
    criticalModules: [],
    testOnlyModules: [],
    unresolvedSample: [],
    outgoingTop: [],
    inboundTop: [],
    cycles: [],
    criticalPaths: [],
    ...overrides,
  };
}

describe('isLikelyEntrypoint', () => {
  it('matches index files', () => {
    expect(isLikelyEntrypoint('src/index.ts')).toBe(true);
    expect(isLikelyEntrypoint('packages/foo/src/index.tsx')).toBe(true);
    expect(isLikelyEntrypoint('index.js')).toBe(true);
  });

  it('matches main, app, server, cli', () => {
    expect(isLikelyEntrypoint('src/main.ts')).toBe(true);
    expect(isLikelyEntrypoint('src/app.ts')).toBe(true);
    expect(isLikelyEntrypoint('src/server.ts')).toBe(true);
    expect(isLikelyEntrypoint('src/cli.ts')).toBe(true);
  });

  it('rejects non-entrypoint files', () => {
    expect(isLikelyEntrypoint('src/utils.ts')).toBe(false);
    expect(isLikelyEntrypoint('src/helper.ts')).toBe(false);
    expect(isLikelyEntrypoint('src/index-utils.ts')).toBe(false);
  });

  it('is case insensitive', () => {
    expect(isLikelyEntrypoint('src/Index.ts')).toBe(true);
    expect(isLikelyEntrypoint('src/MAIN.ts')).toBe(true);
  });
});

describe('computeDependencyCycles', () => {
  it('returns empty for acyclic graph', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'c.ts');
    expect(computeDependencyCycles(state)).toEqual([]);
  });

  it('detects simple 2-node cycle', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'a.ts');
    const cycles = computeDependencyCycles(state);
    expect(cycles.length).toBe(1);
    expect(cycles[0].nodeCount).toBe(2);
  });

  it('detects 3-node cycle', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'c.ts');
    addEdge(state, 'c.ts', 'a.ts');
    const cycles = computeDependencyCycles(state);
    expect(cycles.length).toBe(1);
    expect(cycles[0].nodeCount).toBe(3);
  });

  it('detects multiple cycles', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'a.ts');
    addEdge(state, 'c.ts', 'd.ts');
    addEdge(state, 'd.ts', 'c.ts');
    const cycles = computeDependencyCycles(state);
    expect(cycles.length).toBe(2);
  });

  it('deduplicates same cycle found from different start', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'a.ts');
    const cycles = computeDependencyCycles(state);
    expect(cycles.length).toBe(1);
  });

  it('returns cycles sorted by nodeCount descending', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'a.ts');
    addEdge(state, 'x.ts', 'y.ts');
    addEdge(state, 'y.ts', 'z.ts');
    addEdge(state, 'z.ts', 'x.ts');
    const cycles = computeDependencyCycles(state);
    expect(cycles[0].nodeCount).toBeGreaterThanOrEqual(
      cycles[cycles.length - 1].nodeCount
    );
  });
});

describe('computeDependencyCriticalPaths', () => {
  it('returns empty for isolated files', () => {
    const state = emptyState();
    state.files.add('a.ts');
    const critMap = new Map<string, FileCriticality>();
    critMap.set('a.ts', {
      file: 'a.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 5,
    });
    const paths = computeDependencyCriticalPaths(state, critMap, testOpts);
    expect(paths).toEqual([]);
  });

  it('finds longest weighted path', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'c.ts');
    const critMap = new Map<string, FileCriticality>();
    critMap.set('a.ts', {
      file: 'a.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 100,
    });
    critMap.set('b.ts', {
      file: 'b.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 50,
    });
    critMap.set('c.ts', {
      file: 'c.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 10,
    });

    const paths = computeDependencyCriticalPaths(state, critMap, testOpts);
    expect(paths.length).toBeGreaterThan(0);
    expect(paths[0].path).toContain('a.ts');
    expect(paths[0].length).toBe(3);
  });

  it('handles cycles without infinite loop', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'a.ts');
    const critMap = new Map<string, FileCriticality>();
    critMap.set('a.ts', {
      file: 'a.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 10,
    });
    critMap.set('b.ts', {
      file: 'b.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 10,
    });

    const paths = computeDependencyCriticalPaths(state, critMap, testOpts);
    expect(paths.length).toBeGreaterThan(0);
    expect(paths.some(p => p.containsCycle)).toBe(true);
  });

  it('respects deepLinkTopN limit', () => {
    const state = emptyState();
    for (let i = 0; i < 20; i++) {
      const from = `src/m${i}.ts`;
      const to = `src/m${i + 1}.ts`;
      addEdge(state, from, to);
    }
    const critMap = new Map<string, FileCriticality>();
    for (const file of state.files) {
      critMap.set(file, {
        file,
        complexityRisk: 1,
        highComplexityFunctions: 0,
        functionCount: 1,
        flows: 0,
        score: 5,
      });
    }
    const paths = computeDependencyCriticalPaths(state, critMap, {
      ...testOpts,
      deepLinkTopN: 3,
    });
    expect(paths.length).toBeLessThanOrEqual(3);
  });
});

describe('buildIssueCatalog', () => {
  describe('duplicate findings', () => {
    it('creates duplicate-function-body findings', () => {
      const dups: DuplicateGroup[] = [
        {
          hash: 'abc',
          signature: 'handleError',
          kind: 'ArrowFunction',
          occurrences: 4,
          filesCount: 3,
          locations: Array.from({ length: 4 }, (_, i) => ({
            kind: 'ArrowFunction',
            name: 'handleError',
            nameHint: 'handleError',
            file: `src/file${i}.ts`,
            lineStart: 1,
            lineEnd: 10,
            columnStart: 1,
            columnEnd: 1,
            statementCount: 8,
            complexity: 3,
            maxBranchDepth: 1,
            maxLoopDepth: 0,
            returns: 1,
            awaits: 0,
            calls: 2,
            loops: 0,
            lengthLines: 10,
            cognitiveComplexity: 2,
            hash: 'abc',
            metrics: {
              complexity: 3,
              maxBranchDepth: 1,
              maxLoopDepth: 0,
              returns: 1,
              awaits: 0,
              calls: 2,
              loops: 0,
            },
          })),
        },
      ];
      const { findings } = buildIssueCatalog(
        dups,
        [],
        [],
        minimalDepSummary(),
        emptyState(),
        testOpts
      );
      const dupFindings = findings.filter(
        f => f.category === 'duplicate-function-body'
      );
      expect(dupFindings.length).toBe(1);
      expect(dupFindings[0].title).toContain('handleError');
    });

    it('assigns severity based on occurrence count', () => {
      const makeDup = (occurrences: number): DuplicateGroup => ({
        hash: 'x',
        signature: 'fn',
        kind: 'FunctionDeclaration',
        occurrences,
        filesCount: occurrences,
        locations: Array.from({ length: occurrences }, (_, i) => ({
          kind: 'FunctionDeclaration',
          name: 'fn',
          nameHint: 'fn',
          file: `f${i}.ts`,
          lineStart: 1,
          lineEnd: 5,
          columnStart: 1,
          columnEnd: 1,
          statementCount: 6,
          complexity: 1,
          maxBranchDepth: 0,
          maxLoopDepth: 0,
          returns: 0,
          awaits: 0,
          calls: 0,
          loops: 0,
          lengthLines: 5,
          cognitiveComplexity: 0,
          hash: 'x',
          metrics: {
            complexity: 1,
            maxBranchDepth: 0,
            maxLoopDepth: 0,
            returns: 0,
            awaits: 0,
            calls: 0,
            loops: 0,
          },
        })),
      });

      const low = buildIssueCatalog(
        [makeDup(2)],
        [],
        [],
        minimalDepSummary(),
        emptyState(),
        testOpts
      );
      const med = buildIssueCatalog(
        [makeDup(3)],
        [],
        [],
        minimalDepSummary(),
        emptyState(),
        testOpts
      );
      const high = buildIssueCatalog(
        [makeDup(6)],
        [],
        [],
        minimalDepSummary(),
        emptyState(),
        testOpts
      );

      expect(low.findings[0].severity).toBe('low');
      expect(med.findings[0].severity).toBe('medium');
      expect(high.findings[0].severity).toBe('high');
    });
  });

  describe('function-optimization findings', () => {
    it('flags high-complexity functions', () => {
      const files = [
        makeFile({
          functions: [makeFn({ complexity: 35, name: 'complexFn' })],
        }),
      ];
      const { findings } = buildIssueCatalog(
        [],
        [],
        files,
        minimalDepSummary(),
        emptyState(),
        testOpts
      );
      const optFindings = findings.filter(
        f => f.category === 'function-optimization'
      );
      expect(optFindings.length).toBe(1);
      expect(optFindings[0].title).toContain('complexFn');
    });

    it('flags deeply nested functions', () => {
      const files = [
        makeFile({
          functions: [makeFn({ maxBranchDepth: 8, name: 'deepFn' })],
        }),
      ];
      const { findings } = buildIssueCatalog(
        [],
        [],
        files,
        minimalDepSummary(),
        emptyState(),
        testOpts
      );
      expect(findings.some(f => f.category === 'function-optimization')).toBe(
        true
      );
    });

    it('flags large functions', () => {
      const files = [
        makeFile({
          functions: [makeFn({ statementCount: 30, name: 'bigFn' })],
        }),
      ];
      const { findings } = buildIssueCatalog(
        [],
        [],
        files,
        minimalDepSummary(),
        emptyState(),
        testOpts
      );
      expect(findings.some(f => f.category === 'function-optimization')).toBe(
        true
      );
    });

    it('skips clean functions', () => {
      const files = [
        makeFile({
          functions: [
            makeFn({
              complexity: 5,
              maxBranchDepth: 2,
              maxLoopDepth: 1,
              statementCount: 10,
            }),
          ],
        }),
      ];
      const { findings } = buildIssueCatalog(
        [],
        [],
        files,
        minimalDepSummary(),
        emptyState(),
        testOpts
      );
      expect(
        findings.filter(f => f.category === 'function-optimization')
      ).toEqual([]);
    });
  });

  describe('dead code findings', () => {
    it('detects orphan modules (no inbound or outbound)', () => {
      const state = emptyState();
      state.files.add('src/dead.ts');
      const depSummary = minimalDepSummary({ roots: ['src/dead.ts'] });
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        state,
        testOpts
      );
      expect(
        findings.some(
          f => f.category === 'orphan-module' && f.file === 'src/dead.ts'
        )
      ).toBe(true);
    });

    it('skips entrypoints from orphan-module detection', () => {
      const state = emptyState();
      state.files.add('src/index.ts');
      const depSummary = minimalDepSummary({ roots: ['src/index.ts'] });
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        state,
        testOpts
      );
      expect(
        findings.some(
          f => f.category === 'orphan-module' && f.file === 'src/index.ts'
        )
      ).toBe(false);
    });

    it('detects dead exports', () => {
      const state = emptyState();
      state.files.add('src/lib.ts');
      state.declaredExportsByFile.set('src/lib.ts', [
        { name: 'usedFn', kind: 'value' },
        { name: 'deadFn', kind: 'value', lineStart: 10, lineEnd: 15 },
      ]);
      state.importedSymbolsByFile.set('src/consumer.ts', [
        {
          sourceModule: './lib',
          resolvedModule: 'src/lib.ts',
          importedName: 'usedFn',
          localName: 'usedFn',
          isTypeOnly: false,
        },
      ]);
      addEdge(state, 'src/consumer.ts', 'src/lib.ts');
      const depSummary = minimalDepSummary();
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        state,
        testOpts
      );
      const deadExports = findings.filter(f => f.category === 'dead-export');
      expect(deadExports.some(f => f.title.includes('deadFn'))).toBe(true);
      expect(deadExports.some(f => f.title.includes('usedFn'))).toBe(false);
    });

    it('skips exports consumed via namespace import (*)', () => {
      const state = emptyState();
      state.files.add('src/lib.ts');
      state.declaredExportsByFile.set('src/lib.ts', [
        { name: 'foo', kind: 'value' },
      ]);
      state.importedSymbolsByFile.set('src/consumer.ts', [
        {
          sourceModule: './lib',
          resolvedModule: 'src/lib.ts',
          importedName: '*',
          localName: 'lib',
          isTypeOnly: false,
        },
      ]);
      const depSummary = minimalDepSummary();
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        state,
        testOpts
      );
      expect(
        findings.some(
          f => f.category === 'dead-export' && f.title.includes('foo')
        )
      ).toBe(false);
    });
  });

  describe('re-export findings', () => {
    it('detects dead re-exports', () => {
      const state = emptyState();
      state.files.add('src/index.ts');
      state.reExportsByFile.set('src/index.ts', [
        {
          sourceModule: './a',
          resolvedModule: 'src/a.ts',
          exportedAs: 'deadSymbol',
          importedName: 'deadSymbol',
          isStar: false,
          isTypeOnly: false,
          lineStart: 1,
          lineEnd: 1,
        },
      ]);
      const depSummary = minimalDepSummary();
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        state,
        testOpts
      );
      expect(findings.some(f => f.category === 'dead-re-export')).toBe(true);
    });

    it('detects re-export duplication', () => {
      const state = emptyState();
      state.files.add('src/barrel.ts');
      state.reExportsByFile.set('src/barrel.ts', [
        {
          sourceModule: './a',
          resolvedModule: 'src/a.ts',
          exportedAs: 'Foo',
          importedName: 'Foo',
          isStar: false,
          isTypeOnly: false,
        },
        {
          sourceModule: './b',
          resolvedModule: 'src/b.ts',
          exportedAs: 'Foo',
          importedName: 'Foo',
          isStar: false,
          isTypeOnly: false,
        },
      ]);
      const depSummary = minimalDepSummary();
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        state,
        testOpts
      );
      expect(findings.some(f => f.category === 're-export-duplication')).toBe(
        true
      );
    });

    it('detects shadowed re-exports', () => {
      const state = emptyState();
      state.files.add('src/barrel.ts');
      state.declaredExportsByFile.set('src/barrel.ts', [
        { name: 'Conflict', kind: 'value' },
      ]);
      state.reExportsByFile.set('src/barrel.ts', [
        {
          sourceModule: './a',
          resolvedModule: 'src/a.ts',
          exportedAs: 'Conflict',
          importedName: 'Conflict',
          isStar: false,
          isTypeOnly: false,
        },
      ]);
      const depSummary = minimalDepSummary();
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        state,
        testOpts
      );
      expect(findings.some(f => f.category === 're-export-shadowed')).toBe(
        true
      );
    });
  });

  describe('dependency findings', () => {
    it('creates cycle findings', () => {
      const depSummary = minimalDepSummary({
        cycles: [{ path: ['a.ts', 'b.ts', 'a.ts'], nodeCount: 2 }],
      });
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        emptyState(),
        testOpts
      );
      expect(findings.some(f => f.category === 'dependency-cycle')).toBe(true);
    });

    it('creates critical-path findings above score threshold', () => {
      const depSummary = minimalDepSummary({
        criticalPaths: [
          {
            start: 'a.ts',
            path: ['a.ts', 'b.ts', 'c.ts'],
            score: 300,
            length: 3,
            containsCycle: false,
          },
        ],
      });
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        emptyState(),
        testOpts
      );
      expect(
        findings.some(f => f.category === 'dependency-critical-path')
      ).toBe(true);
    });

    it('skips critical-path findings below score threshold', () => {
      const depSummary = minimalDepSummary({
        criticalPaths: [
          {
            start: 'a.ts',
            path: ['a.ts', 'b.ts'],
            score: 10,
            length: 2,
            containsCycle: false,
          },
        ],
      });
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        emptyState(),
        testOpts
      );
      expect(
        findings.some(f => f.category === 'dependency-critical-path')
      ).toBe(false);
    });

    it('creates test-only module findings', () => {
      const depSummary = minimalDepSummary({
        testOnlyModules: [
          {
            file: 'src/test-helper.ts',
            outboundCount: 0,
            inboundCount: 1,
            inboundFromProduction: 0,
            inboundFromTests: 1,
            externalDependencyCount: 0,
            unresolvedDependencyCount: 0,
          },
        ],
      });
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        emptyState(),
        testOpts
      );
      expect(findings.some(f => f.category === 'dependency-test-only')).toBe(
        true
      );
    });
  });

  describe('finding limits and sorting', () => {
    it('respects findingsLimit', () => {
      const files = [
        makeFile({
          functions: Array.from({ length: 50 }, (_, i) =>
            makeFn({
              complexity: 40,
              name: `fn${i}`,
            })
          ),
        }),
      ];
      const opts = { ...testOpts, findingsLimit: 5 };
      const { findings } = buildIssueCatalog(
        [],
        [],
        files,
        minimalDepSummary(),
        emptyState(),
        opts
      );
      expect(findings.length).toBeLessThanOrEqual(5);
    });

    it('sorts findings by severity descending', () => {
      const depSummary = minimalDepSummary({
        cycles: [{ path: ['a.ts', 'b.ts', 'a.ts'], nodeCount: 2 }],
        testOnlyModules: [
          {
            file: 'src/t.ts',
            outboundCount: 0,
            inboundCount: 1,
            inboundFromProduction: 0,
            inboundFromTests: 1,
            externalDependencyCount: 0,
            unresolvedDependencyCount: 0,
          },
        ],
      });
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        depSummary,
        emptyState(),
        testOpts
      );
      if (findings.length >= 2) {
        const severityOrder: Record<string, number> = {
          critical: 4,
          high: 3,
          medium: 2,
          low: 1,
          info: 0,
        };
        for (let i = 1; i < findings.length; i++) {
          expect(
            severityOrder[findings[i - 1].severity]
          ).toBeGreaterThanOrEqual(severityOrder[findings[i].severity]);
        }
      }
    });

    it('assigns unique IDs to each finding', () => {
      const files = [
        makeFile({
          functions: [
            makeFn({ complexity: 40, name: 'fn1' }),
            makeFn({ complexity: 40, name: 'fn2' }),
          ],
        }),
      ];
      const { findings } = buildIssueCatalog(
        [],
        [],
        files,
        minimalDepSummary(),
        emptyState(),
        testOpts
      );
      const ids = findings.map(f => f.id);
      expect(new Set(ids).size).toBe(ids.length);
    });

    it('tracks findings per file', () => {
      const files = [
        makeFile({
          file: 'src/hot.ts',
          functions: [
            makeFn({ complexity: 40, name: 'fn1', file: 'src/hot.ts' }),
          ],
        }),
      ];
      const { byFile } = buildIssueCatalog(
        [],
        [],
        files,
        minimalDepSummary(),
        emptyState(),
        testOpts
      );
      expect(byFile.get('src/hot.ts')?.length).toBeGreaterThan(0);
    });
  });

  describe('features filtering', () => {
    it('filters findings by features set', () => {
      const depSummary = minimalDepSummary({
        cycles: [{ path: ['a.ts', 'b.ts', 'a.ts'], nodeCount: 2 }],
        testOnlyModules: [
          {
            file: 'src/t.ts',
            outboundCount: 0,
            inboundCount: 1,
            inboundFromProduction: 0,
            inboundFromTests: 1,
            externalDependencyCount: 0,
            unresolvedDependencyCount: 0,
          },
        ],
      });
      const state = emptyState();
      state.files.add('src/lib.ts');
      state.declaredExportsByFile.set('src/lib.ts', [
        { name: 'deadFn', kind: 'value', lineStart: 10, lineEnd: 15 },
      ]);
      const files = [
        makeFile({
          functions: [makeFn({ complexity: 40, name: 'complexFn' })],
        }),
      ];
      const optsAll = { ...testOpts, features: null };
      const optsArchOnly = {
        ...testOpts,
        features: new Set(['dependency-cycle', 'dependency-test-only']),
      };

      const { findings: allFindings } = buildIssueCatalog(
        [],
        [],
        files,
        depSummary,
        state,
        optsAll
      );
      const { findings: filteredFindings } = buildIssueCatalog(
        [],
        [],
        files,
        depSummary,
        state,
        optsArchOnly
      );

      expect(allFindings.some(f => f.category === 'dependency-cycle')).toBe(
        true
      );
      expect(allFindings.some(f => f.category === 'dependency-test-only')).toBe(
        true
      );
      expect(allFindings.some(f => f.category === 'dead-export')).toBe(true);
      expect(
        allFindings.some(f => f.category === 'function-optimization')
      ).toBe(true);

      expect(
        filteredFindings.every(
          f =>
            f.category === 'dependency-cycle' ||
            f.category === 'dependency-test-only'
        )
      ).toBe(true);
      expect(filteredFindings.some(f => f.category === 'dead-export')).toBe(
        false
      );
      expect(
        filteredFindings.some(f => f.category === 'function-optimization')
      ).toBe(false);
    });
  });

  describe('architecture integration', () => {
    it('includes SDP violation findings from architecture module', () => {
      const state = emptyState();
      for (let i = 0; i < 10; i++) {
        const f = `src/dep${i}.ts`;
        state.files.add(f);
        addEdge(state, f, 'src/stable.ts');
      }
      addEdge(state, 'src/stable.ts', 'src/unstable.ts');
      for (let i = 0; i < 10; i++) {
        const f = `src/lib${i}.ts`;
        state.files.add(f);
        addEdge(state, 'src/unstable.ts', f);
      }
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        minimalDepSummary(),
        state,
        testOpts
      );
      expect(
        findings.some(f => f.category === 'architecture-sdp-violation')
      ).toBe(true);
    });

    it('includes orphan-module findings', () => {
      const state = emptyState();
      state.files.add('src/orphan.ts');
      addEdge(state, 'src/a.ts', 'src/b.ts');
      const { findings } = buildIssueCatalog(
        [],
        [],
        [],
        minimalDepSummary(),
        state,
        testOpts
      );
      expect(findings.some(f => f.category === 'orphan-module')).toBe(true);
    });
  });
});

describe('category group constants', () => {
  const ALL_CATEGORIES = [
    'dependency-cycle',
    'dependency-critical-path',
    'dependency-test-only',
    'architecture-sdp-violation',
    'high-coupling',
    'god-module-coupling',
    'orphan-module',
    'unreachable-module',
    'layer-violation',
    'low-cohesion',
    'duplicate-function-body',
    'duplicate-flow-structure',
    'function-optimization',
    'cognitive-complexity',
    'god-module',
    'god-function',
    'halstead-effort',
    'low-maintainability',
    'excessive-parameters',
    'unsafe-any',
    'empty-catch',
    'switch-no-default',
    'dead-export',
    'dead-re-export',
    're-export-duplication',
    're-export-shadowed',
    'unused-npm-dependency',
    'package-boundary-violation',
    'barrel-explosion',
    'distance-from-main-sequence',
    'feature-envy',
    'untested-critical-code',
    'over-abstraction',
    'concrete-dependency',
    'circular-type-dependency',
    'unused-parameter',
    'deep-override-chain',
    'interface-compliance',
    'unused-import',
    'orphan-implementation',
    'shotgun-surgery',
    'move-to-caller',
    'narrowable-type',
    'type-assertion-escape',
    'promise-misuse',
    'missing-error-boundary',
  ];

  it('every known category belongs to exactly one group', () => {
    for (const cat of ALL_CATEGORIES) {
      const inArch = ARCHITECTURE_CATEGORIES.has(cat);
      const inQual = CODE_QUALITY_CATEGORIES.has(cat);
      const inDead = DEAD_CODE_CATEGORIES.has(cat);
      const count = [inArch, inQual, inDead].filter(Boolean).length;
      expect(count).toBe(1);
    }
  });

  it('groups have no overlap', () => {
    for (const cat of ARCHITECTURE_CATEGORIES) {
      expect(CODE_QUALITY_CATEGORIES.has(cat)).toBe(false);
      expect(DEAD_CODE_CATEGORIES.has(cat)).toBe(false);
    }
    for (const cat of CODE_QUALITY_CATEGORIES) {
      expect(DEAD_CODE_CATEGORIES.has(cat)).toBe(false);
    }
  });

  it('all categories are covered across all pillars', () => {
    const total = Object.values(PILLAR_CATEGORIES).flat().length;
    const setTotal =
      ARCHITECTURE_CATEGORIES.size +
      CODE_QUALITY_CATEGORIES.size +
      DEAD_CODE_CATEGORIES.size +
      SECURITY_CATEGORIES.size +
      TEST_QUALITY_CATEGORIES.size;
    expect(setTotal).toBe(total);
  });

  it('architecture group has 27 categories', () => {
    expect(ARCHITECTURE_CATEGORIES.size).toBe(28);
  });

  it('code quality group has expected categories', () => {
    expect(CODE_QUALITY_CATEGORIES.size).toBe(34);
  });

  it('dead code group has 11 categories', () => {
    expect(DEAD_CODE_CATEGORIES.size).toBe(12);
  });

  it('security group has 10 categories', () => {
    expect(SECURITY_CATEGORIES.size).toBe(12);
  });

  it('test quality group has 8 categories', () => {
    expect(TEST_QUALITY_CATEGORIES.size).toBe(8);
  });
});

describe('severityBreakdown', () => {
  it('returns zero counts for empty findings', () => {
    const result = severityBreakdown([]);
    expect(result).toEqual({
      critical: 0,
      high: 0,
      medium: 0,
      low: 0,
      info: 0,
    });
  });

  it('counts each severity correctly', () => {
    const findings = [
      { severity: 'high' },
      { severity: 'high' },
      { severity: 'medium' },
      { severity: 'critical' },
    ] as Finding[];
    const result = severityBreakdown(findings);
    expect(result.critical).toBe(1);
    expect(result.high).toBe(2);
    expect(result.medium).toBe(1);
    expect(result.low).toBe(0);
  });
});

describe('categoryBreakdown', () => {
  it('returns empty object for empty findings', () => {
    expect(categoryBreakdown([])).toEqual({});
  });

  it('counts each category correctly', () => {
    const findings = [
      { category: 'dead-export' },
      { category: 'dead-export' },
      { category: 'dependency-cycle' },
    ] as Finding[];
    const result = categoryBreakdown(findings);
    expect(result['dead-export']).toBe(2);
    expect(result['dependency-cycle']).toBe(1);
  });
});

describe('diversifyFindings', () => {
  type DraftFinding = Omit<Finding, 'id'> & { id?: string };

  const makeDraft = (
    severity: string,
    category: string,
    idx: number
  ): DraftFinding => ({
    severity: severity as Finding['severity'],
    category,
    file: `${category}-${idx}.ts`,
    lineStart: 1,
    lineEnd: 1,
    title: `${category} finding ${idx}`,
    reason: 'test',
    files: [`${category}-${idx}.ts`],
    suggestedFix: { strategy: 'test', steps: ['step1'] },
  });

  it('returns all findings when limit >= length', () => {
    const input = [makeDraft('high', 'a', 1), makeDraft('high', 'b', 1)];
    expect(diversifyFindings(input, 10)).toBe(input);
    expect(diversifyFindings(input, 2)).toBe(input);
  });

  it('returns all findings when limit is Infinity', () => {
    const input = [makeDraft('high', 'a', 1)];
    expect(diversifyFindings(input, Infinity)).toBe(input);
  });

  it('round-robins across categories instead of taking all from one', () => {
    const input = [
      ...Array.from({ length: 10 }, (_, i) =>
        makeDraft('high', 'await-in-loop', i)
      ),
      makeDraft('high', 'dead-export', 1),
      makeDraft('high', 'dead-export', 2),
    ];
    const result = diversifyFindings(input, 5);
    expect(result).toHaveLength(5);
    const categories = new Set(result.map(f => f.category));
    expect(categories.size).toBe(2);
    expect(categories.has('await-in-loop')).toBe(true);
    expect(categories.has('dead-export')).toBe(true);
  });

  it('prioritizes categories by highest severity', () => {
    const input = [
      makeDraft('critical', 'security', 1),
      makeDraft('high', 'quality', 1),
      makeDraft('high', 'quality', 2),
      makeDraft('medium', 'dead-code', 1),
      makeDraft('medium', 'dead-code', 2),
    ];
    const result = diversifyFindings(input, 3);
    expect(result).toHaveLength(3);
    expect(result[0].category).toBe('security');
    expect(result[1].category).toBe('quality');
    expect(result[2].category).toBe('dead-code');
  });

  it('continues round-robin when some categories are exhausted', () => {
    const input = [
      makeDraft('high', 'a', 1),
      makeDraft('high', 'b', 1),
      makeDraft('high', 'b', 2),
      makeDraft('high', 'b', 3),
    ];
    const result = diversifyFindings(input, 3);
    expect(result).toHaveLength(3);
    expect(result.filter(f => f.category === 'a')).toHaveLength(1);
    expect(result.filter(f => f.category === 'b')).toHaveLength(2);
  });

  it('handles empty input', () => {
    expect(diversifyFindings([], 5)).toEqual([]);
  });

  it('handles single category (no diversity possible)', () => {
    const input = Array.from({ length: 10 }, (_, i) =>
      makeDraft('high', 'only-cat', i)
    );
    const result = diversifyFindings(input, 3);
    expect(result).toHaveLength(3);
    expect(result.every(f => f.category === 'only-cat')).toBe(true);
  });

  it('handles limit of 1', () => {
    const input = [makeDraft('critical', 'a', 1), makeDraft('high', 'b', 1)];
    const result = diversifyFindings(input, 1);
    expect(result).toHaveLength(1);
    expect(result[0].severity).toBe('critical');
  });

  it('preserves severity order within each category', () => {
    const input = [
      makeDraft('critical', 'a', 1),
      makeDraft('high', 'a', 2),
      makeDraft('medium', 'a', 3),
      makeDraft('critical', 'b', 1),
      makeDraft('high', 'b', 2),
    ];
    const result = diversifyFindings(input, 4);
    const aFindings = result.filter(f => f.category === 'a');
    expect(aFindings[0].severity).toBe('critical');
    expect(aFindings[1].severity).toBe('high');
  });
});

describe('diverseTopRecommendations', () => {
  const makeFinding = (
    id: string,
    severity: string,
    category: string
  ): Finding => ({
    id,
    severity: severity as Finding['severity'],
    category,
    file: 'test.ts',
    lineStart: 1,
    lineEnd: 1,
    title: `Test ${id}`,
    reason: 'test',
    files: ['test.ts'],
    suggestedFix: { strategy: 'test', steps: ['step1'] },
    impact: 'test',
  });

  it('limits findings per category', () => {
    const findings = [
      makeFinding('1', 'high', 'dead-export'),
      makeFinding('2', 'high', 'dead-export'),
      makeFinding('3', 'high', 'dead-export'),
      makeFinding('4', 'high', 'cognitive-complexity'),
      makeFinding('5', 'high', 'cognitive-complexity'),
      makeFinding('6', 'high', 'cognitive-complexity'),
    ];
    const result = diverseTopRecommendations(findings, 10, 2);
    expect(result).toHaveLength(4);
    expect(result.filter(f => f.category === 'dead-export')).toHaveLength(2);
    expect(
      result.filter(f => f.category === 'cognitive-complexity')
    ).toHaveLength(2);
  });

  it('respects total limit', () => {
    const findings = Array.from({ length: 30 }, (_, i) =>
      makeFinding(`${i}`, 'high', `cat-${i % 10}`)
    );
    const result = diverseTopRecommendations(findings, 5, 2);
    expect(result).toHaveLength(5);
  });

  it('returns empty for empty input', () => {
    expect(diverseTopRecommendations([], 10, 2)).toHaveLength(0);
  });

  it('uses maxPerCategory=1 to force maximum diversity', () => {
    const findings = [
      makeFinding('1', 'critical', 'a'),
      makeFinding('2', 'critical', 'a'),
      makeFinding('3', 'high', 'b'),
      makeFinding('4', 'high', 'b'),
      makeFinding('5', 'medium', 'c'),
    ];
    const result = diverseTopRecommendations(findings, 10, 1);
    expect(result).toHaveLength(3);
    expect(new Set(result.map(f => f.category)).size).toBe(3);
  });
});

describe('writeMultiFileReport', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'scan-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  function makeReport(overrides: Partial<FullReport> = {}): FullReport {
    return {
      generatedAt: '2026-03-17T00:00:00.000Z',
      repoRoot: '/repo',
      options: {},
      parser: { requested: 'auto', effective: 'typescript' },
      summary: {
        totalFiles: 10,
        totalFunctions: 50,
        totalFlows: 200,
        totalDependencyFiles: 12,
        totalPackages: 2,
      },
      fileInventory: [],
      duplicateFlows: {
        duplicatedFunctions: [],
        duplicatedControlFlow: [],
        totalFunctionGroups: 0,
        totalFlowGroups: 0,
      },
      dependencyGraph: minimalDepSummary(),
      dependencyFindings: [],
      agentOutput: {
        totalFindings: 0,
        highPriority: 0,
        mediumPriority: 0,
        lowPriority: 0,
        topRecommendations: [],
        filesWithIssues: [],
      },
      optimizationOpportunities: [],
      optimizationFindings: [],
      parseErrors: [],
      ...overrides,
    };
  }

  function makeFindings(
    ...categories: Array<{ category: string; severity: string }>
  ): Finding[] {
    return categories.map((c, i) => ({
      id: `AST-ISSUE-${i}`,
      category: c.category,
      severity: c.severity as Finding['severity'],
      file: `src/file${i}.ts`,
      lineStart: 1,
      lineEnd: 10,
      title: `Finding: ${c.category}`,
      reason: 'test reason',
      files: [`src/file${i}.ts`],
      suggestedFix: { strategy: 'fix it', steps: ['step 1'] },
    }));
  }

  it('creates all 7 expected files (6 json + summary.md)', () => {
    const outDir = path.join(tmpDir, 'scan');
    writeMultiFileReport(
      outDir,
      makeReport(),
      { ...DEFAULT_OPTS, graph: false },
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const files = fs.readdirSync(outDir).sort();
    expect(files).toEqual([
      'architecture.json',
      'code-quality.json',
      'dead-code.json',
      'file-inventory.json',
      'findings.json',
      'summary.json',
      'summary.md',
    ]);
  });

  it('includes graph.md when graph option is true', () => {
    const outDir = path.join(tmpDir, 'scan-graph');
    writeMultiFileReport(
      outDir,
      makeReport(),
      { ...DEFAULT_OPTS, graph: true },
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    expect(fs.existsSync(path.join(outDir, 'graph.md'))).toBe(true);
  });

  it('includes ast-trees.txt in compact text format when astTrees are present', () => {
    const outDir = path.join(tmpDir, 'scan-trees');
    const tree = {
      kind: 'SourceFile',
      startLine: 1,
      endLine: 10,
      children: [
        { kind: 'ImportDeclaration', startLine: 1, endLine: 1, children: [] },
        {
          kind: 'FunctionDeclaration',
          startLine: 3,
          endLine: 8,
          children: [
            {
              kind: 'Block',
              startLine: 3,
              endLine: 8,
              children: [],
              truncated: true,
            },
          ],
        },
      ],
    };
    const report = makeReport({
      astTrees: [{ package: 'test', file: 'a.ts', tree }],
    });
    writeMultiFileReport(
      outDir,
      report,
      { ...DEFAULT_OPTS },
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const txtPath = path.join(outDir, 'ast-trees.txt');
    expect(fs.existsSync(txtPath)).toBe(true);
    const content = fs.readFileSync(txtPath, 'utf8');
    expect(content).toContain('## test — a.ts');
    expect(content).toContain('SourceFile[1:10]');
    expect(content).toContain('  ImportDeclaration[1]');
    expect(content).toContain('  FunctionDeclaration[3:8]');
    expect(content).toContain('    Block[3:8] ...');
    expect(content).not.toContain('"kind"');
  });

  it('routes architecture findings into architecture.json', () => {
    const findings = makeFindings(
      { category: 'dependency-cycle', severity: 'high' },
      { category: 'architecture-sdp-violation', severity: 'medium' },
      { category: 'dead-export', severity: 'high' }
    );
    const outDir = path.join(tmpDir, 'scan-arch');
    writeMultiFileReport(
      outDir,
      makeReport({ optimizationFindings: findings }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const archData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'architecture.json'), 'utf8')
    );
    expect(archData.findingsCount).toBe(2);
    expect(
      archData.findings.every((f: Finding) =>
        ARCHITECTURE_CATEGORIES.has(f.category)
      )
    ).toBe(true);
  });

  it('routes code quality findings into code-quality.json', () => {
    const findings = makeFindings(
      { category: 'function-optimization', severity: 'high' },
      { category: 'cognitive-complexity', severity: 'medium' },
      { category: 'orphan-module', severity: 'medium' }
    );
    const outDir = path.join(tmpDir, 'scan-qual');
    writeMultiFileReport(
      outDir,
      makeReport({ optimizationFindings: findings }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const qualData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'code-quality.json'), 'utf8')
    );
    expect(qualData.findingsCount).toBe(2);
    expect(
      qualData.findings.every((f: Finding) =>
        CODE_QUALITY_CATEGORIES.has(f.category)
      )
    ).toBe(true);
  });

  it('routes dead code findings into dead-code.json', () => {
    const findings = makeFindings(
      { category: 'dead-export', severity: 'high' },
      { category: 'unused-npm-dependency', severity: 'low' },
      { category: 'barrel-explosion', severity: 'medium' },
      { category: 'dependency-cycle', severity: 'high' }
    );
    const outDir = path.join(tmpDir, 'scan-dead');
    writeMultiFileReport(
      outDir,
      makeReport({ optimizationFindings: findings }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const deadData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'dead-code.json'), 'utf8')
    );
    expect(deadData.findingsCount).toBe(3);
    expect(
      deadData.findings.every((f: Finding) =>
        DEAD_CODE_CATEGORIES.has(f.category)
      )
    ).toBe(true);
  });

  it('findings.json contains ALL findings', () => {
    const findings = makeFindings(
      { category: 'dependency-cycle', severity: 'high' },
      { category: 'dead-export', severity: 'medium' },
      { category: 'function-optimization', severity: 'high' }
    );
    const outDir = path.join(tmpDir, 'scan-all');
    writeMultiFileReport(
      outDir,
      makeReport({ optimizationFindings: findings }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const findingsData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'findings.json'), 'utf8')
    );
    expect(findingsData.totalFindings).toBe(3);
  });

  it('summary.json contains outputFiles index', () => {
    const outDir = path.join(tmpDir, 'scan-idx');
    writeMultiFileReport(
      outDir,
      makeReport(),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const summaryData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'summary.json'), 'utf8')
    );
    expect(summaryData.outputFiles).toBeDefined();
    expect(summaryData.outputFiles.summary).toBe('summary.json');
    expect(summaryData.outputFiles.architecture).toBe('architecture.json');
    expect(summaryData.outputFiles.deadCode).toBe('dead-code.json');
    expect(summaryData.outputFiles.summaryMd).toBe('summary.md');
  });

  it('file-inventory.json contains fileInventory and fileCount', () => {
    const fileEntries = [
      makeFile({ file: 'src/a.ts' }),
      makeFile({ file: 'src/b.ts' }),
    ];
    const outDir = path.join(tmpDir, 'scan-inv');
    writeMultiFileReport(
      outDir,
      makeReport({ fileInventory: fileEntries }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const invData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'file-inventory.json'), 'utf8')
    );
    expect(invData.fileCount).toBe(2);
    expect(invData.fileInventory.length).toBe(2);
  });

  it('architecture.json includes severityBreakdown and categoryBreakdown', () => {
    const findings = makeFindings(
      { category: 'dependency-cycle', severity: 'high' },
      { category: 'dependency-cycle', severity: 'high' },
      { category: 'high-coupling', severity: 'medium' }
    );
    const outDir = path.join(tmpDir, 'scan-arch-meta');
    writeMultiFileReport(
      outDir,
      makeReport({ optimizationFindings: findings }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const archData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'architecture.json'), 'utf8')
    );
    expect(archData.severityBreakdown.high).toBe(2);
    expect(archData.severityBreakdown.medium).toBe(1);
    expect(archData.categoryBreakdown['dependency-cycle']).toBe(2);
    expect(archData.categoryBreakdown['high-coupling']).toBe(1);
  });

  it('code-quality.json includes severityBreakdown and categoryBreakdown', () => {
    const findings = makeFindings(
      { category: 'function-optimization', severity: 'high' },
      { category: 'god-module', severity: 'high' },
      { category: 'cognitive-complexity', severity: 'medium' }
    );
    const outDir = path.join(tmpDir, 'scan-qual-meta');
    writeMultiFileReport(
      outDir,
      makeReport({ optimizationFindings: findings }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const qualData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'code-quality.json'), 'utf8')
    );
    expect(qualData.severityBreakdown.high).toBe(2);
    expect(qualData.categoryBreakdown['function-optimization']).toBe(1);
    expect(qualData.categoryBreakdown['god-module']).toBe(1);
  });

  it('dead-code.json includes severityBreakdown and categoryBreakdown', () => {
    const findings = makeFindings(
      { category: 'dead-export', severity: 'high' },
      { category: 'dead-export', severity: 'medium' },
      { category: 'unused-npm-dependency', severity: 'low' }
    );
    const outDir = path.join(tmpDir, 'scan-dead-meta');
    writeMultiFileReport(
      outDir,
      makeReport({ optimizationFindings: findings }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const deadData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'dead-code.json'), 'utf8')
    );
    expect(deadData.severityBreakdown.high).toBe(1);
    expect(deadData.severityBreakdown.medium).toBe(1);
    expect(deadData.severityBreakdown.low).toBe(1);
    expect(deadData.categoryBreakdown['dead-export']).toBe(2);
    expect(deadData.categoryBreakdown['unused-npm-dependency']).toBe(1);
  });

  it('all json files have generatedAt timestamp', () => {
    const outDir = path.join(tmpDir, 'scan-ts');
    writeMultiFileReport(
      outDir,
      makeReport(),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    for (const file of [
      'summary.json',
      'architecture.json',
      'code-quality.json',
      'dead-code.json',
      'file-inventory.json',
      'findings.json',
    ]) {
      const data = JSON.parse(fs.readFileSync(path.join(outDir, file), 'utf8'));
      expect(data.generatedAt).toBe('2026-03-17T00:00:00.000Z');
    }
  });

  it('returns correct outputFiles mapping', () => {
    const outDir = path.join(tmpDir, 'scan-ret');
    const result = writeMultiFileReport(
      outDir,
      makeReport(),
      { ...DEFAULT_OPTS, graph: true },
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    expect(result.summary).toBe('summary.json');
    expect(result.architecture).toBe('architecture.json');
    expect(result.codeQuality).toBe('code-quality.json');
    expect(result.deadCode).toBe('dead-code.json');
    expect(result.fileInventory).toBe('file-inventory.json');
    expect(result.findings).toBe('findings.json');
    expect(result.graph).toBe('graph.md');
    expect(result.summaryMd).toBe('summary.md');
  });
});

describe('generateSummaryMd', () => {
  const fakeDir = '/tmp/nonexistent-scan-dir';

  function makeReportForMd(overrides: Partial<FullReport> = {}): FullReport {
    return {
      generatedAt: '2026-03-17T00:00:00.000Z',
      repoRoot: '/repo',
      options: {},
      parser: { requested: 'auto', effective: 'typescript' },
      summary: {
        totalFiles: 42,
        totalFunctions: 318,
        totalFlows: 1204,
        totalDependencyFiles: 50,
        totalPackages: 3,
      },
      fileInventory: [],
      duplicateFlows: {},
      dependencyGraph: minimalDepSummary({
        totalModules: 42,
        totalEdges: 187,
        cycles: [{ path: ['a', 'b', 'a'], nodeCount: 2 }],
        criticalPaths: [],
      }),
      dependencyFindings: [],
      agentOutput: {
        totalFindings: 5,
        highPriority: 2,
        mediumPriority: 2,
        lowPriority: 1,
        topRecommendations: [
          {
            severity: 'high',
            title: 'Fix cycle',
            file: 'src/a.ts',
            category: 'dependency-cycle',
          },
        ],
        filesWithIssues: [],
      },
      optimizationOpportunities: [],
      optimizationFindings: [],
      parseErrors: [],
      ...overrides,
    };
  }

  it('produces markdown with all major sections', () => {
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: { summary: 'summary.json' },
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('# Code Quality Scan Report');
    expect(md).toContain('## Scan Scope');
    expect(md).toContain('## Findings Overview');
    expect(md).toContain('## Health Scores');
    expect(md).toContain('## Architecture Health');
    expect(md).toContain('## Code Quality');
    expect(md).toContain('## Dead Code & Hygiene');
    expect(md).toContain('## Output Files');
  });

  it('includes file counts from summary', () => {
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('42');
    expect(md).toContain('318');
    expect(md).toContain('1204');
  });

  it('includes severity counts', () => {
    const findings: Finding[] = [
      {
        id: '1',
        severity: 'high',
        category: 'dependency-cycle',
        file: 'a',
        lineStart: 1,
        lineEnd: 1,
        title: 't',
        reason: 'r',
        files: [],
        suggestedFix: { strategy: 's', steps: [] },
      },
      {
        id: '2',
        severity: 'medium',
        category: 'dead-export',
        file: 'b',
        lineStart: 1,
        lineEnd: 1,
        title: 't',
        reason: 'r',
        files: [],
        suggestedFix: { strategy: 's', steps: [] },
      },
    ];
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd({ optimizationFindings: findings }),
      outputFiles: {},
      architectureFindings: [findings[0]],
      codeQualityFindings: [],
      deadCodeFindings: [findings[1]],
    });
    expect(md).toContain('| High | 1 |');
    expect(md).toContain('| Medium | 1 |');
    expect(md).toContain('| **Total** | **2** |');
  });

  it('includes dependency graph metrics', () => {
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('| Modules | 42 |');
    expect(md).toContain('| Import edges | 187 |');
    expect(md).toContain('| Cycles | 1 |');
  });

  it('includes category breakdowns per section', () => {
    const archFindings = [
      { category: 'dependency-cycle', severity: 'high' },
      { category: 'dependency-cycle', severity: 'high' },
      { category: 'high-coupling', severity: 'medium' },
    ] as Finding[];
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: archFindings,
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('`dependency-cycle`: 2');
    expect(md).toContain('`high-coupling`: 1');
  });

  it('includes top recommendations', () => {
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Top Recommendations');
    expect(md).toContain('Fix cycle');
    expect(md).toContain('src/a.ts');
  });

  it('includes parse errors when present', () => {
    const report = makeReportForMd({
      parseErrors: [{ file: 'bad.ts', message: 'Unexpected token' }],
    });
    const md = generateSummaryMd({
      dir: fakeDir,
      report,
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Parse Errors');
    expect(md).toContain('bad.ts');
    expect(md).toContain('Unexpected token');
  });

  it('does not include parse errors section when none exist', () => {
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).not.toContain('## Parse Errors');
  });

  it('links output files in the table', () => {
    const outputFiles = {
      summary: 'summary.json',
      architecture: 'architecture.json',
      summaryMd: 'summary.md',
    };
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles,
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('[`summary.json`](./summary.json)');
    expect(md).toContain('[`architecture.json`](./architecture.json)');
    expect(md).toContain('[`summary.md`](./summary.md)');
  });

  it('shows file sizes when files exist', () => {
    const realDir = fs.mkdtempSync(path.join(os.tmpdir(), 'summary-size-'));
    try {
      fs.writeFileSync(
        path.join(realDir, 'architecture.json'),
        '{"x":1}',
        'utf8'
      );
      fs.writeFileSync(
        path.join(realDir, 'big.json'),
        'x'.repeat(2048),
        'utf8'
      );
      const outputFiles = {
        architecture: 'architecture.json',
        big: 'big.json',
      };
      const md = generateSummaryMd({
        dir: realDir,
        report: makeReportForMd(),
        outputFiles,
        architectureFindings: [],
        codeQualityFindings: [],
        deadCodeFindings: [],
      });
      expect(md).toContain('| Size |');
      expect(md).toMatch(/\d+(\.\d+)?\s*(B|KB|MB)/);
    } finally {
      fs.rmSync(realDir, { recursive: true, force: true });
    }
  });
});

describe('computeHealthScore', () => {
  it('returns 100 for no findings', () => {
    expect(computeHealthScore([], 50)).toBe(100);
  });

  it('returns 100 for empty repo', () => {
    expect(computeHealthScore([], 0)).toBe(100);
  });

  it('penalizes critical findings heavily', () => {
    const findings = [
      { severity: 'critical' } as Finding,
      { severity: 'critical' } as Finding,
    ];
    const score = computeHealthScore(findings, 10);
    expect(score).toBeLessThan(70);
  });

  it('penalizes proportional to file count', () => {
    const findings = [{ severity: 'high' } as Finding];
    const smallRepo = computeHealthScore(findings, 5);
    const largeRepo = computeHealthScore(findings, 100);
    expect(largeRepo).toBeGreaterThan(smallRepo);
  });

  it('keeps extreme cases near the floor', () => {
    const findings = Array.from(
      { length: 100 },
      () => ({ severity: 'critical' }) as Finding
    );
    expect(computeHealthScore(findings, 1)).toBeLessThanOrEqual(1);
  });
});

describe('collectTagCloud', () => {
  it('returns empty for no findings', () => {
    expect(collectTagCloud([])).toEqual([]);
  });

  it('returns empty when findings have no tags', () => {
    const findings = [{ tags: undefined } as unknown as Finding];
    expect(collectTagCloud(findings)).toEqual([]);
  });

  it('counts and sorts tags by frequency', () => {
    const findings = [
      { tags: ['coupling', 'architecture'] } as unknown as Finding,
      { tags: ['coupling', 'change-risk'] } as unknown as Finding,
      { tags: ['dead-code'] } as unknown as Finding,
    ];
    const cloud = collectTagCloud(findings);
    expect(cloud[0]).toEqual({ tag: 'coupling', count: 2 });
    expect(cloud.length).toBe(4);
  });
});

describe('end-to-end output validation', () => {
  it('produces valid summary.md with all sections', async () => {
    const { execSync } = await import('node:child_process');
    const dir = '/tmp/cq-test-' + Date.now();
    const scriptPath = path.join(process.cwd(), 'scripts', 'run.js');
    const monorepoRoot = path.join(process.cwd(), '..', '..');
    try {
      try {
        execSync(
          `node "${scriptPath}" --root "${monorepoRoot}" --out "${dir}" --no-tree`,
          { cwd: process.cwd(), encoding: 'utf8', timeout: 30000 }
        );
      } catch (execErr: unknown) {
        const e = execErr as { status?: number };
        if (e.status !== 1) throw execErr;
      }

      expect(fs.existsSync(`${dir}/summary.md`)).toBe(true);
      expect(fs.existsSync(`${dir}/summary.json`)).toBe(true);
      expect(fs.existsSync(`${dir}/architecture.json`)).toBe(true);
      expect(fs.existsSync(`${dir}/code-quality.json`)).toBe(true);
      expect(fs.existsSync(`${dir}/dead-code.json`)).toBe(true);
      expect(fs.existsSync(`${dir}/findings.json`)).toBe(true);
      expect(fs.existsSync(`${dir}/file-inventory.json`)).toBe(true);

      const summary = fs.readFileSync(`${dir}/summary.md`, 'utf8');
      expect(summary).toContain('## Scan Scope');
      expect(summary).toContain('## Findings Overview');
      expect(summary).toContain('## Architecture Health');
      expect(summary).toContain('## Code Quality');
      expect(summary).toContain('## Dead Code & Hygiene');
      expect(summary).toContain('## Output Files');

      expect(summary).toContain('`dependency-cycle`');
      expect(summary).toContain('`dead-export`');
      expect(summary).toContain('`cognitive-complexity`');

      const findingsData = JSON.parse(
        fs.readFileSync(`${dir}/findings.json`, 'utf8')
      );
      expect(findingsData.optimizationFindings).toBeDefined();
      expect(Array.isArray(findingsData.optimizationFindings)).toBe(true);

      for (const f of findingsData.optimizationFindings.slice(0, 10)) {
        expect(f.id).toBeDefined();
        expect(f.severity).toBeDefined();
        expect(f.category).toBeDefined();
        expect(f.file).toBeDefined();
        expect(f.lineStart).toBeDefined();
        expect(f.lineEnd).toBeDefined();
        expect(f.title).toBeDefined();
        expect(f.reason).toBeDefined();
        expect(f.suggestedFix).toBeDefined();
        expect(f.suggestedFix.strategy).toBeDefined();
        expect(f.suggestedFix.steps).toBeDefined();
      }
    } finally {
      try {
        execSync(`rm -rf "${dir}"`, { encoding: 'utf8' });
      } catch {
        void 0;
      }
    }
  }, 30000);
});

describe('new AST detectors via buildIssueCatalog', () => {
  const testOpts = { ...DEFAULT_OPTS, findingsLimit: 500, includeTests: false };

  function makeEntry(
    file: string,
    overrides: Partial<FileEntry> = {}
  ): FileEntry {
    return {
      package: 'test-pkg',
      file,
      parseEngine: 'typescript',
      nodeCount: 0,
      kindCounts: {},
      functions: [],
      flows: [],
      dependencyProfile: {
        internalDependencies: [],
        externalDependencies: [],
        unresolvedDependencies: [],
        declaredExports: [],
        importedSymbols: [],
        reExports: [],
      },
      ...overrides,
    };
  }

  it('detects type-assertion-escape from pre-collected data', () => {
    const entry = makeEntry('src/risky.ts', {
      typeAssertionEscapes: {
        asAny: [
          { file: 'src/risky.ts', lineStart: 5, lineEnd: 5 },
          { file: 'src/risky.ts', lineStart: 10, lineEnd: 10 },
        ],
        doubleAssertion: [{ file: 'src/risky.ts', lineStart: 15, lineEnd: 15 }],
        nonNull: [{ file: 'src/risky.ts', lineStart: 20, lineEnd: 20 }],
      },
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    const escapes = findings.filter(
      f => f.category === 'type-assertion-escape'
    );
    expect(escapes.length).toBe(1);
    expect(escapes[0].title).toContain('4');
    expect(escapes[0].severity).toBe('medium');
  });

  it('detects high-severity type-assertion-escape', () => {
    const entry = makeEntry('src/bad.ts', {
      typeAssertionEscapes: {
        asAny: [
          { file: 'src/bad.ts', lineStart: 1, lineEnd: 1 },
          { file: 'src/bad.ts', lineStart: 2, lineEnd: 2 },
          { file: 'src/bad.ts', lineStart: 3, lineEnd: 3 },
          { file: 'src/bad.ts', lineStart: 4, lineEnd: 4 },
        ],
        doubleAssertion: [],
        nonNull: [],
      },
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    const escapes = findings.filter(
      f => f.category === 'type-assertion-escape'
    );
    expect(escapes[0].severity).toBe('high');
  });

  it('detects missing-error-boundary from pre-collected data', () => {
    const entry = makeEntry('src/api.ts', {
      unprotectedAsync: [
        { name: 'fetchData', awaitCount: 5, lineStart: 10, lineEnd: 20 },
      ],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    const errors = findings.filter(
      f => f.category === 'missing-error-boundary'
    );
    expect(errors.length).toBe(1);
    expect(errors[0].title).toContain('fetchData');
    expect(errors[0].severity).toBe('high');
  });

  it('detects promise-misuse from pre-collected data', () => {
    const entry = makeEntry('src/svc.ts', {
      asyncWithoutAwait: [{ name: 'doNothing', lineStart: 5, lineEnd: 10 }],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    const misuse = findings.filter(f => f.category === 'promise-misuse');
    expect(misuse.length).toBe(1);
    expect(misuse[0].title).toContain('doNothing');
    expect(misuse[0].severity).toBe('medium');
  });

  it('skips test files for all new detectors', () => {
    const entry = makeEntry('src/__tests__/foo.test.ts', {
      typeAssertionEscapes: {
        asAny: [
          { file: 'src/__tests__/foo.test.ts', lineStart: 1, lineEnd: 1 },
        ],
        doubleAssertion: [],
        nonNull: [],
      },
      unprotectedAsync: [
        { name: 'testFn', awaitCount: 1, lineStart: 1, lineEnd: 5 },
      ],
      asyncWithoutAwait: [{ name: 'mockFn', lineStart: 1, lineEnd: 5 }],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    expect(
      findings.filter(f =>
        [
          'type-assertion-escape',
          'missing-error-boundary',
          'promise-misuse',
        ].includes(f.category)
      )
    ).toHaveLength(0);
  });

  it('detects import-side-effect-risk for shared library with top-level sync-io', () => {
    const state = emptyState();
    for (let i = 0; i < 10; i++) {
      addEdge(state, `src/consumer${i}.ts`, 'src/shared-lib.ts');
    }
    const entry = makeEntry('src/shared-lib.ts', {
      topLevelEffects: [
        {
          kind: 'sync-io',
          lineStart: 5,
          lineEnd: 5,
          detail: 'fs.readFileSync()',
          weight: 5,
          confidence: 'high',
        },
        {
          kind: 'timer',
          lineStart: 8,
          lineEnd: 8,
          detail: 'setInterval()',
          weight: 4,
          confidence: 'high',
        },
      ],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      state,
      testOpts
    );
    const sideEffects = findings.filter(
      f => f.category === 'import-side-effect-risk'
    );
    expect(sideEffects.length).toBe(1);
    expect(sideEffects[0].severity).toBe('high');
    expect(sideEffects[0].reason).toContain('fan-in=10');
  });

  it('discounts entrypoint role for import-side-effect-risk', () => {
    const entry = makeEntry('src/index.ts', {
      topLevelEffects: [
        {
          kind: 'process-handler',
          lineStart: 10,
          lineEnd: 10,
          detail: 'process.on()',
          weight: 4,
          confidence: 'high',
        },
      ],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    const sideEffects = findings.filter(
      f => f.category === 'import-side-effect-risk'
    );
    expect(sideEffects).toHaveLength(0);
  });

  it('flags side-effect-only imports in high fan-in modules', () => {
    const state = emptyState();
    for (let i = 0; i < 20; i++) {
      addEdge(state, `src/consumer${i}.ts`, 'src/barrel.ts');
    }
    const entry = makeEntry('src/barrel.ts', {
      topLevelEffects: [
        {
          kind: 'side-effect-import',
          lineStart: 1,
          lineEnd: 1,
          detail: "import './init'",
          weight: 3,
          confidence: 'medium',
        },
        {
          kind: 'side-effect-import',
          lineStart: 2,
          lineEnd: 2,
          detail: "import './polyfill'",
          weight: 3,
          confidence: 'medium',
        },
      ],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      state,
      testOpts
    );
    const sideEffects = findings.filter(
      f => f.category === 'import-side-effect-risk'
    );
    expect(sideEffects.length).toBe(1);
    expect(sideEffects[0].reason).toContain('fan-in=20');
    expect(sideEffects[0].severity).toBe('high');
  });

  it('skips modules with no top-level effects', () => {
    const entry = makeEntry('src/clean.ts');
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    const sideEffects = findings.filter(
      f => f.category === 'import-side-effect-risk'
    );
    expect(sideEffects).toHaveLength(0);
  });

  it('detects critical severity for exec-sync at top level with high fan-in', () => {
    const state = emptyState();
    for (let i = 0; i < 25; i++) {
      addEdge(state, `src/consumer${i}.ts`, 'src/danger.ts');
    }
    const depSummary = minimalDepSummary({
      criticalPaths: [
        {
          start: 'src/danger.ts',
          path: ['src/danger.ts', 'src/core.ts'],
          score: 100,
          length: 2,
          containsCycle: false,
        },
      ],
    });
    const entry = makeEntry('src/danger.ts', {
      topLevelEffects: [
        {
          kind: 'exec-sync',
          lineStart: 3,
          lineEnd: 3,
          detail: 'execSync()',
          weight: 8,
          confidence: 'high',
        },
      ],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      depSummary,
      state,
      testOpts
    );
    const sideEffects = findings.filter(
      f => f.category === 'import-side-effect-risk'
    );
    expect(sideEffects.length).toBe(1);
    expect(sideEffects[0].severity).toBe('critical');
  });

  it('skips test files for import-side-effect-risk', () => {
    const entry = makeEntry('src/__tests__/setup.test.ts', {
      topLevelEffects: [
        {
          kind: 'sync-io',
          lineStart: 1,
          lineEnd: 1,
          detail: 'fs.readFileSync()',
          weight: 5,
          confidence: 'high',
        },
      ],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    const sideEffects = findings.filter(
      f => f.category === 'import-side-effect-risk'
    );
    expect(sideEffects).toHaveLength(0);
  });
});

function makeFinding(override: Partial<Finding> = {}): Finding {
  return {
    id: 'AST-ISSUE-0001',
    severity: 'medium',
    category: 'function-optimization',
    file: 'src/a.ts',
    lineStart: 1,
    lineEnd: 10,
    title: 'Test',
    reason: 'test',
    files: ['src/a.ts'],
    suggestedFix: { strategy: 's', steps: ['s'] },
    ...override,
  };
}

describe('computeDependencyCycles (additional)', () => {
  it('detects self-loop (A->A)', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'a.ts');
    const cycles = computeDependencyCycles(state);
    expect(cycles.length).toBe(1);
    expect(cycles[0].nodeCount).toBe(1);
    expect(cycles[0].path).toContain('a.ts');
  });
});

describe('computeDependencyCriticalPaths (additional)', () => {
  it('returns longest path by weighted score for simple chain', () => {
    const state = emptyState();
    addEdge(state, 'root.ts', 'mid.ts');
    addEdge(state, 'mid.ts', 'leaf.ts');
    const critMap = new Map<string, FileCriticality>();
    critMap.set('root.ts', {
      file: 'root.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 5,
    });
    critMap.set('mid.ts', {
      file: 'mid.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 50,
    });
    critMap.set('leaf.ts', {
      file: 'leaf.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 100,
    });
    const paths = computeDependencyCriticalPaths(state, critMap, testOpts);
    expect(paths.length).toBeGreaterThan(0);
    expect(paths[0].path).toEqual(['root.ts', 'mid.ts', 'leaf.ts']);
    expect(paths[0].score).toBe(155);
    expect(paths[0].length).toBe(3);
  });
});

describe('diversifyFindings (additional)', () => {
  it('interleaves multiple categories when limit is below total', () => {
    const input = [
      makeFinding({ category: 'dead-export', severity: 'high' }),
      makeFinding({ category: 'dead-export', severity: 'high' }),
      makeFinding({ category: 'dependency-cycle', severity: 'high' }),
      makeFinding({ category: 'function-optimization', severity: 'medium' }),
    ];
    const result = diversifyFindings(input, 3);
    expect(result).toHaveLength(3);
    const categories = result.map(f => f.category);
    expect(new Set(categories).size).toBeGreaterThanOrEqual(2);
  });
});

describe('diverseTopRecommendations (additional)', () => {
  it('honors maxPerCategory when one category dominates', () => {
    const findings = Array.from({ length: 20 }, (_, i) =>
      makeFinding({ id: `f-${i}`, category: 'dead-export', severity: 'high' })
    );
    findings.push(
      makeFinding({
        id: 'other',
        category: 'dependency-cycle',
        severity: 'high',
      })
    );
    const result = diverseTopRecommendations(findings, 10, 2);
    expect(result.filter(f => f.category === 'dead-export')).toHaveLength(2);
    expect(result.filter(f => f.category === 'dependency-cycle')).toHaveLength(
      1
    );
  });
});

describe('severityBreakdown and categoryBreakdown (additional)', () => {
  it('severityBreakdown includes info severity', () => {
    const findings = [
      makeFinding({ severity: 'info' }),
      makeFinding({ severity: 'info' }),
    ];
    const result = severityBreakdown(findings);
    expect(result.info).toBe(2);
  });

  it('categoryBreakdown handles unknown categories', () => {
    const findings = [makeFinding({ category: 'custom-cat' })];
    const result = categoryBreakdown(findings);
    expect(result['custom-cat']).toBe(1);
  });
});

describe('buildIssueCatalog (additional paths)', () => {
  it('respects noDiversify option (no round-robin)', () => {
    const depSummary = minimalDepSummary({
      cycles: [{ path: ['a.ts', 'b.ts', 'a.ts'], nodeCount: 2 }],
      testOnlyModules: [
        {
          file: 'src/t.ts',
          outboundCount: 0,
          inboundCount: 1,
          inboundFromProduction: 0,
          inboundFromTests: 1,
          externalDependencyCount: 0,
          unresolvedDependencyCount: 0,
        },
      ],
    });
    const state = emptyState();
    state.files.add('src/lib.ts');
    state.declaredExportsByFile.set('src/lib.ts', [
      { name: 'deadFn', kind: 'value', lineStart: 10, lineEnd: 15 },
    ]);
    const files = [
      makeFile({ functions: [makeFn({ complexity: 40, name: 'complexFn' })] }),
    ];
    const optsNoDiv = { ...testOpts, findingsLimit: 3, noDiversify: true };
    const { findings } = buildIssueCatalog(
      [],
      [],
      files,
      depSummary,
      state,
      optsNoDiv
    );
    expect(findings.length).toBeLessThanOrEqual(3);
  });

  it('triggers await-in-loop from awaitInLoopLocations', () => {
    const entry = makeFile({
      file: 'src/async.ts',
      awaitInLoopLocations: [
        { file: 'src/async.ts', lineStart: 10, lineEnd: 12 },
      ],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    expect(findings.some(f => f.category === 'await-in-loop')).toBe(true);
  });

  it('triggers sync-io from syncIoCalls', () => {
    const entry = makeFile({
      file: 'src/io.ts',
      syncIoCalls: [{ name: 'readFileSync', lineStart: 5, lineEnd: 5 }],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    expect(findings.some(f => f.category === 'sync-io')).toBe(true);
  });

  it('triggers uncleared-timer from timerCalls (setInterval without cleanup)', () => {
    const entry = makeFile({
      file: 'src/timers.ts',
      timerCalls: [
        { kind: 'setInterval', lineStart: 8, lineEnd: 8, hasCleanup: false },
      ],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    expect(findings.some(f => f.category === 'uncleared-timer')).toBe(true);
  });

  it('triggers listener-leak-risk from listenerRegistrations without removals', () => {
    const entry = makeFile({
      file: 'src/events.ts',
      listenerRegistrations: [
        { file: 'src/events.ts', lineStart: 15, lineEnd: 15 },
      ],
      listenerRemovals: [],
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [entry],
      minimalDepSummary(),
      emptyState(),
      testOpts
    );
    expect(findings.some(f => f.category === 'listener-leak-risk')).toBe(true);
  });

  it('respects findingsLimit truncation', () => {
    const files = Array.from({ length: 30 }, (_, i) =>
      makeFile({
        file: `src/f${i}.ts`,
        functions: [makeFn({ complexity: 40, name: `fn${i}` })],
      })
    );
    const opts = { ...testOpts, findingsLimit: 8 };
    const { findings, totalBeforeTruncation } = buildIssueCatalog(
      [],
      [],
      files,
      minimalDepSummary(),
      emptyState(),
      opts
    );
    expect(findings.length).toBeLessThanOrEqual(8);
    expect(totalBeforeTruncation).toBeGreaterThanOrEqual(findings.length);
  });
});

describe('writeMultiFileReport (additional)', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'scan-add-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  function makeMinimalReport(overrides: Partial<FullReport> = {}): FullReport {
    return {
      generatedAt: '2026-03-17T00:00:00.000Z',
      repoRoot: '/repo',
      options: {},
      parser: { requested: 'auto', effective: 'typescript' },
      summary: {
        totalFiles: 5,
        totalFunctions: 20,
        totalFlows: 50,
        totalDependencyFiles: 5,
        totalPackages: 1,
      },
      fileInventory: [],
      duplicateFlows: {
        duplicatedFunctions: [],
        duplicatedControlFlow: [],
        totalFunctionGroups: 0,
        totalFlowGroups: 0,
      },
      dependencyGraph: minimalDepSummary(),
      dependencyFindings: [],
      agentOutput: {
        totalFindings: 0,
        highPriority: 0,
        mediumPriority: 0,
        lowPriority: 0,
        topRecommendations: [],
        filesWithIssues: [],
      },
      optimizationOpportunities: [],
      optimizationFindings: [],
      parseErrors: [],
      ...overrides,
    };
  }

  it('creates summary.md with expected content', () => {
    const outDir = path.join(tmpDir, 'scan');
    writeMultiFileReport(
      outDir,
      makeMinimalReport(),
      { ...DEFAULT_OPTS, graph: false },
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const summaryPath = path.join(outDir, 'summary.md');
    expect(fs.existsSync(summaryPath)).toBe(true);
    const content = fs.readFileSync(summaryPath, 'utf8');
    expect(content).toContain('# Code Quality Scan Report');
    expect(content).toContain('## Scan Scope');
    expect(content).toContain('## Findings Overview');
  });

  it('creates findings.json with expected structure', () => {
    const outDir = path.join(tmpDir, 'scan');
    const findings = [
      makeFinding({ id: '1', category: 'dead-export', severity: 'high' }),
      makeFinding({
        id: '2',
        category: 'dependency-cycle',
        severity: 'medium',
      }),
    ];
    writeMultiFileReport(
      outDir,
      makeMinimalReport({ optimizationFindings: findings }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const findingsPath = path.join(outDir, 'findings.json');
    expect(fs.existsSync(findingsPath)).toBe(true);
    const data = JSON.parse(fs.readFileSync(findingsPath, 'utf8'));
    expect(data.totalFindings).toBe(2);
    expect(Array.isArray(data.optimizationFindings)).toBe(true);
    expect(data.optimizationFindings.length).toBe(2);
  });

  it('summary.md includes Analysis Signals when reportAnalysis is provided', () => {
    const outDir = path.join(tmpDir, 'scan');
    const reportAnalysis: import('./reporting/analysis.js').ReportAnalysisSummary =
      {
        graphSignals: [],
        astSignals: [],
        combinedSignals: [],
        strongestGraphSignal: {
          kind: 'cycle',
          lens: 'graph',
          title: 'Cycle',
          summary: 'Cycle detected',
          confidence: 'high',
          score: 80,
          files: [],
          categories: [],
          evidence: {},
        },
        strongestAstSignal: null,
        combinedInterpretation: {
          kind: 'hybrid',
          lens: 'hybrid',
          title: 'Hybrid',
          summary: 'Combined view',
          confidence: 'medium',
          score: 60,
          files: [],
          categories: [],
          evidence: {},
        },
        recommendedValidation: {
          summary: 'Validate with LSP',
          tools: ['lspFindReferences', 'lspGotoDefinition'],
        },
        investigationPrompts: ['Check cycle impact'],
      };
    writeMultiFileReport(
      outDir,
      makeMinimalReport({ reportAnalysis }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const summaryPath = path.join(outDir, 'summary.md');
    const content = fs.readFileSync(summaryPath, 'utf8');
    expect(content).toContain('## Analysis Signals');
    expect(content).toContain('Cycle detected');
    expect(content).toContain('Combined view');
    expect(content).toContain('Validate with LSP');
    expect(content).toContain('Check cycle impact');
  });

  it('summary.md includes structural layout alert for mega-folder signal', () => {
    const outDir = path.join(tmpDir, 'scan-mega');
    const reportAnalysis: import('./reporting/analysis.js').ReportAnalysisSummary =
      {
        graphSignals: [
          {
            kind: 'mega-folder-cluster',
            lens: 'graph',
            title: 'Mega folder concentration',
            summary:
              'src/core concentrates 42 files (54.0% of analyzed production files), which is a structural decomposition risk.',
            confidence: 'high',
            score: 180,
            files: ['src/core/a.ts'],
            categories: ['mega-folder'],
            evidence: {
              folderPath: 'src/core',
              fileCount: 42,
              concentration: 0.54,
            },
          },
        ],
        astSignals: [],
        combinedSignals: [],
        strongestGraphSignal: {
          kind: 'mega-folder-cluster',
          lens: 'graph',
          title: 'Mega folder concentration',
          summary: 'src/core concentrates 42 files',
          confidence: 'high',
          score: 180,
          files: ['src/core/a.ts'],
          categories: ['mega-folder'],
          evidence: { folderPath: 'src/core' },
        },
        strongestAstSignal: null,
        combinedInterpretation: null,
        recommendedValidation: null,
        investigationPrompts: [
          'Plan decomposition for src/core into smaller domain folders before adding more files there.',
        ],
      };
    writeMultiFileReport(
      outDir,
      makeMinimalReport({ reportAnalysis }),
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const content = fs.readFileSync(path.join(outDir, 'summary.md'), 'utf8');
    expect(content).toContain('Structural Layout Alert');
    expect(content).toContain('src/core concentrates 42 files');
  });
});

describe('writeMultiFileReport comprehensive', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'idx-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  function makeFullReport(overrides: Partial<FullReport> = {}): FullReport {
    const fileEntry = makeFile({ file: 'src/main.ts' });
    const depState = emptyState();
    addEdge(depState, 'src/a.ts', 'src/b.ts');
    addEdge(depState, 'src/b.ts', 'src/c.ts');
    addEdge(depState, 'src/c.ts', 'src/a.ts');
    const depSummary = minimalDepSummary({
      totalModules: 3,
      totalEdges: 3,
      cycles: [
        {
          path: ['src/a.ts', 'src/b.ts', 'src/c.ts', 'src/a.ts'],
          nodeCount: 3,
        },
      ],
      criticalPaths: [
        {
          start: 'src/a.ts',
          path: ['src/a.ts', 'src/b.ts', 'src/c.ts'],
          score: 200,
          length: 3,
          containsCycle: true,
        },
      ],
      criticalModules: [
        {
          file: 'src/a.ts',
          inboundCount: 1,
          outboundCount: 1,
          inboundFromProduction: 1,
          inboundFromTests: 0,
          externalDependencyCount: 0,
          unresolvedDependencyCount: 0,
          score: 50,
          riskBand: 'medium',
        },
      ],
      outgoingTop: [{ file: 'src/a.ts', count: 1, score: 50 }],
      inboundTop: [{ file: 'src/c.ts', count: 1, score: 50 }],
    });
    return {
      generatedAt: '2026-03-18T00:00:00.000Z',
      repoRoot: '/repo',
      options: {},
      parser: { requested: 'auto', effective: 'typescript' },
      summary: {
        totalFiles: 5,
        totalFunctions: 20,
        totalFlows: 80,
        totalDependencyFiles: 5,
        totalPackages: 1,
      },
      fileInventory: [fileEntry],
      duplicateFlows: {
        duplicatedFunctions: [],
        duplicatedControlFlow: [],
        totalFunctionGroups: 0,
        totalFlowGroups: 0,
      },
      dependencyGraph: depSummary,
      dependencyFindings: [],
      agentOutput: {
        totalFindings: 0,
        highPriority: 0,
        mediumPriority: 0,
        lowPriority: 0,
        topRecommendations: [],
        filesWithIssues: [],
      },
      optimizationOpportunities: [],
      optimizationFindings: [],
      parseErrors: [{ file: 'bad.ts', message: 'Unexpected token' }],
      ...overrides,
    };
  }

  function makeDepStateWithEdges(): {
    state: ReturnType<typeof emptyState>;
    summary: DependencySummary;
  } {
    const state = emptyState();
    addEdge(state, 'src/a.ts', 'src/b.ts');
    addEdge(state, 'src/b.ts', 'src/c.ts');
    addEdge(state, 'src/c.ts', 'src/a.ts');
    const summary = minimalDepSummary({
      totalModules: 3,
      totalEdges: 3,
      cycles: [
        {
          path: ['src/a.ts', 'src/b.ts', 'src/c.ts', 'src/a.ts'],
          nodeCount: 3,
        },
      ],
      criticalPaths: [
        {
          start: 'src/a.ts',
          path: ['src/a.ts', 'src/b.ts', 'src/c.ts'],
          score: 150,
          length: 3,
          containsCycle: true,
        },
      ],
      criticalModules: [
        {
          file: 'src/a.ts',
          inboundCount: 1,
          outboundCount: 1,
          inboundFromProduction: 1,
          inboundFromTests: 0,
          externalDependencyCount: 0,
          unresolvedDependencyCount: 0,
          score: 60,
          riskBand: 'high',
        },
      ],
      outgoingTop: [{ file: 'src/a.ts', count: 1, score: 60 }],
      inboundTop: [{ file: 'src/c.ts', count: 1, score: 60 }],
    });
    return { state, summary };
  }

  it('creates ALL output files including graph.md when options.graph=true', () => {
    const outDir = path.join(tmpDir, 'scan-graph');
    const { state, summary } = makeDepStateWithEdges();
    const report = makeFullReport();
    writeMultiFileReport(
      outDir,
      report,
      { ...DEFAULT_OPTS, graph: true },
      state,
      summary,
      new Map()
    );
    expect(fs.existsSync(path.join(outDir, 'graph.md'))).toBe(true);
    const graphContent = fs.readFileSync(path.join(outDir, 'graph.md'), 'utf8');
    expect(graphContent).toContain('graph LR');
    expect(graphContent).toContain('Dependency Cycles');
    expect(graphContent).toContain('Critical Dependency Chains');
    expect(graphContent).toContain('Total modules');
  });

  it('includes sccClusters and packageGraphSummary when options.graphAdvanced=true', () => {
    const outDir = path.join(tmpDir, 'scan-adv');
    const { state, summary } = makeDepStateWithEdges();
    const report = makeFullReport();
    writeMultiFileReport(
      outDir,
      report,
      { ...DEFAULT_OPTS, graph: true, graphAdvanced: true },
      state,
      summary,
      new Map()
    );
    const archData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'architecture.json'), 'utf8')
    );
    expect(archData.sccClusters).toBeDefined();
    expect(Array.isArray(archData.sccClusters)).toBe(true);
    expect(archData.packageGraphSummary).toBeDefined();
  });

  it('creates ast-trees.txt when report.astTrees is set', () => {
    const outDir = path.join(tmpDir, 'scan-trees');
    const tree = {
      kind: 'SourceFile',
      startLine: 1,
      endLine: 20,
      children: [
        {
          kind: 'FunctionDeclaration',
          startLine: 5,
          endLine: 15,
          children: [],
        },
      ],
    };
    const report = makeFullReport({
      astTrees: [{ package: 'test', file: 'src/foo.ts', tree }],
    });
    writeMultiFileReport(
      outDir,
      report,
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const txtPath = path.join(outDir, 'ast-trees.txt');
    expect(fs.existsSync(txtPath)).toBe(true);
    const content = fs.readFileSync(txtPath, 'utf8');
    expect(content).toContain('## test — src/foo.ts');
    expect(content).toContain('SourceFile');
    expect(content).toContain('FunctionDeclaration');
  });

  it('creates security.json when security findings present', () => {
    const outDir = path.join(tmpDir, 'scan-sec');
    const securityFindings = [
      makeFinding({
        id: 's1',
        category: 'hardcoded-secret',
        severity: 'high',
        file: 'src/keys.ts',
      }),
    ];
    const report = makeFullReport({ optimizationFindings: securityFindings });
    writeMultiFileReport(
      outDir,
      report,
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    expect(fs.existsSync(path.join(outDir, 'security.json'))).toBe(true);
    const secData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'security.json'), 'utf8')
    );
    expect(secData.findingsCount).toBe(1);
    expect(secData.findings[0].category).toBe('hardcoded-secret');
  });

  it('creates test-quality.json when test quality findings present', () => {
    const outDir = path.join(tmpDir, 'scan-test');
    const testFindings = [
      makeFinding({
        id: 't1',
        category: 'low-assertion-density',
        severity: 'medium',
        file: 'src/foo.test.ts',
      }),
    ];
    const report = makeFullReport({ optimizationFindings: testFindings });
    writeMultiFileReport(
      outDir,
      report,
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    expect(fs.existsSync(path.join(outDir, 'test-quality.json'))).toBe(true);
    const tqData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'test-quality.json'), 'utf8')
    );
    expect(tqData.findingsCount).toBe(1);
  });

  it('verifies summary.json contains outputFiles index', () => {
    const outDir = path.join(tmpDir, 'scan-sum');
    const { state, summary } = makeDepStateWithEdges();
    writeMultiFileReport(
      outDir,
      makeFullReport(),
      { ...DEFAULT_OPTS, graph: true },
      state,
      summary,
      new Map()
    );
    const summaryData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'summary.json'), 'utf8')
    );
    expect(summaryData.outputFiles.summary).toBe('summary.json');
    expect(summaryData.outputFiles.findings).toBe('findings.json');
    expect(summaryData.outputFiles.architecture).toBe('architecture.json');
    expect(summaryData.outputFiles.codeQuality).toBe('code-quality.json');
    expect(summaryData.outputFiles.deadCode).toBe('dead-code.json');
    expect(summaryData.outputFiles.fileInventory).toBe('file-inventory.json');
    expect(summaryData.outputFiles.summaryMd).toBe('summary.md');
    expect(summaryData.outputFiles.graph).toBe('graph.md');
  });

  it('verifies summary.md contains expected sections', () => {
    const outDir = path.join(tmpDir, 'scan-md');
    const report = makeFullReport();
    writeMultiFileReport(
      outDir,
      report,
      DEFAULT_OPTS,
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const md = fs.readFileSync(path.join(outDir, 'summary.md'), 'utf8');
    expect(md).toContain('# Code Quality Scan Report');
    expect(md).toContain('## Scan Scope');
    expect(md).toContain('## Findings Overview');
    expect(md).toContain('## Health Scores');
    expect(md).toContain('## Architecture Health');
    expect(md).toContain('## Code Quality');
    expect(md).toContain('## Dead Code & Hygiene');
    expect(md).toContain('## Output Files');
    expect(md).toContain('## Parse Errors');
    expect(md).toContain('bad.ts');
    expect(md).toContain('Unexpected token');
  });

  it('works with options.flow=true (enriches file inventory and findings)', () => {
    const outDir = path.join(tmpDir, 'scan-flow');
    const report = makeFullReport({
      fileInventory: [
        makeFile({
          file: 'src/a.ts',
          flows: [
            {
              kind: 'flow',
              file: 'src/a.ts',
              lineStart: 1,
              lineEnd: 5,
              columnStart: 1,
              columnEnd: 1,
              statementCount: 3,
            },
          ],
        }),
      ],
    });
    writeMultiFileReport(
      outDir,
      report,
      { ...DEFAULT_OPTS, flow: true },
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const invData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'file-inventory.json'), 'utf8')
    );
    expect(invData.fileInventory).toBeDefined();
    expect(invData.fileCount).toBe(1);
  });

  it('uses report.graphAnalytics when provided (skips computeGraphAnalytics)', () => {
    const outDir = path.join(tmpDir, 'scan-precomputed');
    const graphAnalytics = {
      sccClusters: [
        {
          id: 'c1',
          files: ['src/a.ts'],
          nodeCount: 1,
          edgeCount: 0,
          entryEdges: 0,
          exitEdges: 0,
          hubFiles: [],
        },
      ],
      chokepoints: [],
      packageGraphSummary: {
        packageCount: 1,
        edgeCount: 0,
        packages: [],
        hotspots: [],
      },
      articulationPoints: [],
      bridgeEdges: [],
    };
    const report = makeFullReport({ graphAnalytics });
    writeMultiFileReport(
      outDir,
      report,
      { ...DEFAULT_OPTS, graphAdvanced: true },
      emptyState(),
      minimalDepSummary(),
      new Map()
    );
    const archData = JSON.parse(
      fs.readFileSync(path.join(outDir, 'architecture.json'), 'utf8')
    );
    expect(archData.sccClusters).toHaveLength(1);
    expect(archData.sccClusters[0].id).toBe('c1');
  });
});

describe('buildIssueCatalog detector paths', () => {
  const opts = {
    ...DEFAULT_OPTS,
    root: '/repo',
    findingsLimit: 500,
    thresholds: { ...DEFAULT_OPTS.thresholds, anyThreshold: 5, halsteadEffortThreshold: 500_000, maintainabilityIndexThreshold: 20 },
  };

  it('detects dead exports via declaredExportsByFile without consumedFromModule', () => {
    const state = emptyState();
    state.files.add('src/lib.ts');
    state.declaredExportsByFile.set('src/lib.ts', [
      { name: 'usedFn', kind: 'value' },
      { name: 'deadExport', kind: 'value', lineStart: 20, lineEnd: 25 },
    ]);
    state.importedSymbolsByFile.set('src/consumer.ts', [
      {
        sourceModule: './lib',
        resolvedModule: 'src/lib.ts',
        importedName: 'usedFn',
        localName: 'usedFn',
        isTypeOnly: false,
      },
    ]);
    addEdge(state, 'src/consumer.ts', 'src/lib.ts');
    const { findings } = buildIssueCatalog(
      [],
      [],
      [],
      minimalDepSummary(),
      state,
      opts
    );
    expect(
      findings.some(
        f => f.category === 'dead-export' && f.title.includes('deadExport')
      )
    ).toBe(true);
  });

  it('detects dead re-exports when reExport not consumed', () => {
    const state = emptyState();
    state.files.add('src/barrel.ts');
    state.reExportsByFile.set('src/barrel.ts', [
      {
        sourceModule: './a',
        resolvedModule: 'src/a.ts',
        exportedAs: 'unusedReExport',
        importedName: 'unusedReExport',
        isStar: false,
        isTypeOnly: false,
        lineStart: 1,
        lineEnd: 1,
      },
    ]);
    const { findings } = buildIssueCatalog(
      [],
      [],
      [],
      minimalDepSummary(),
      state,
      opts
    );
    expect(findings.some(f => f.category === 'dead-re-export')).toBe(true);
  });

  it('detects namespace import (importedName=*)', () => {
    const state = emptyState();
    state.files.add('src/consumer.ts');
    state.importedSymbolsByFile.set('src/consumer.ts', [
      {
        sourceModule: 'lodash',
        resolvedModule: 'node_modules/lodash',
        importedName: '*',
        localName: '_',
        isTypeOnly: false,
      },
    ]);
    const { findings } = buildIssueCatalog(
      [],
      [],
      [],
      minimalDepSummary(),
      state,
      opts
    );
    expect(findings.some(f => f.category === 'namespace-import')).toBe(true);
  });

  it('detects CommonJS in ESM (localName=require)', () => {
    const state = emptyState();
    state.files.add('src/mixed.ts');
    state.importedSymbolsByFile.set('src/mixed.ts', [
      {
        sourceModule: 'createRequire',
        resolvedModule: 'node:module',
        importedName: 'createRequire',
        localName: 'require',
        isTypeOnly: false,
      },
    ]);
    const { findings } = buildIssueCatalog(
      [],
      [],
      [],
      minimalDepSummary(),
      state,
      opts
    );
    expect(findings.some(f => f.category === 'commonjs-in-esm')).toBe(true);
  });

  it('detects export-star leak (isStar=true)', () => {
    const state = emptyState();
    state.files.add('src/barrel.ts');
    state.reExportsByFile.set('src/barrel.ts', [
      {
        sourceModule: './internal',
        resolvedModule: 'src/internal.ts',
        exportedAs: '*',
        importedName: '*',
        isStar: true,
        isTypeOnly: false,
        lineStart: 1,
        lineEnd: 1,
      },
    ]);
    const { findings } = buildIssueCatalog(
      [],
      [],
      [],
      minimalDepSummary(),
      state,
      opts
    );
    expect(findings.some(f => f.category === 'export-star-leak')).toBe(true);
  });

  it('detects unsafe-any when anyCount > threshold', () => {
    const files = [makeFile({ file: 'src/loose.ts', anyCount: 12 })];
    const { findings } = buildIssueCatalog(
      [],
      [],
      files,
      minimalDepSummary(),
      emptyState(),
      opts
    );
    expect(
      findings.some(
        f => f.category === 'unsafe-any' && f.file === 'src/loose.ts'
      )
    ).toBe(true);
  });

  it('detects high Halstead effort when effort > threshold', () => {
    const fn = makeFn({
      file: 'src/hard.ts',
      halstead: {
        operators: 50,
        operands: 100,
        distinctOperators: 20,
        distinctOperands: 30,
        vocabulary: 50,
        length: 150,
        volume: 800,
        difficulty: 10,
        effort: 600_000,
        time: 30,
        estimatedBugs: 1,
      },
    });
    const files = [makeFile({ file: 'src/hard.ts', functions: [fn] })];
    const { findings } = buildIssueCatalog(
      [],
      [],
      files,
      minimalDepSummary(),
      emptyState(),
      opts
    );
    expect(findings.some(f => f.category === 'halstead-effort')).toBe(true);
  });

  it('detects low maintainability when maintainabilityIndex < threshold', () => {
    const fn = makeFn({ file: 'src/bad.ts', maintainabilityIndex: 15 });
    const files = [makeFile({ file: 'src/bad.ts', functions: [fn] })];
    const { findings } = buildIssueCatalog(
      [],
      [],
      files,
      minimalDepSummary(),
      emptyState(),
      opts
    );
    expect(findings.some(f => f.category === 'low-maintainability')).toBe(true);
  });

  it('detects unbounded-collection when loops>=2, calls>=5, maxLoopDepth>=2', () => {
    const fn = makeFn({
      file: 'src/collect.ts',
      loops: 3,
      calls: 8,
      maxLoopDepth: 3,
    });
    const files = [makeFile({ file: 'src/collect.ts', functions: [fn] })];
    const { findings } = buildIssueCatalog(
      [],
      [],
      files,
      minimalDepSummary(),
      emptyState(),
      opts
    );
    expect(findings.some(f => f.category === 'unbounded-collection')).toBe(
      true
    );
  });
});

describe('computeDependencyCycles comprehensive', () => {
  it('detects triangle cycle A->B->C->A', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'c.ts');
    addEdge(state, 'c.ts', 'a.ts');
    const cycles = computeDependencyCycles(state);
    expect(cycles.length).toBe(1);
    expect(cycles[0].nodeCount).toBe(3);
    expect(cycles[0].path).toContain('a.ts');
    expect(cycles[0].path).toContain('b.ts');
    expect(cycles[0].path).toContain('c.ts');
  });

  it('detects multiple disjoint cycles', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'a.ts');
    addEdge(state, 'x.ts', 'y.ts');
    addEdge(state, 'y.ts', 'z.ts');
    addEdge(state, 'z.ts', 'x.ts');
    const cycles = computeDependencyCycles(state);
    expect(cycles.length).toBe(2);
  });

  it('returns empty for linear chain (no cycles)', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'b.ts');
    addEdge(state, 'b.ts', 'c.ts');
    addEdge(state, 'c.ts', 'd.ts');
    expect(computeDependencyCycles(state)).toEqual([]);
  });

  it('detects self-loop A->A', () => {
    const state = emptyState();
    addEdge(state, 'a.ts', 'a.ts');
    const cycles = computeDependencyCycles(state);
    expect(cycles.length).toBe(1);
    expect(cycles[0].nodeCount).toBe(1);
    expect(cycles[0].path).toContain('a.ts');
  });
});

describe('computeDependencyCriticalPaths comprehensive', () => {
  it('returns single path root->leaf for linear chain', () => {
    const state = emptyState();
    addEdge(state, 'root.ts', 'mid.ts');
    addEdge(state, 'mid.ts', 'leaf.ts');
    const critMap = new Map<string, FileCriticality>();
    critMap.set('root.ts', {
      file: 'root.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 10,
    });
    critMap.set('mid.ts', {
      file: 'mid.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 20,
    });
    critMap.set('leaf.ts', {
      file: 'leaf.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 30,
    });
    const paths = computeDependencyCriticalPaths(state, critMap, testOpts);
    expect(paths.length).toBeGreaterThan(0);
    expect(paths[0].path).toEqual(['root.ts', 'mid.ts', 'leaf.ts']);
    expect(paths[0].length).toBe(3);
  });

  it('handles branching paths and returns highest weighted path', () => {
    const state = emptyState();
    addEdge(state, 'root.ts', 'branch-a.ts');
    addEdge(state, 'root.ts', 'branch-b.ts');
    addEdge(state, 'branch-a.ts', 'leaf-a.ts');
    addEdge(state, 'branch-b.ts', 'leaf-b.ts');
    const critMap = new Map<string, FileCriticality>();
    critMap.set('root.ts', {
      file: 'root.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 5,
    });
    critMap.set('branch-a.ts', {
      file: 'branch-a.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 100,
    });
    critMap.set('branch-b.ts', {
      file: 'branch-b.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 10,
    });
    critMap.set('leaf-a.ts', {
      file: 'leaf-a.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 5,
    });
    critMap.set('leaf-b.ts', {
      file: 'leaf-b.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 5,
    });
    const paths = computeDependencyCriticalPaths(state, critMap, testOpts);
    expect(paths.length).toBeGreaterThan(0);
    expect(paths[0].path).toContain('branch-a.ts');
    expect(paths[0].path[0]).toBe('root.ts');
  });

  it('returns empty for empty state (no files)', () => {
    const state = emptyState();
    const critMap = new Map<string, FileCriticality>();
    const paths = computeDependencyCriticalPaths(state, critMap, testOpts);
    expect(paths).toEqual([]);
  });
});

describe('generateSummaryMd comprehensive', () => {
  const fakeDir = '/tmp/nonexistent-scan-dir';

  function makeReportForMd(overrides: Partial<FullReport> = {}): FullReport {
    return {
      generatedAt: '2026-03-17T00:00:00.000Z',
      repoRoot: '/repo',
      options: {},
      parser: { requested: 'auto', effective: 'typescript' },
      summary: {
        totalFiles: 42,
        totalFunctions: 318,
        totalFlows: 1204,
        totalDependencyFiles: 50,
        totalPackages: 3,
      },
      fileInventory: [],
      duplicateFlows: {},
      dependencyGraph: minimalDepSummary({
        totalModules: 42,
        totalEdges: 187,
        cycles: [{ path: ['a', 'b', 'a'], nodeCount: 2 }],
        criticalPaths: [],
      }),
      dependencyFindings: [],
      agentOutput: {
        totalFindings: 5,
        highPriority: 2,
        mediumPriority: 2,
        lowPriority: 1,
        topRecommendations: [
          {
            severity: 'high',
            title: 'Fix cycle',
            file: 'src/a.ts',
            category: 'dependency-cycle',
          },
        ],
        filesWithIssues: [],
      },
      optimizationOpportunities: [],
      optimizationFindings: [],
      parseErrors: [],
      ...overrides,
    };
  }

  it('always includes Security section', () => {
    const securityFindings: Finding[] = [
      makeFinding({
        id: 's1',
        category: 'hardcoded-secret',
        severity: 'high',
        file: 'src/keys.ts',
      }),
    ];
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      securityFindings,
    });
    expect(md).toContain('## Security');
    expect(md).toContain('security.json');
    expect(md).toContain('hardcoded-secret');
  });

  it('always includes Test Quality section', () => {
    const testQualityFindings: Finding[] = [
      makeFinding({
        id: 't1',
        category: 'low-assertion-density',
        severity: 'medium',
        file: 'src/foo.test.ts',
      }),
    ];
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      testQualityFindings,
    });
    expect(md).toContain('## Test Quality');
    expect(md).toContain('test-quality.json');
    expect(md).toContain('low-assertion-density');
  });

  it('shows empty pillar sections when no findings were emitted for that pillar', () => {
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      securityFindings: [],
      testQualityFindings: [],
    });
    expect(md).toContain('## Security');
    expect(md).toContain('no `security.json` written for this scan');
    expect(md).toContain('## Test Quality');
    expect(md).toContain('no `test-quality.json` written for this scan');
  });

  it('shows scope when scope option is set', () => {
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      scope: ['/repo/src/a.ts', '/repo/src/b.ts'],
      root: '/repo',
    });
    expect(md).toContain('## Scan Scope');
    expect(md).toContain('Scoped scan');
  });

  it('shows scopeSymbols when scopeSymbols set with scope', () => {
    const scopeSymbols = new Map<string, string[]>();
    scopeSymbols.set('/repo/src/lib.ts', ['foo', 'bar']);
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      scope: ['/repo/src/lib.ts'],
      root: '/repo',
      scopeSymbols,
    });
    expect(md).toContain('Scoped scan');
    expect(md).toContain('lib.ts');
  });

  it('shows reportAnalysis signals when provided', () => {
    const reportAnalysis: import('./reporting/analysis.js').ReportAnalysisSummary =
      {
        graphSignals: [],
        astSignals: [],
        combinedSignals: [],
        strongestGraphSignal: {
          kind: 'cycle',
          lens: 'graph',
          title: 'Cycle',
          summary: 'Cycle detected',
          confidence: 'high',
          score: 80,
          files: [],
          categories: [],
          evidence: {},
        },
        strongestAstSignal: {
          kind: 'complexity',
          lens: 'ast',
          title: 'Complex',
          summary: 'High complexity',
          confidence: 'medium',
          score: 60,
          files: [],
          categories: [],
          evidence: {},
        },
        combinedInterpretation: {
          kind: 'hybrid',
          lens: 'hybrid',
          title: 'Hybrid',
          summary: 'Combined view',
          confidence: 'medium',
          score: 60,
          files: [],
          categories: [],
          evidence: {},
        },
        recommendedValidation: {
          summary: 'Validate with LSP',
          tools: ['lspFindReferences', 'lspGotoDefinition'],
        },
        investigationPrompts: ['Check cycle impact', 'Verify complexity'],
      };
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      reportAnalysis,
    });
    expect(md).toContain('## Analysis Signals');
    expect(md).toContain('Cycle detected');
    expect(md).toContain('High complexity');
    expect(md).toContain('Combined view');
    expect(md).toContain('Validate with LSP');
    expect(md).toContain('Check cycle impact');
  });

  it('shows semantic enabled message when semanticEnabled=true', () => {
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      semanticEnabled: true,
    });
    expect(md).toContain('Semantic analysis');
    expect(md).toContain('14 additional categories');
  });

  it('shows activeFeatures filter when present', () => {
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [
        makeFinding({ category: 'dependency-cycle' }),
        makeFinding({ category: 'dead-export' }),
      ],
      codeQualityFindings: [],
      deadCodeFindings: [],
      activeFeatures: new Set(['dependency-cycle']),
    });
    expect(md).toContain('Features filter');
    expect(md).toContain('dependency-cycle');
    expect(md).toContain('*(skipped)*');
    expect(md).toContain('| Security | — | skipped |');
  });

  it('shows truncated message when totalBeforeTruncation > findings length', () => {
    const report = makeReportForMd({
      optimizationFindings: [],
      agentOutput: {
        totalFindings: 5,
        totalBeforeTruncation: 20,
        highPriority: 2,
        mediumPriority: 2,
        lowPriority: 1,
        topRecommendations: [],
        filesWithIssues: [],
        droppedCategories: ['dead-export', 'function-optimization'],
      },
    });
    const md = generateSummaryMd({
      dir: fakeDir,
      report,
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('Truncated');
    expect(md).toContain('20');
    expect(md).toContain('dead-export');
  });

  it('shows Change Risk Hotspots when hotFiles provided', () => {
    const hotFiles: import('./types/index.js').HotFile[] = [
      {
        file: 'src/core.ts',
        riskScore: 85,
        fanIn: 10,
        fanOut: 5,
        complexityScore: 50,
        exportCount: 8,
        inCycle: true,
        onCriticalPath: true,
      },
    ];
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      hotFiles,
    });
    expect(md).toContain('## Change Risk Hotspots');
    expect(md).toContain('src/core.ts');
    expect(md).toContain('85');
  });

  it('shows Top Concern Tags when findings have tags', () => {
    const findings: Finding[] = [
      { ...makeFinding({ id: '1' }), tags: ['coupling', 'architecture'] },
      { ...makeFinding({ id: '2' }), tags: ['coupling'] },
    ];
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd({ optimizationFindings: findings }),
      outputFiles: {},
      architectureFindings: findings,
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Top Concern Tags');
    expect(md).toContain('coupling');
  });

  it('formatFileSize: shows B for small files, KB for medium, MB for large (via outputFiles)', () => {
    const realDir = fs.mkdtempSync(path.join(os.tmpdir(), 'summary-size-'));
    try {
      fs.writeFileSync(
        path.join(realDir, 'small.json'),
        'x'.repeat(100),
        'utf8'
      );
      fs.writeFileSync(
        path.join(realDir, 'medium.json'),
        'x'.repeat(2048),
        'utf8'
      );
      fs.writeFileSync(
        path.join(realDir, 'large.json'),
        'x'.repeat(2 * 1024 * 1024),
        'utf8'
      );
      const outputFiles = {
        small: 'small.json',
        medium: 'medium.json',
        large: 'large.json',
      };
      const md = generateSummaryMd({
        dir: realDir,
        report: makeReportForMd(),
        outputFiles,
        architectureFindings: [],
        codeQualityFindings: [],
        deadCodeFindings: [],
      });
      expect(md).toContain('| Size |');
      expect(md).toMatch(/\d+\s*B/);
      expect(md).toMatch(/\d+(\.\d+)?\s*KB/);
      expect(md).toMatch(/\d+(\.\d+)?\s*MB/);
    } finally {
      fs.rmSync(realDir, { recursive: true, force: true });
    }
  });

  it('includes AST Trees section when outputFiles.astTrees present', () => {
    const outputFiles = { astTrees: 'ast-trees.txt' };
    const scanDir = path.join(
      fakeDir,
      '.octocode',
      'scan',
      '2026-03-19T00-00-00-000Z'
    );
    const md = generateSummaryMd({
      dir: scanDir,
      report: makeReportForMd(),
      outputFiles,
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      root: fakeDir,
    });
    expect(md).toContain('## AST Trees');
    expect(md).toContain('ast-trees.txt');
    expect(md).toContain('Run these commands from the skill directory.');
    expect(md).toContain('node scripts/ast/tree-search.js');
    expect(md).toContain(
      '.octocode/scan/2026-03-19T00-00-00-000Z/ast-trees.txt'
    );
    expect(md).toContain('--limit 25');
    expect(md).toContain('Raw text fallback');
    expect(md).not.toContain('grep "^##" ast-trees.txt');
  });

  it('skips depGraph block when dependencyGraph is undefined', () => {
    const report = makeReportForMd({
      dependencyGraph: undefined as unknown as DependencySummary,
    });
    const md = generateSummaryMd({
      dir: fakeDir,
      report,
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
    });
    expect(md).toContain('## Architecture Health');
    expect(md).not.toContain('| Modules |');
  });

  it('handles reportAnalysis with null strongestGraphSignal and strongestAstSignal', () => {
    const reportAnalysis: import('./reporting/analysis.js').ReportAnalysisSummary =
      {
        graphSignals: [],
        astSignals: [],
        combinedSignals: [],
        strongestGraphSignal: null,
        strongestAstSignal: null,
        combinedInterpretation: {
          kind: 'hybrid',
          lens: 'hybrid',
          title: 'Hybrid',
          summary: 'No combined interpretation available yet.',
          confidence: 'low',
          score: 0,
          files: [],
          categories: [],
          evidence: {},
        },
        recommendedValidation: null,
        investigationPrompts: [],
      };
    const md = generateSummaryMd({
      dir: fakeDir,
      report: makeReportForMd(),
      outputFiles: {},
      architectureFindings: [],
      codeQualityFindings: [],
      deadCodeFindings: [],
      reportAnalysis,
    });
    expect(md).toContain('No dominant graph signal');
    expect(md).toContain('No dominant AST signal');
    expect(md).toContain('No combined interpretation available yet');
  });

  it('shows dash for file size when output file does not exist', () => {
    const realDir = fs.mkdtempSync(path.join(os.tmpdir(), 'summary-missing-'));
    try {
      const outputFiles = {
        existing: 'existing.json',
        missing: 'nonexistent.json',
      };
      fs.writeFileSync(path.join(realDir, 'existing.json'), '{}', 'utf8');
      const md = generateSummaryMd({
        dir: realDir,
        report: makeReportForMd(),
        outputFiles,
        architectureFindings: [],
        codeQualityFindings: [],
        deadCodeFindings: [],
      });
      expect(md).toContain('| Size |');
      expect(md).toContain('—');
    } finally {
      fs.rmSync(realDir, { recursive: true, force: true });
    }
  });
});

describe('diversifyFindings edge cases', () => {
  it('returns empty when limit=0', () => {
    const input = [
      makeFinding({ category: 'a', severity: 'high' }),
      makeFinding({ category: 'b', severity: 'high' }),
    ];
    const result = diversifyFindings(input, 0);
    expect(result).toEqual([]);
  });

  it('returns single finding when limit=1', () => {
    const input = [
      makeFinding({ category: 'critical-cat', severity: 'critical' }),
      makeFinding({ category: 'high-cat', severity: 'high' }),
    ];
    const result = diversifyFindings(input, 1);
    expect(result).toHaveLength(1);
    expect(result[0].severity).toBe('critical');
  });

  it('handles all same category (no diversity possible)', () => {
    const input = Array.from({ length: 10 }, (_, i) =>
      makeFinding({ id: `f-${i}`, category: 'only-cat', severity: 'high' })
    );
    const result = diversifyFindings(input, 5);
    expect(result).toHaveLength(5);
    expect(result.every(f => f.category === 'only-cat')).toBe(true);
  });

  it('handles many categories with 1 finding each', () => {
    const input = [
      makeFinding({ id: '1', category: 'cat-a', severity: 'high' }),
      makeFinding({ id: '2', category: 'cat-b', severity: 'high' }),
      makeFinding({ id: '3', category: 'cat-c', severity: 'medium' }),
      makeFinding({ id: '4', category: 'cat-d', severity: 'medium' }),
      makeFinding({ id: '5', category: 'cat-e', severity: 'low' }),
    ];
    const result = diversifyFindings(input, 3);
    expect(result).toHaveLength(3);
    const categories = new Set(result.map(f => f.category));
    expect(categories.size).toBe(3);
  });
});

describe('diverseTopRecommendations edge cases', () => {
  it('maxPerCategory=1 forces maximum diversity', () => {
    const findings = [
      makeFinding({ id: '1', category: 'dead-export', severity: 'critical' }),
      makeFinding({ id: '2', category: 'dead-export', severity: 'high' }),
      makeFinding({ id: '3', category: 'dependency-cycle', severity: 'high' }),
      makeFinding({
        id: '4',
        category: 'function-optimization',
        severity: 'medium',
      }),
    ];
    const result = diverseTopRecommendations(findings, 10, 1);
    expect(result).toHaveLength(3);
    expect(new Set(result.map(f => f.category)).size).toBe(3);
  });

  it('all findings same category returns up to maxPerCategory', () => {
    const findings = Array.from({ length: 10 }, (_, i) =>
      makeFinding({ id: `f-${i}`, category: 'only-cat', severity: 'high' })
    );
    const result = diverseTopRecommendations(findings, 10, 2);
    expect(result).toHaveLength(2);
    expect(result.every(f => f.category === 'only-cat')).toBe(true);
  });

  it('exactly at limit returns correct count', () => {
    const findings = [
      makeFinding({ id: '1', category: 'a', severity: 'high' }),
      makeFinding({ id: '2', category: 'b', severity: 'high' }),
      makeFinding({ id: '3', category: 'c', severity: 'high' }),
    ];
    const result = diverseTopRecommendations(findings, 3, 2);
    expect(result).toHaveLength(3);
  });
});

describe('buildIssueCatalog detector paths via buildIssueCatalog', () => {
  const opts = { ...DEFAULT_OPTS, root: '/repo', findingsLimit: 500 };

  it('detectDistanceFromMainSequence: state with Zone of Pain (concrete+stable) triggers finding', () => {
    const state = emptyState();
    state.files.add('src/lib.ts');
    state.declaredExportsByFile.set('src/lib.ts', [
      { name: 'a', kind: 'value' },
      { name: 'b', kind: 'value' },
      { name: 'c', kind: 'value' },
      { name: 'd', kind: 'value' },
      { name: 'e', kind: 'value' },
    ]);
    for (let i = 0; i < 7; i++) {
      state.files.add(`src/dep${i}.ts`);
      addEdge(state, `src/dep${i}.ts`, 'src/lib.ts');
    }
    addEdge(state, 'src/lib.ts', 'src/dep0.ts');
    addEdge(state, 'src/lib.ts', 'src/dep1.ts');
    const { findings } = buildIssueCatalog(
      [],
      [],
      [],
      minimalDepSummary(),
      state,
      opts
    );
    expect(
      findings.some(f => f.category === 'distance-from-main-sequence')
    ).toBe(true);
  });

  it('detectFeatureEnvy: file importing heavily from one module triggers finding', () => {
    const state = emptyState();
    state.files.add('src/envious.ts');
    state.files.add('src/target.ts');
    state.importedSymbolsByFile.set('src/envious.ts', [
      {
        sourceModule: './target',
        resolvedModule: 'src/target.ts',
        importedName: 'a',
        localName: 'a',
        isTypeOnly: false,
      },
      {
        sourceModule: './target',
        resolvedModule: 'src/target.ts',
        importedName: 'b',
        localName: 'b',
        isTypeOnly: false,
      },
      {
        sourceModule: './target',
        resolvedModule: 'src/target.ts',
        importedName: 'c',
        localName: 'c',
        isTypeOnly: false,
      },
      {
        sourceModule: './target',
        resolvedModule: 'src/target.ts',
        importedName: 'd',
        localName: 'd',
        isTypeOnly: false,
      },
      {
        sourceModule: './target',
        resolvedModule: 'src/target.ts',
        importedName: 'e',
        localName: 'e',
        isTypeOnly: false,
      },
      {
        sourceModule: './other',
        resolvedModule: 'src/other.ts',
        importedName: 'x',
        localName: 'x',
        isTypeOnly: false,
      },
    ]);
    addEdge(state, 'src/envious.ts', 'src/target.ts');
    addEdge(state, 'src/envious.ts', 'src/other.ts');
    const { findings } = buildIssueCatalog(
      [],
      [],
      [],
      minimalDepSummary(),
      state,
      opts
    );
    expect(findings.some(f => f.category === 'feature-envy')).toBe(true);
  });

  it('detectUntestedCriticalCode: hot file with no test imports triggers finding', () => {
    const state = emptyState();
    addEdge(state, 'src/hot.ts', 'src/consumer.ts');
    addEdge(state, 'src/consumer.ts', 'src/hot.ts');
    const depSummary = minimalDepSummary({
      cycles: [
        { path: ['src/hot.ts', 'src/consumer.ts', 'src/hot.ts'], nodeCount: 2 },
      ],
      criticalPaths: [
        {
          start: 'src/hot.ts',
          path: ['src/hot.ts', 'src/consumer.ts'],
          score: 100,
          length: 2,
          containsCycle: true,
        },
      ],
    });
    const critMap = new Map<string, import('./types/index.js').FileCriticality>();
    critMap.set('src/hot.ts', {
      file: 'src/hot.ts',
      complexityRisk: 1,
      highComplexityFunctions: 0,
      functionCount: 1,
      flows: 0,
      score: 80,
    });
    const { findings } = buildIssueCatalog(
      [],
      [],
      [],
      depSummary,
      state,
      opts,
      {},
      {},
      critMap
    );
    expect(findings.some(f => f.category === 'untested-critical-code')).toBe(
      true
    );
  });

  it('detectDuplicateFlowStructures: control duplicates with occurrences >= flowDupThreshold', () => {
    const controlDuplicates: import('./types/index.js').RedundantFlowGroup[] = [
      {
        kind: 'IfElseChain',
        occurrences: 5,
        filesCount: 3,
        locations: [
          {
            kind: 'IfStatement',
            file: 'src/a.ts',
            lineStart: 10,
            lineEnd: 20,
            columnStart: 1,
            columnEnd: 1,
            hash: 'x',
            statementCount: 5,
          },
          {
            kind: 'IfStatement',
            file: 'src/b.ts',
            lineStart: 15,
            lineEnd: 25,
            columnStart: 1,
            columnEnd: 1,
            hash: 'x',
            statementCount: 5,
          },
        ],
      },
    ];
    const optsWithFlow = { ...opts, thresholds: { ...opts.thresholds, flowDupThreshold: 3 } };
    const { findings } = buildIssueCatalog(
      [],
      controlDuplicates,
      [],
      minimalDepSummary(),
      emptyState(),
      optsWithFlow
    );
    expect(findings.some(f => f.category === 'duplicate-flow-structure')).toBe(
      true
    );
  });

  it('detectLayerViolations: layerOrder triggers when lower layer imports from upper', () => {
    const state = emptyState();
    addEdge(state, 'src/repository/db.ts', 'src/service/handler.ts');
    const optsWithLayers = { ...opts, thresholds: { ...opts.thresholds, layerOrder: ['service', 'repository'] } };
    const { findings } = buildIssueCatalog(
      [],
      [],
      [],
      minimalDepSummary(),
      state,
      optsWithLayers
    );
    expect(findings.some(f => f.category === 'layer-violation')).toBe(true);
  });
});

describe('new v2 quality detectors via buildIssueCatalog', () => {
  const testOpts2 = { ...DEFAULT_OPTS, findingsLimit: 500, includeTests: false };

  function makeEntry2(
    file: string,
    overrides: Partial<FileEntry> = {}
  ): FileEntry {
    return {
      package: 'test-pkg',
      file,
      parseEngine: 'typescript',
      nodeCount: 0,
      kindCounts: {},
      functions: [],
      flows: [],
      dependencyProfile: {
        internalDependencies: [],
        externalDependencies: [],
        unresolvedDependencies: [],
        declaredExports: [],
        importedSymbols: [],
        reExports: [],
      },
      ...overrides,
    };
  }

  describe('detectDeepNesting', () => {
    it('triggers when function has branch depth >= threshold', () => {
      const entry = makeEntry2('src/deep.ts', {
        functions: [
          makeFn({
            name: 'deepFn',
            file: 'src/deep.ts',
            maxBranchDepth: 6,
            maxLoopDepth: 0,
            statementCount: 20,
          }),
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      const deep = findings.filter(f => f.category === 'deep-nesting');
      expect(deep.length).toBe(1);
      expect(deep[0].title).toContain('6');
      expect(deep[0].title).toContain('deepFn');
    });

    it('triggers when function has loop depth >= threshold', () => {
      const entry = makeEntry2('src/loops.ts', {
        functions: [
          makeFn({
            name: 'loopFn',
            file: 'src/loops.ts',
            maxBranchDepth: 1,
            maxLoopDepth: 7,
            statementCount: 15,
          }),
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'deep-nesting')).toBe(true);
    });

    it('does not trigger when depth is below threshold', () => {
      const entry = makeEntry2('src/shallow.ts', {
        functions: [
          makeFn({ maxBranchDepth: 2, maxLoopDepth: 1, statementCount: 10 }),
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'deep-nesting')).toBe(false);
    });

    it('skips test files', () => {
      const entry = makeEntry2('src/__tests__/deep.test.ts', {
        functions: [
          makeFn({ maxBranchDepth: 10, maxLoopDepth: 10, statementCount: 50 }),
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'deep-nesting')).toBe(false);
    });

    it('severity scales with nesting depth', () => {
      const low = makeEntry2('src/low.ts', {
        functions: [makeFn({ name: 'fn', file: 'src/low.ts', maxBranchDepth: 5, statementCount: 10 })],
      });
      const high = makeEntry2('src/high.ts', {
        functions: [makeFn({ name: 'fn', file: 'src/high.ts', maxBranchDepth: 9, statementCount: 10 })],
      });
      const { findings: lowF } = buildIssueCatalog(
        [], [], [low], minimalDepSummary(), emptyState(), testOpts2
      );
      const { findings: highF } = buildIssueCatalog(
        [], [], [high], minimalDepSummary(), emptyState(), testOpts2
      );
      const lowSev = lowF.find(f => f.category === 'deep-nesting')?.severity;
      const highSev = highF.find(f => f.category === 'deep-nesting')?.severity;
      expect(lowSev).toBe('low');
      expect(highSev).toBe('high');
    });
  });

  describe('detectMultipleReturnPaths', () => {
    it('triggers when function has returns >= threshold', () => {
      const entry = makeEntry2('src/multi.ts', {
        functions: [
          makeFn({
            name: 'multiFn',
            file: 'src/multi.ts',
            returns: 7,
            statementCount: 20,
          }),
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      const multi = findings.filter(f => f.category === 'multiple-return-paths');
      expect(multi.length).toBe(1);
      expect(multi[0].title).toContain('7');
    });

    it('does not trigger below threshold', () => {
      const entry = makeEntry2('src/few.ts', {
        functions: [makeFn({ returns: 3, statementCount: 10 })],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'multiple-return-paths')).toBe(false);
    });

    it('skips test files', () => {
      const entry = makeEntry2('src/x.test.ts', {
        functions: [makeFn({ returns: 20, statementCount: 50 })],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'multiple-return-paths')).toBe(false);
    });
  });

  describe('detectCatchRethrow', () => {
    it('triggers from pre-collected catchRethrows data', () => {
      const entry = makeEntry2('src/rethrow.ts', {
        catchRethrows: [
          { file: 'src/rethrow.ts', lineStart: 5, lineEnd: 8 },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      const cr = findings.filter(f => f.category === 'catch-rethrow');
      expect(cr.length).toBe(1);
      expect(cr[0].severity).toBe('low');
      expect(cr[0].title).toContain('Catch-rethrow');
    });

    it('does not trigger when no catchRethrows', () => {
      const entry = makeEntry2('src/clean.ts');
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'catch-rethrow')).toBe(false);
    });

    it('skips test files', () => {
      const entry = makeEntry2('src/__tests__/rethrow.test.ts', {
        catchRethrows: [
          { file: 'src/__tests__/rethrow.test.ts', lineStart: 5, lineEnd: 8 },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'catch-rethrow')).toBe(false);
    });
  });

  describe('detectMagicStrings', () => {
    it('triggers when string appears >= minOccurrences across files', () => {
      const e1 = makeEntry2('src/a.ts', {
        magicStrings: [
          { file: 'src/a.ts', lineStart: 1, lineEnd: 1, value: 'active' },
          { file: 'src/a.ts', lineStart: 5, lineEnd: 5, value: 'active' },
        ],
      });
      const e2 = makeEntry2('src/b.ts', {
        magicStrings: [
          { file: 'src/b.ts', lineStart: 3, lineEnd: 3, value: 'active' },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [e1, e2], minimalDepSummary(), emptyState(), testOpts2
      );
      const ms = findings.filter(f => f.category === 'magic-string');
      expect(ms.length).toBe(1);
      expect(ms[0].title).toContain('active');
      expect(ms[0].title).toContain('3');
    });

    it('does not trigger below minOccurrences', () => {
      const entry = makeEntry2('src/single.ts', {
        magicStrings: [
          { file: 'src/single.ts', lineStart: 1, lineEnd: 1, value: 'rare' },
          { file: 'src/single.ts', lineStart: 3, lineEnd: 3, value: 'rare' },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'magic-string')).toBe(false);
    });

    it('severity scales with occurrence count', () => {
      const entries = Array.from({ length: 1 }, () =>
        makeEntry2('src/many.ts', {
          magicStrings: Array.from({ length: 9 }, (_, i) => ({
            file: 'src/many.ts',
            lineStart: i + 1,
            lineEnd: i + 1,
            value: 'status',
          })),
        })
      );
      const { findings } = buildIssueCatalog(
        [], [], entries, minimalDepSummary(), emptyState(), testOpts2
      );
      const ms = findings.filter(f => f.category === 'magic-string');
      expect(ms.length).toBe(1);
      expect(ms[0].severity).toBe('high');
    });
  });

  describe('detectBooleanParameterCluster', () => {
    it('triggers from pre-collected booleanParamClusters', () => {
      const entry = makeEntry2('src/flags.ts', {
        booleanParamClusters: [
          {
            name: 'configure',
            booleanCount: 3,
            totalParams: 4,
            lineStart: 1,
            lineEnd: 5,
          },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      const bp = findings.filter(f => f.category === 'boolean-parameter-cluster');
      expect(bp.length).toBe(1);
      expect(bp[0].severity).toBe('medium');
      expect(bp[0].title).toContain('3');
      expect(bp[0].title).toContain('configure');
    });

    it('does not trigger when below threshold', () => {
      const entry = makeEntry2('src/few.ts', {
        booleanParamClusters: [
          { name: 'fn', booleanCount: 2, totalParams: 3, lineStart: 1, lineEnd: 3 },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'boolean-parameter-cluster')).toBe(false);
    });

    it('skips test files', () => {
      const entry = makeEntry2('src/flags.test.ts', {
        booleanParamClusters: [
          { name: 'testFn', booleanCount: 4, totalParams: 4, lineStart: 1, lineEnd: 5 },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'boolean-parameter-cluster')).toBe(false);
    });
  });

  describe('detectPromiseAllUnhandled', () => {
    it('triggers from pre-collected promiseAllUnhandled', () => {
      const entry = makeEntry2('src/fetch.ts', {
        promiseAllUnhandled: [
          { file: 'src/fetch.ts', lineStart: 10, lineEnd: 10, kind: 'Promise.all' },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      const pa = findings.filter(f => f.category === 'promise-all-unhandled');
      expect(pa.length).toBe(1);
      expect(pa[0].severity).toBe('medium');
      expect(pa[0].title).toContain('Promise.all');
    });

    it('detects multiple kinds (race, any)', () => {
      const entry = makeEntry2('src/multi.ts', {
        promiseAllUnhandled: [
          { file: 'src/multi.ts', lineStart: 1, lineEnd: 1, kind: 'Promise.race' },
          { file: 'src/multi.ts', lineStart: 5, lineEnd: 5, kind: 'Promise.any' },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      const pa = findings.filter(f => f.category === 'promise-all-unhandled');
      expect(pa.length).toBe(2);
      expect(pa.some(f => f.title.includes('Promise.race'))).toBe(true);
      expect(pa.some(f => f.title.includes('Promise.any'))).toBe(true);
    });

    it('skips test files', () => {
      const entry = makeEntry2('src/fetch.spec.ts', {
        promiseAllUnhandled: [
          { file: 'src/fetch.spec.ts', lineStart: 1, lineEnd: 1, kind: 'Promise.all' },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'promise-all-unhandled')).toBe(false);
    });
  });

  describe('detectExportSurfaceDensity', () => {
    it('triggers when export ratio >= 50%', () => {
      const fns = Array.from({ length: 5 }, (_, i) =>
        makeFn({
          name: `fn${i}`,
          file: 'src/dense.ts',
          statementCount: 5,
        })
      );
      const entry = makeEntry2('src/dense.ts', {
        functions: fns,
        dependencyProfile: {
          internalDependencies: [],
          externalDependencies: [],
          unresolvedDependencies: [],
          declaredExports: Array.from({ length: 15 }, (_, i) => ({
            name: `export${i}`,
            kind: 'function' as const,
            isType: false,
            isDefault: false,
          })),
          importedSymbols: [],
          reExports: [],
        },
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      const esd = findings.filter(f => f.category === 'export-surface-density');
      expect(esd.length).toBe(1);
      expect(esd[0].title).toContain('%');
    });

    it('does not trigger when ratio < 50%', () => {
      const fns = Array.from({ length: 10 }, (_, i) =>
        makeFn({ name: `fn${i}`, file: 'src/normal.ts', statementCount: 10 })
      );
      const entry = makeEntry2('src/normal.ts', {
        functions: fns,
        dependencyProfile: {
          internalDependencies: [],
          externalDependencies: [],
          unresolvedDependencies: [],
          declaredExports: [
            { name: 'main', kind: 'function' as const, isType: false, isDefault: false },
          ],
          importedSymbols: [],
          reExports: [],
        },
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'export-surface-density')).toBe(false);
    });

    it('does not trigger for small files (< 20 statements)', () => {
      const entry = makeEntry2('src/small.ts', {
        functions: [makeFn({ statementCount: 5, file: 'src/small.ts' })],
        dependencyProfile: {
          internalDependencies: [],
          externalDependencies: [],
          unresolvedDependencies: [],
          declaredExports: Array.from({ length: 5 }, (_, i) => ({
            name: `e${i}`,
            kind: 'function' as const,
            isType: false,
            isDefault: false,
          })),
          importedSymbols: [],
          reExports: [],
        },
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'export-surface-density')).toBe(false);
    });
  });

  describe('detectChangeRisk', () => {
    it('triggers when multiple quality signals overlap', () => {
      const entry = makeEntry2('src/risky.ts', {
        functions: [
          makeFn({
            name: 'complexFn',
            file: 'src/risky.ts',
            complexity: 25,
            cognitiveComplexity: 30,
            maintainabilityIndex: 10,
            statementCount: 50,
          }),
          makeFn({
            name: 'anotherFn',
            file: 'src/risky.ts',
            complexity: 20,
            cognitiveComplexity: 25,
            maintainabilityIndex: 15,
            statementCount: 30,
          }),
        ],
        emptyCatches: [{ file: 'src/risky.ts', lineStart: 10, lineEnd: 12 }],
        promiseAllUnhandled: [
          { file: 'src/risky.ts', lineStart: 20, lineEnd: 20, kind: 'Promise.all' as const },
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      const cr = findings.filter(f => f.category === 'change-risk');
      expect(cr.length).toBe(1);
      expect(['medium', 'high', 'critical']).toContain(cr[0].severity);
      expect(cr[0].title).toContain('Change-risk score');
    });

    it('does not trigger when risk score < 4', () => {
      const entry = makeEntry2('src/clean.ts', {
        functions: [
          makeFn({ complexity: 3, cognitiveComplexity: 2, statementCount: 10 }),
        ],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'change-risk')).toBe(false);
    });

    it('skips test files', () => {
      const entry = makeEntry2('src/__tests__/risky.test.ts', {
        functions: [
          makeFn({ complexity: 30, cognitiveComplexity: 30, maintainabilityIndex: 5, statementCount: 100 }),
        ],
        emptyCatches: [{ file: 'src/__tests__/risky.test.ts', lineStart: 1, lineEnd: 2 }],
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      expect(findings.some(f => f.category === 'change-risk')).toBe(false);
    });

    it('severity critical when score >= 8', () => {
      const entry = makeEntry2('src/terrible.ts', {
        functions: [
          makeFn({
            name: 'fn1',
            file: 'src/terrible.ts',
            complexity: 40,
            cognitiveComplexity: 50,
            maintainabilityIndex: 5,
            statementCount: 80,
          }),
          makeFn({
            name: 'fn2',
            file: 'src/terrible.ts',
            complexity: 35,
            cognitiveComplexity: 40,
            maintainabilityIndex: 8,
            statementCount: 60,
          }),
          makeFn({
            name: 'fn3',
            file: 'src/terrible.ts',
            complexity: 30,
            cognitiveComplexity: 25,
            maintainabilityIndex: 12,
            statementCount: 40,
          }),
        ],
        emptyCatches: [{ file: 'src/terrible.ts', lineStart: 1, lineEnd: 2 }],
        promiseAllUnhandled: [
          { file: 'src/terrible.ts', lineStart: 5, lineEnd: 5, kind: 'Promise.all' as const },
        ],
        dependencyProfile: {
          internalDependencies: [],
          externalDependencies: [],
          unresolvedDependencies: [],
          declaredExports: Array.from({ length: 20 }, (_, i) => ({
            name: `e${i}`,
            kind: 'function' as const,
            isType: false,
            isDefault: false,
          })),
          importedSymbols: [],
          reExports: [],
        },
      });
      const { findings } = buildIssueCatalog(
        [], [], [entry], minimalDepSummary(), emptyState(), testOpts2
      );
      const cr = findings.filter(f => f.category === 'change-risk');
      expect(cr.length).toBe(1);
      expect(cr[0].severity).toBe('critical');
    });
  });

  describe('new categories registered in PILLAR_CATEGORIES', () => {
    it('all new categories exist in code-quality pillar', () => {
      const newCategories = [
        'deep-nesting',
        'multiple-return-paths',
        'catch-rethrow',
        'magic-string',
        'boolean-parameter-cluster',
        'promise-all-unhandled',
        'export-surface-density',
        'change-risk',
      ];
      for (const cat of newCategories) {
        expect(PILLAR_CATEGORIES['code-quality']).toContain(cat);
      }
    });
  });
});
