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
        directRefs: 0,
        externalRefs: 0,
        retainedBy: [],
        verdict: 'candidate-unused-export',
      },
    ],
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
    expect(used.retainedBy[0]!.relation).toBe('references');

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

  it('respects maxPackets and flags truncation', () => {
    const { packets, graphSummary } = buildResearchPackets(fixture(), {
      maxPackets: 2,
    });
    expect(packets.length).toBe(2);
    expect(graphSummary.packetsTruncated).toBe(true);
    // Truncation keeps the actionable packets, not the reachable one.
    expect(packets.some(p => p.verdict === 'reachable')).toBe(false);
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

    const packets = data.packets as Array<Record<string, unknown>>;
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
        params: { goal: 'dead code', mode: 'prove' },
      })
    );
    const data = (env.results[0] as { data: Record<string, unknown> }).data;
    const caveats = data.caveats as string[];
    expect(caveats.some(c => c.toLowerCase().includes('candidate-grade'))).toBe(
      true
    );
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
