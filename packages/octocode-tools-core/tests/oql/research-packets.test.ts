/**
 * Research evidence packets: the candidate decision graph derived from the
 * heuristic analysis. Verifies verdict mapping, honest proof status / missing
 * proof, risk, executable continuations, and the target:"research" integration.
 */
import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildResearchPackets } from '../../src/oql/research/packets.js';
import type { ResearchAnalysisResult } from '../../src/oql/research/analyze.js';
import { runOqlSearch } from '../../src/oql/run.js';
import { isBatchEnvelope } from '../../src/oql/types.js';

const OQL_SRC = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../src/oql'
);
const ENGINE_SIGNATURES_SRC = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../../octocode-engine/src/signatures'
);
function single(r: Awaited<ReturnType<typeof runOqlSearch>>) {
  if (isBatchEnvelope(r)) throw new Error('expected single');
  return r;
}

const ZERO_SUMMARY = {
  manifests: 0,
  sourceFiles: 0,
  entrypoints: 0,
  reachableFiles: 0,
  unusedFiles: 0,
  unlistedDependencies: 0,
  unusedDependencies: 0,
  duplicateDependencies: 0,
  exportedSymbols: 0,
  candidateUnusedExports: 0,
  transitiveDeadExports: 0,
  nativeGraphFiles: 0,
  nativeGraphDeclarations: 0,
  nativeGraphCalls: 0,
};

function fixture(): ResearchAnalysisResult {
  return {
    kind: 'researchFlow',
    goal: 'what is dead',
    intent: 'reachability',
    facets: ['symbols', 'files', 'dependencies'],
    mode: 'analyze',
    root: '/repo',
    flow: [],
    summary: ZERO_SUMMARY,
    manifests: [],
    files: [
      {
        kind: 'unusedFile',
        file: 'src/orphan.ts',
        retainedBy: [],
        verdict: 'unused-file',
      },
    ],
    dependencies: [
      {
        kind: 'unusedDependency',
        packageName: 'left-pad',
        manifest: 'package.json',
        usedBy: [],
        declaredIn: ['dependencies'],
        verdict: 'candidate-unused-dependency',
      },
      {
        kind: 'unlistedDependency',
        packageName: 'lodash',
        manifest: 'package.json',
        usedBy: ['src/a.ts'],
        declaredIn: [],
        verdict: 'unlisted-dependency',
      },
    ],
    symbols: [
      {
        symbol: 'usedFn',
        kind: 'function',
        file: 'src/a.ts',
        line: 10,
        evidenceSource: 'ast',
        retentionSource: 'ast',
        directRefs: 2,
        externalRefs: 1,
        retainedBy: ['src/b.ts'],
        verdict: 'reachable',
      },
      {
        symbol: 'deadFn',
        kind: 'function',
        file: 'src/a.ts',
        line: 20,
        evidenceSource: 'ast',
        retentionSource: 'ast',
        directRefs: 0,
        externalRefs: 0,
        retainedBy: [],
        verdict: 'candidate-unused-export',
      },
    ],
    graphFacts: [],
    graphCapabilities: {
      graphFactExtensions: ['ts', 'rs', 'py'],
      capabilityCount: 3,
      factFamilies: ['calls', 'declarations'],
      sourceFilesByLanguage: {},
      graphFilesByLanguage: {},
      missingGraphFacts: [],
    },
    caveats: [],
  };
}

describe('buildResearchPackets', () => {
  it('maps verdicts, honest proof status, risk, and continuations', () => {
    const { packets, graphSummary } = buildResearchPackets(fixture());

    // 2 symbols + 1 unused file + 1 unused dependency (unlisted is not a packet).
    expect(packets.length).toBe(4);

    const dead = packets.find(p => p.subject.id === 'sym:src/a.ts#deadFn')!;
    expect(dead.verdict).toBe('candidate-dead');
    expect(dead.proofStatus).toBe('candidate');
    expect(dead.risk.deleteRisk).toBe('medium');
    // Heuristic, not LSP -> high-severity missing proof for a dead candidate.
    const lsp = dead.missingProof.find(m => m.kind === 'lsp-unavailable')!;
    expect(lsp.severity).toBe('high');
    // Executable path to upgrade the candidate to proof.
    expect(dead.next['next.semantic']?.query).toMatchObject({
      target: 'semantics',
      params: { type: 'references', symbolName: 'deadFn' },
    });
    expect(dead.next['next.fetch']).toBeDefined();

    const used = packets.find(p => p.subject.id === 'sym:src/a.ts#usedFn')!;
    expect(used.verdict).toBe('reachable');
    expect(used.risk.deleteRisk).toBe('high');
    expect(used.retainedBy.length).toBe(1);
    expect(used.retainedBy[0]!.relation).toBe('imports');
    expect(used.retainedBy[0]!.source).toBe('ast');
    expect(used.retainedBy[0]!.confidence).toBe('exact');

    const file = packets.find(p => p.subject.kind === 'file')!;
    expect(file.verdict).toBe('candidate-unused-file');
    expect(
      file.missingProof.some(m => m.kind === 'dynamic-import-unresolved')
    ).toBe(true);

    const dep = packets.find(p => p.subject.kind === 'dependency')!;
    expect(dep.verdict).toBe('candidate-unused-dependency');
    expect(dep.subject.name).toBe('left-pad');

    // Actionable (dead/unused) packets sort before reachable.
    expect(packets[packets.length - 1]!.verdict).toBe('reachable');

    expect(graphSummary.byVerdict.reachable).toBe(1);
    expect(graphSummary.byVerdict['candidate-dead']).toBe(1);
    expect(graphSummary.subjects).toBe(4);
  });

  it('keeps all packets and leaves response sizing to pagination', () => {
    const { packets, graphSummary } = buildResearchPackets(fixture());
    expect(packets.length).toBe(4);
    expect(graphSummary.subjects).toBe(4);
    // Sorting still puts actionable packets before reachable packets.
    expect(packets[packets.length - 1]!.verdict).toBe('reachable');
  });
});

