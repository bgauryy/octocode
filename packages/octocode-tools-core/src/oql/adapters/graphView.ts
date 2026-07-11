/**
 * Graph-view construction for the `research`/`graph` targets: packet paging,
 * graph filters, node/edge/fact tree building, and native AST edge overlay.
 *
 * Pure over research packets + native graph facts — no tool invocation here
 * (LSP proof escalation lives in graphProof.ts; the execute* adapters stay in
 * researchTargets.ts).
 *
 * Filter parsing/matching lives in graphView/filters.ts; the native AST edge
 * overlay lives in graphView/nativeEdges.ts. This file re-exports the public
 * surface and owns the orchestration (`buildGraphView`) that combines them.
 */
import type { analyzeResearchFlow } from '../research/analyze.js';
import {
  tallyPacketVerdicts,
  type EvidenceEdge,
  type EvidenceFact,
  type EvidenceSubject,
  type MissingProof,
  type ResearchEvidencePacket,
  type ResearchGraphSummary,
} from '../research/packets.js';
import type { OqlGraphData, OqlQuery, Pagination } from '../types.js';
import {
  addEdge,
  addFact,
  addNode,
  missingProofKey,
  packetMatchesGraphFilters,
  type GraphFilters,
} from './graphView/filters.js';
import { addNativeGraphEdges } from './graphView/nativeEdges.js';

export type { GraphDirection, GraphFilters } from './graphView/filters.js';
export {
  graphFilters,
  packetMatchesGraphFilters,
} from './graphView/filters.js';
export { nativeGraphSummary } from './graphView/nativeEdges.js';

const DEFAULT_RESEARCH_PACKET_PAGE_SIZE = 25;
export function packetPage(
  query: OqlQuery,
  totalItems: number
): {
  packetsStart: number;
  packetsEnd: number;
  pagination: Pagination;
} {
  const currentPage = Math.max(1, query.page ?? 1);
  const itemsPerPage = Math.max(
    1,
    query.itemsPerPage ?? query.limit ?? DEFAULT_RESEARCH_PACKET_PAGE_SIZE
  );
  const totalPages = Math.max(1, Math.ceil(totalItems / itemsPerPage));
  const packetsStart = (currentPage - 1) * itemsPerPage;
  return {
    packetsStart,
    packetsEnd: packetsStart + itemsPerPage,
    pagination: {
      currentPage,
      totalPages,
      itemsPerPage,
      totalItems,
      hasMore: currentPage < totalPages,
    },
  };
}

export function buildGraphView(
  query: OqlQuery,
  packets: ResearchEvidencePacket[],
  graphSummary: ResearchGraphSummary,
  filters: GraphFilters,
  nativeGraphFacts: Awaited<
    ReturnType<typeof analyzeResearchFlow>
  >['graphFacts'],
  root: string
): {
  data: OqlGraphData;
  pagination: Pagination;
} {
  const filteredPackets = packets.filter(p =>
    packetMatchesGraphFilters(p, filters)
  );
  const pageWindow = packetPage(query, filteredPackets.length);
  const pagedPackets = filteredPackets.slice(
    pageWindow.packetsStart,
    pageWindow.packetsEnd
  );

  const nodes = new Map<string, EvidenceSubject>();
  const edges = new Map<string, EvidenceEdge>();
  const facts = new Map<string, EvidenceFact>();
  const missingProof = new Map<string, MissingProof>();
  const byVerdict: Record<string, number> = {};
  const proofStatus: Record<string, number> = {};

  for (const packet of filteredPackets) {
    byVerdict[packet.verdict] = (byVerdict[packet.verdict] ?? 0) + 1;
    proofStatus[packet.proofStatus] =
      (proofStatus[packet.proofStatus] ?? 0) + 1;
  }

  for (const packet of pagedPackets) {
    addNode(nodes, packet.subject);

    if (filters.includeFacts) {
      for (const fact of packet.why) addFact(facts, fact, filters);
    }
    if (filters.includeEdges) {
      if (filters.direction !== 'outgoing') {
        for (const edge of packet.retainedBy) {
          addEdge(nodes, edges, edge, filters);
        }
      }
      if (filters.direction !== 'incoming') {
        for (const edge of packet.retains ?? []) {
          addEdge(nodes, edges, edge, filters);
        }
      }
    }
    for (const proof of packet.missingProof) {
      missingProof.set(missingProofKey(proof), proof);
    }
  }

  if (filters.includeEdges) {
    addNativeGraphEdges(
      root,
      nativeGraphFacts,
      new Set(nodes.keys()),
      nodes,
      edges,
      filters
    );
  }

  return {
    data: {
      kind: 'relationshipGraph',
      filters: {
        ...(filters.subject ? { subject: filters.subject } : {}),
        ...(filters.subjectKind ? { subjectKind: filters.subjectKind } : {}),
        ...(filters.relations ? { relation: [...filters.relations] } : {}),
        ...(filters.verdicts ? { verdict: [...filters.verdicts] } : {}),
        direction: filters.direction,
        includePackets: filters.includePackets,
        includeFacts: filters.includeFacts,
        includeEdges: filters.includeEdges,
      },
      summary: {
        totalPackets: filteredPackets.length,
        returnedPackets: pagedPackets.length,
        nodes: nodes.size,
        edges: edges.size,
        facts: facts.size,
        missingProof: missingProof.size,
        byVerdict,
        proofStatus,
      },
      graphSummary,
      packetPage: pageWindow.pagination,
      nodes: [...nodes.values()],
      edges: [...edges.values()],
      facts: [...facts.values()],
      missingProof: [...missingProof.values()],
      ...(filters.includePackets ? { packets: pagedPackets } : {}),
      caveats: [
        'target:"graph" uses native AST facts where available plus research-packet reachability. LSP proof is page-bounded; follow next.page / next.semantic before treating deletion as safe.',
      ],
    },
    pagination: pageWindow.pagination,
  };
}

export function summarizePacketGraph(
  packets: readonly ResearchEvidencePacket[]
): ResearchGraphSummary {
  const { byVerdict, facts, edges } = tallyPacketVerdicts(packets);
  return {
    subjects: packets.length,
    facts,
    edges,
    byVerdict,
  };
}
