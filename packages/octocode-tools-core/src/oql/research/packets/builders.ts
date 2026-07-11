/**
 * Research evidence packet builders.
 *
 * Transforms the heuristic `ResearchAnalysisResult` (reachability + reference +
 * dependency analysis) into compact, decision-grade `ResearchEvidencePacket`s —
 * the agent's unit for "what looks dead, why, what keeps it alive, what proof is
 * missing, and is it safe to delete". See `./types.ts` for the evidence model.
 */
import path from 'node:path';
import type { OqlContinuation } from '../../types.js';
import type { ResearchAnalysisResult } from '../analyze.js';
import type {
  EvidenceEdge,
  EvidenceFact,
  EvidenceSubject,
  MissingProof,
  PacketVerdict,
  ResearchEvidencePacket,
  ResearchGraphSummary,
  ResearchPacketBundle,
} from './types.js';

export function tallyPacketVerdicts(
  packets: readonly ResearchEvidencePacket[]
): Pick<ResearchGraphSummary, 'byVerdict' | 'facts' | 'edges'> {
  const byVerdict = {
    reachable: 0,
    'candidate-dead': 0,
    'transitive-dead': 0,
    'candidate-unused-file': 0,
    'candidate-unused-dependency': 0,
    unknown: 0,
  } as Record<PacketVerdict, number>;
  let facts = 0;
  let edges = 0;
  for (const p of packets) {
    byVerdict[p.verdict] += 1;
    facts += p.why.length;
    edges += p.retainedBy.length + (p.retains?.length ?? 0);
  }
  return { byVerdict, facts, edges };
}

export function buildResearchPackets(
  analysis: ResearchAnalysisResult
): ResearchPacketBundle {
  const root = analysis.root;

  const packets: ResearchEvidencePacket[] = [
    ...analysis.symbols.map(s => symbolPacket(root, s)),
    ...analysis.files.map(f => filePacket(root, f)),
    ...analysis.dependencies
      .filter(d => d.kind === 'unusedDependency')
      .map(d => dependencyPacket(d)),
  ];

  // Actionable (dead/unused) packets first; page/itemsPerPage controls response size.
  const rank = (v: PacketVerdict): number =>
    v === 'reachable' ? 1 : v === 'unknown' ? 0.5 : 0;
  packets.sort((a, b) => rank(a.verdict) - rank(b.verdict));

  const { byVerdict, facts, edges } = tallyPacketVerdicts(packets);

  return {
    packets,
    graphSummary: {
      subjects: packets.length,
      facts,
      edges,
      byVerdict,
    },
  };
}

/* ------------------------------ symbols --------------------------------- */

function symbolPacket(
  root: string,
  s: ResearchAnalysisResult['symbols'][number]
): ResearchEvidencePacket {
  const subject: EvidenceSubject = {
    id: `sym:${s.file}#${s.symbol}`,
    kind: 'symbol',
    name: s.symbol,
    symbolKind: s.kind,
    uri: s.file,
    range: { start: { line: s.line } },
  };
  const verdict = mapSymbolVerdict(s.verdict);
  const dead = verdict !== 'reachable';

  const why: EvidenceFact[] = [
    {
      id: `${subject.id}:exports`,
      subject,
      claim: 'exports',
      source: s.evidenceSource,
      confidence: s.evidenceSource === 'ast' ? 'exact' : 'heuristic',
      flags: ['declaration'],
    },
  ];
  const retainedBy: EvidenceEdge[] = s.retainedBy.map((refFile, i) => ({
    id: `${subject.id}:ref:${i}`,
    from: { id: `file:${refFile}`, kind: 'file', uri: refFile },
    to: subject,
    relation: s.retentionSource === 'ast' ? 'imports' : 'references',
    source: s.retentionSource === 'ast' ? 'ast' : 'ripgrep',
    confidence: s.retentionSource === 'ast' ? 'exact' : 'heuristic',
    flags: s.externalRefs > 0 ? undefined : ['unreachable'],
  }));

  const missingProof: MissingProof[] = [
    {
      kind: 'lsp-unavailable',
      severity: dead ? 'high' : 'low',
      location: { uri: s.file, range: { start: { line: s.line } } },
    },
  ];

  return {
    subject,
    verdict,
    proofStatus:
      verdict === 'transitive-dead' && s.directRefs > 0
        ? 'conflicting-evidence'
        : 'candidate',
    why,
    retainedBy,
    missingProof,
    risk: symbolRisk(verdict),
    next: symbolNext(root, s),
  };
}

function mapSymbolVerdict(
  v: ResearchAnalysisResult['symbols'][number]['verdict']
): PacketVerdict {
  switch (v) {
    case 'reachable':
      return 'reachable';
    case 'candidate-unused-export':
    case 'unused-export':
      return 'candidate-dead';
    case 'transitive-dead':
      return 'transitive-dead';
    default:
      return 'unknown';
  }
}

