/**
 * Evidence model for research packets.
 *
 * Pure type/interface definitions describing the evidence graph — subjects,
 * facts, edges, verdicts, and the `ResearchEvidencePacket` shape itself. No
 * construction logic here; see `./builders.ts` for how `ResearchAnalysisResult`
 * is turned into packets built from these types.
 *
 * HONESTY CONTRACT: this layer never invents proof. Native AST facts can prove
 * syntax-level declarations/imports/exports, but cross-file references remain
 * heuristic until LSP proof is attached by the graph adapter or followed via
 * `next.semantic`.
 */
import type { OqlContinuation } from '../../types.js';

export type EvidenceSource =
  | 'ripgrep'
  | 'regex'
  | 'ast'
  | 'symbols'
  | 'lsp'
  | 'manifest'
  | 'graph'
  | 'exactRead';

export type EvidenceConfidence = 'exact' | 'heuristic' | 'partial';

export interface EvidenceLocation {
  uri: string;
  range?: {
    start: { line: number; character?: number };
    end?: { line: number; character?: number };
  };
}

export interface EvidenceSubject extends EvidenceLocation {
  id: string;
  kind:
    | 'file'
    | 'symbol'
    | 'function'
    | 'class'
    | 'method'
    | 'interface'
    | 'type'
    | 'dependency'
    | 'package'
    | 'entrypoint';
  name?: string;
  symbolKind?: number | string;
  language?: string;
}

export type EvidenceClaim =
  | 'defines'
  | 'declares'
  | 'exports'
  | 'imports'
  | 'references'
  | 'calls'
  | 'extends'
  | 'implements'
  | 'typeUses'
  | 'entrypoint'
  | 'unresolved'
  | 'textMatch'
  | 'structuralMatch';

export type EvidenceFlag =
  | 'same-file'
  | 'external'
  | 'type-only'
  | 'test-only'
  | 'dynamic'
  | 'string-only'
  | 'unreachable'
  | 'generated'
  | 'declaration';

export interface EvidenceFact {
  id: string;
  subject: EvidenceSubject;
  claim: EvidenceClaim;
  source: EvidenceSource;
  confidence: EvidenceConfidence;
  flags?: EvidenceFlag[];
  value?: unknown;
}

export type EvidenceRelation =
  | 'contains'
  | 'defines'
  | 'exports'
  | 'imports'
  | 'references'
  | 'calls'
  | 'constructs'
  | 'extends'
  | 'implements'
  | 'typeUses'
  | 'retains'
  | 'reachableFrom'
  | 'declaresDependency'
  | 'usesDependency';

export interface EvidenceEdge {
  id: string;
  from: EvidenceSubject;
  to: EvidenceSubject;
  relation: EvidenceRelation;
  source: EvidenceSource;
  confidence: EvidenceConfidence;
  flags?: EvidenceFlag[];
  via?: EvidenceLocation;
}

export type PacketVerdict =
  | 'reachable'
  | 'candidate-dead'
  | 'transitive-dead'
  | 'candidate-unused-file'
  | 'candidate-unused-dependency'
  | 'unknown';

export type PacketProofStatus =
  | 'candidate'
  | 'confirmed-by-lsp'
  | 'confirmed-by-ast-and-lsp'
  | 'needs-framework-graph'
  | 'conflicting-evidence';

export type MissingProofKind =
  | 'lsp-unavailable'
  | 'dynamic-import-unresolved'
  | 'framework-entrypoint-unknown'
  | 'tsconfig-paths-unresolved'
  | 'manifest-rule-missing'
  | 'parser-failed'
  | 'pagination-open';

export interface MissingProof {
  kind: MissingProofKind;
  severity: 'low' | 'medium' | 'high';
  location?: EvidenceLocation;
}

export interface ResearchEvidencePacket {
  subject: EvidenceSubject;
  verdict: PacketVerdict;
  proofStatus: PacketProofStatus;
  why: EvidenceFact[];
  retainedBy: EvidenceEdge[];
  retains?: EvidenceEdge[];
  missingProof: MissingProof[];
  proof?: Record<string, unknown>;
  risk: {
    deleteRisk: 'low' | 'medium' | 'high' | 'unknown';
    reason: string;
  };
  next: Record<string, OqlContinuation>;
}

export interface ResearchGraphSummary {
  subjects: number;
  facts: number;
  edges: number;
  byVerdict: Record<PacketVerdict, number>;
}

export interface ResearchPacketBundle {
  packets: ResearchEvidencePacket[];
  graphSummary: ResearchGraphSummary;
}