describe('target:"research" emits packets + graphSummary', () => {
  it('analyze mode over a real directory produces decision-grade packets', async () => {
    const env = single(
      await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: OQL_SRC },
        params: {
          goal: 'what looks dead and why?',
          mode: 'analyze',
          facets: ['symbols', 'files', 'dependencies', 'relations'],
        },
        itemsPerPage: 2,
      })
    );
    const row = env.results[0] as {
      kind: string;
      data: Record<string, unknown>;
    };
    expect(row.kind).toBe('record');
    const data = row.data;
    expect(Array.isArray(data.packets)).toBe(true);
    expect(data.graphSummary).toBeDefined();
    expect(data.packetPage).toMatchObject({
      currentPage: 1,
      itemsPerPage: 2,
    });
    expect(env.pagination).toMatchObject({
      currentPage: 1,
      itemsPerPage: 2,
    });
    expect(env.pagination?.totalItems).toBeGreaterThanOrEqual(1);
    expect(env.pagination?.totalItems).toBeGreaterThanOrEqual(
      (data.packets as unknown[]).length
    );
    expect(data.symbols).toBeUndefined();
    expect(data.files).toBeUndefined();

    const packets = data.packets as Array<Record<string, unknown>>;
    expect(packets.length).toBeLessThanOrEqual(2);
    for (const p of packets.slice(0, 20)) {
      expect(p.subject).toBeDefined();
      expect(typeof p.verdict).toBe('string');
      expect(typeof p.proofStatus).toBe('string');
      expect(Array.isArray(p.missingProof)).toBe(true);
      expect(p.risk).toBeDefined();
      expect(p.next).toBeDefined();
    }
    // research is candidate-grade, never proof.
    expect(env.evidence.kind).not.toBe('proof');
  });

  it('mode:"prove" is accepted and honestly flags missing LSP proof', async () => {
    const env = single(
      await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: OQL_SRC },
        params: { goal: 'dead code', intent: 'reachability', mode: 'prove' },
      })
    );
    const data = (env.results[0] as { data: Record<string, unknown> }).data;
    expect(data.mode).toBe('prove');
    const caveats = data.caveats as string[];
    expect(caveats.some(c => c.toLowerCase().includes('candidate-grade'))).toBe(
      true
    );
  });

  it('mode:"prove" requires explicit intent', async () => {
    const env = single(
      await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: OQL_SRC },
        params: { goal: 'dead code', mode: 'prove' },
      })
    );
    expect(env.results.length).toBe(0);
    expect(env.diagnostics.some(d => d.code === 'invalidQuery')).toBe(true);
    expect(env.diagnostics[0]?.queryPath).toBe('params.intent');
    expect(env.evidence.answerReady).toBe(false);
  });

  it('uses native graph capabilities to inventory Rust source files', async () => {
    const env = single(
      await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: ENGINE_SIGNATURES_SRC },
        params: {
          goal: 'check non-js graph inventory',
          intent: 'symbols',
          facets: ['symbols', 'relations'],
          maxFiles: 200,
        },
        itemsPerPage: 3,
      })
    );
    const data = (env.results[0] as { data: Record<string, unknown> }).data;
    const summary = data.summary as Record<string, number>;
    expect(summary.sourceFiles).toBeGreaterThan(0);
    expect(summary.nativeGraphFiles).toBeGreaterThan(0);
    expect(summary.nativeGraphDeclarations).toBeGreaterThan(0);

    const capabilities = data.graphCapabilities as {
      graphFactExtensions: string[];
      sourceFilesByLanguage: Record<string, number>;
      graphFilesByLanguage: Record<string, number>;
    };
    expect(capabilities.graphFactExtensions).toContain('rs');
    expect(capabilities.sourceFilesByLanguage.rust).toBeGreaterThan(0);
    expect(capabilities.graphFilesByLanguage.rust).toBeGreaterThan(0);
  });

  it('rejects unimplemented facets instead of silently no-oping', async () => {
    const env = single(
      await runOqlSearch({
        target: 'research',
        from: { kind: 'local', path: OQL_SRC },
        params: { goal: 'find flows', facets: ['flows'] },
      } as never)
    );

    expect(env.evidence.kind).toBe('unsupported');
    expect(env.diagnostics.some(d => d.code === 'invalidQuery')).toBe(true);
    expect(env.diagnostics[0]?.message).toContain('params.facets.0');
  });
});