function symbolRisk(verdict: PacketVerdict): ResearchEvidencePacket['risk'] {
  switch (verdict) {
    case 'reachable':
      return {
        deleteRisk: 'high',
        reason:
          'Reachable from entrypoints via AST import/call retention (or lexical fallback); likely used — do not delete without confirming.',
      };
    case 'candidate-dead':
      return {
        deleteRisk: 'medium',
        reason:
          'No reachable retainers found in analyze (AST import/call scan, then lexical fallback). Confirm with LSP references (next.semantic) before deleting.',
      };
    case 'transitive-dead':
      return {
        deleteRisk: 'medium',
        reason:
          'Only retained from unreachable code (AST or lexical scan). Confirm the retention chain with LSP before deleting.',
      };
    default:
      return { deleteRisk: 'unknown', reason: 'Insufficient evidence.' };
  }
}

function symbolNext(
  root: string,
  s: ResearchAnalysisResult['symbols'][number]
): Record<string, OqlContinuation> {
  const abs = path.resolve(root, s.file);
  return {
    'next.fetch': {
      query: {
        schema: 'oql',
        target: 'content',
        from: { kind: 'local', path: abs },
        fetch: {
          content: {
            range: { startLine: s.line, contextLines: 3 },
            contentView: 'none',
          },
        },
      },
      why: 'Read the declaration at this symbol.',
      confidence: 'exact',
    },
    'next.semantic': {
      query: {
        schema: 'oql',
        target: 'semantics',
        from: { kind: 'local', path: abs },
        params: {
          type: 'references',
          symbolName: s.symbol,
          lineHint: s.line,
          includeDeclaration: false,
        },
      },
      why: 'Confirm references with LSP — upgrades this candidate to proof.',
      confidence: 'exact',
    },
    'next.search': {
      query: {
        schema: 'oql',
        target: 'code',
        from: { kind: 'local', path: root },
        where: { kind: 'text', value: s.symbol },
        view: 'discovery',
      },
      why: 'Find string/dynamic usages of the name the reference scan may miss.',
      confidence: 'heuristic',
    },
  };
}

/* ------------------------------- files ---------------------------------- */

function filePacket(
  root: string,
  f: ResearchAnalysisResult['files'][number]
): ResearchEvidencePacket {
  const subject: EvidenceSubject = {
    id: `file:${f.file}`,
    kind: 'file',
    uri: f.file,
  };
  return {
    subject,
    verdict: 'candidate-unused-file',
    proofStatus: 'needs-framework-graph',
    why: [
      {
        id: `${subject.id}:no-importer`,
        subject,
        claim: 'unresolved',
        source: 'graph',
        confidence: 'heuristic',
        value: {
          reason: 'no static importer reaches this file from an entrypoint',
        },
      },
    ],
    retainedBy: [],
    missingProof: [
      { kind: 'dynamic-import-unresolved', severity: 'medium' },
      { kind: 'framework-entrypoint-unknown', severity: 'medium' },
    ],
    risk: {
      deleteRisk: 'medium',
      reason:
        'No static importer found, but dynamic imports, framework entrypoints, and config globs are not modeled. Verify before deleting.',
    },
    next: {
      'next.fetch': {
        query: {
          schema: 'oql',
          target: 'content',
          from: { kind: 'local', path: path.resolve(root, f.file) },
          fetch: { content: { contentView: 'symbols' } },
        },
        why: 'Read the file outline.',
        confidence: 'exact',
      },
      'next.search': {
        query: {
          schema: 'oql',
          target: 'code',
          from: { kind: 'local', path: root },
          where: { kind: 'text', value: path.basename(f.file) },
          view: 'discovery',
        },
        why: 'Find references to the file name (dynamic import / config).',
        confidence: 'heuristic',
      },
    },
  };
}

/* ---------------------------- dependencies ------------------------------ */

function dependencyPacket(
  d: ResearchAnalysisResult['dependencies'][number]
): ResearchEvidencePacket {
  const subject: EvidenceSubject = {
    id: `dep:${d.manifest}#${d.packageName}`,
    kind: 'dependency',
    name: d.packageName,
    uri: d.manifest,
  };
  return {
    subject,
    verdict: 'candidate-unused-dependency',
    proofStatus: 'candidate',
    why: [
      {
        id: `${subject.id}:declared`,
        subject,
        claim: 'declares',
        source: 'manifest',
        confidence: 'exact',
        value: { declaredIn: d.declaredIn },
      },
      {
        id: `${subject.id}:no-import`,
        subject,
        claim: 'unresolved',
        source: 'regex',
        confidence: 'heuristic',
        value: { reason: 'no import specifier resolves to this package' },
      },
    ],
    retainedBy: [],
    missingProof: [
      { kind: 'manifest-rule-missing', severity: 'medium' },
      { kind: 'dynamic-import-unresolved', severity: 'low' },
    ],
    risk: {
      deleteRisk: 'medium',
      reason:
        'No import specifier resolves to this dependency, but it may be used via config, scripts, types, or transitive re-export. Verify before removing.',
    },
    next: {},
  };
}
