/**
 * Research evidence packets.
 *
 * Transforms the heuristic `ResearchAnalysisResult` (reachability + reference +
 * dependency analysis) into compact, decision-grade `ResearchEvidencePacket`s —
 * the agent's unit for "what looks dead, why, what keeps it alive, what proof is
 * missing, and is it safe to delete".
 *
 * HONESTY CONTRACT: this layer never invents proof. Native AST facts can prove
 * syntax-level declarations/imports/exports, but cross-file references remain
 * heuristic until LSP proof is attached by the graph adapter or followed via
 * `next.semantic`.
 *
 * This module is a thin barrel: the evidence model (types/interfaces) lives in
 * `./packets/types.ts`, and the packet-construction logic (including
 * `buildResearchPackets`/`tallyPacketVerdicts`) lives in `./packets/builders.ts`.
 * Both are re-exported here so existing `from '.../research/packets.js'`
 * imports keep working unchanged.
 */
export type {
  EvidenceSource,
  EvidenceConfidence,
  EvidenceLocation,
  EvidenceSubject,
  EvidenceClaim,
  EvidenceFlag,
  EvidenceFact,
  EvidenceRelation,
  EvidenceEdge,
  PacketVerdict,
  PacketProofStatus,
  MissingProofKind,
  MissingProof,
  ResearchEvidencePacket,
  ResearchGraphSummary,
  ResearchPacketBundle,
} from './packets/types.js';

export {
  buildResearchPackets,
  tallyPacketVerdicts,
} from './packets/builders.js';