describe('target:"graph" emits relationship packets', () => {
  it('analyze mode returns a bounded relationship graph row', async () => {
    const env = single(
      await runOqlSearch({
        target: 'graph',
        from: { kind: 'local', path: OQL_SRC },
        params: {
          goal: 'what keeps dead-looking symbols alive?',
          intent: 'reachability',
          facets: ['symbols', 'files', 'dependencies', 'relations'],
        },
        itemsPerPage: 3,
      })
    );

    const row = env.results[0] as {
      kind: string;
      recordType?: string;
      data: Record<string, unknown>;
    };
    expect(row.kind).toBe('record');
    expect(row.recordType).toBe('graph');
    expect(row.data.kind).toBe('relationshipGraph');
    expect(Array.isArray(row.data.nodes)).toBe(true);
    expect(Array.isArray(row.data.edges)).toBe(true);
    expect(Array.isArray(row.data.facts)).toBe(true);
    expect(Array.isArray(row.data.missingProof)).toBe(true);
    expect(Array.isArray(row.data.packets)).toBe(true);
    expect(
      (
        row as {
          next?: Record<string, { query: Record<string, unknown> }>;
        }
      ).next?.['next.graph']?.query
    ).toMatchObject({
      target: 'graph',
      params: { proof: 'lsp', mode: 'prove', intent: 'reachability' },
    });
    expect(row.data.packetPage).toMatchObject({
      currentPage: 1,
      itemsPerPage: 3,
    });
    expect(env.pagination).toMatchObject({
      currentPage: 1,
      itemsPerPage: 3,
    });
    expect(env.evidence.kind).not.toBe('proof');
  });

  it('includes native AST graph edges that touch the visible packet nodes', async () => {
    const env = single(
      await runOqlSearch({
        target: 'graph',
        from: { kind: 'local', path: OQL_SRC },
        params: {
          intent: 'symbols',
          subject: 'CompiledMatch',
          includeEdges: true,
          includePackets: false,
          maxFiles: 12,
        },
        itemsPerPage: 1,
      })
    );

    const data = (env.results[0] as { data: Record<string, unknown> }).data;
    const edges = data.edges as Array<Record<string, unknown>>;
    expect(
      edges.some(edge => edge.source === 'ast' && edge.relation === 'contains')
    ).toBe(true);
    expect((data.summary as Record<string, number>).edges).toBeGreaterThan(0);
  });

  it('proof:"lsp" upgrades current-page symbols and exposes LSP reference edges', async () => {
    const env = single(
      await runOqlSearch({
        target: 'graph',
        from: { kind: 'local', path: OQL_SRC },
        params: {
          intent: 'symbols',
          subject: 'CompiledMatch',
          proof: 'lsp',
          proofLimit: 1,
          includeEdges: true,
          maxFiles: 12,
        },
        itemsPerPage: 1,
      })
    );

    const row = env.results[0] as {
      proofGrade?: string;
      data: Record<string, unknown>;
    };
    if (env.diagnostics.some(d => d.code === 'lspUnavailable')) {
      expect(row.proofGrade).toBe('missing');
      return;
    }

    expect(row.proofGrade).toBe('graph');
    expect((row.data.summary as Record<string, number>).missingProof).toBe(0);
    const edges = row.data.edges as Array<Record<string, unknown>>;
    expect(
      edges.some(
        edge => edge.source === 'lsp' && edge.relation === 'references'
      )
    ).toBe(true);
  });

  it('filters by verdict and incoming relation direction', async () => {
    const env = single(
      await runOqlSearch({
        target: 'graph',
        from: { kind: 'local', path: OQL_SRC },
        params: {
          intent: 'reachability',
          verdict: 'transitive-dead',
          relation: 'references',
          direction: 'incoming',
          includePackets: false,
        },
        itemsPerPage: 5,
      })
    );

    const data = (env.results[0] as { data: Record<string, unknown> }).data;
    expect(data.filters).toMatchObject({
      direction: 'incoming',
      includePackets: false,
    });
    expect(data.packets).toBeUndefined();
    const summary = data.summary as Record<string, unknown>;
    expect(summary.totalPackets).toBeGreaterThanOrEqual(0);
    expect(Array.isArray(data.edges)).toBe(true);
    for (const edge of data.edges as Array<Record<string, unknown>>) {
      expect(edge.relation).toBe('references');
    }
  });

  it('mode:"prove" requires explicit intent', async () => {
    const env = single(
      await runOqlSearch({
        target: 'graph',
        from: { kind: 'local', path: OQL_SRC },
        params: { goal: 'dead code graph', mode: 'prove' },
      })
    );
    expect(env.results.length).toBe(0);
    expect(env.diagnostics.some(d => d.code === 'invalidQuery')).toBe(true);
    expect(env.diagnostics[0]?.queryPath).toBe('params.intent');
    expect(env.evidence.answerReady).toBe(false);
  });
});
