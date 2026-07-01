/**
 * Graph-view construction for the `research`/`graph` targets: packet paging,
 * graph filters, node/edge/fact tree building, and native AST edge overlay.
 *
 * Pure over research packets + native graph facts — no tool invocation here
 * (LSP proof escalation lives in graphProof.ts; the execute* adapters stay in
 * researchTargets.ts).
 */
import nodePath from 'node:path';
import type { analyzeResearchFlow } from '../research/analyze.js';
import type {
  EvidenceEdge,
  EvidenceFact,
  EvidenceRelation,
  EvidenceSubject,
  MissingProof,
  ResearchEvidencePacket,
  ResearchGraphSummary,
} from '../research/packets.js';
import type { OqlGraphData, OqlQuery, Pagination } from '../types.js';

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

type GraphDirection = 'incoming' | 'outgoing' | 'both';

export interface GraphFilters {
  subject?: string;
  subjectKind?: string;
  relations?: ReadonlySet<string>;
  verdicts?: ReadonlySet<string>;
  direction: GraphDirection;
  includePackets: boolean;
  includeFacts: boolean;
  includeEdges: boolean;
}

function stringFilterSet(value: unknown): ReadonlySet<string> | undefined {
  const values = Array.isArray(value)
    ? value
    : value === undefined
      ? []
      : [value];
  const normalized = values
    .filter((v): v is string => typeof v === 'string' && v.trim().length > 0)
    .map(v => v.trim().toLowerCase());
  return normalized.length ? new Set(normalized) : undefined;
}

export function graphFilters(p: Record<string, unknown>): GraphFilters {
  return {
    ...(typeof p.subject === 'string' && p.subject.trim()
      ? { subject: p.subject.trim().toLowerCase() }
      : {}),
    ...(typeof p.subjectKind === 'string' && p.subjectKind.trim()
      ? { subjectKind: p.subjectKind.trim().toLowerCase() }
      : {}),
    relations: stringFilterSet(p.relation),
    verdicts: stringFilterSet(p.verdict),
    direction:
      p.direction === 'incoming' || p.direction === 'outgoing'
        ? p.direction
        : 'both',
    includePackets: p.includePackets !== false,
    includeFacts: p.includeFacts !== false,
    includeEdges: p.includeEdges !== false,
  };
}

function subjectMatches(
  subject: EvidenceSubject,
  filters: GraphFilters
): boolean {
  if (filters.subjectKind) {
    const kind = subject.kind.toLowerCase();
    const symbolKind =
      subject.symbolKind === undefined
        ? undefined
        : String(subject.symbolKind).toLowerCase();
    if (kind !== filters.subjectKind && symbolKind !== filters.subjectKind) {
      return false;
    }
  }

  if (!filters.subject) return true;
  const haystack = [subject.id, subject.name, subject.uri]
    .filter((v): v is string => typeof v === 'string')
    .map(v => v.toLowerCase());
  return haystack.some(v => v.includes(filters.subject!));
}

function relationAllowed(
  relation: string | undefined,
  filters: GraphFilters
): boolean {
  if (!filters.relations || !relation) return true;
  return filters.relations.has(relation.toLowerCase());
}

export function packetMatchesGraphFilters(
  packet: ResearchEvidencePacket,
  filters: GraphFilters
): boolean {
  if (!subjectMatches(packet.subject, filters)) return false;
  if (filters.verdicts && !filters.verdicts.has(packet.verdict.toLowerCase())) {
    return false;
  }

  if (!filters.relations) return true;
  const incoming =
    filters.direction !== 'outgoing' &&
    packet.retainedBy.some(e => relationAllowed(e.relation, filters));
  const outgoing =
    filters.direction !== 'incoming' &&
    (packet.retains ?? []).some(e => relationAllowed(e.relation, filters));
  const fact = packet.why.some(f => relationAllowed(f.claim, filters));
  return incoming || outgoing || fact;
}

function addNode(
  nodes: Map<string, EvidenceSubject>,
  subject: EvidenceSubject
): void {
  nodes.set(subject.id, subject);
}

function addFact(
  facts: Map<string, EvidenceFact>,
  fact: EvidenceFact,
  filters: GraphFilters
): void {
  if (relationAllowed(fact.claim, filters)) facts.set(fact.id, fact);
}

function addEdge(
  nodes: Map<string, EvidenceSubject>,
  edges: Map<string, EvidenceEdge>,
  edge: EvidenceEdge,
  filters: GraphFilters
): void {
  if (!relationAllowed(edge.relation, filters)) return;
  addNode(nodes, edge.from);
  addNode(nodes, edge.to);
  edges.set(edge.id, edge);
}

function missingProofKey(proof: MissingProof): string {
  const line = proof.location?.range?.start.line;
  return [
    proof.kind,
    proof.severity,
    proof.location?.uri ?? '',
    line === undefined ? '' : String(line),
  ].join(':');
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

export function nativeGraphSummary(
  facts: Awaited<ReturnType<typeof analyzeResearchFlow>>['graphFacts']
): Record<string, number> {
  return {
    files: facts.length,
    declarations: facts.reduce(
      (total, file) => total + file.declarations.length,
      0
    ),
    imports: facts.reduce((total, file) => total + file.imports.length, 0),
    exports: facts.reduce((total, file) => total + file.exports.length, 0),
    calls: facts.reduce((total, file) => total + file.calls.length, 0),
    edges: facts.reduce((total, file) => total + file.edges.length, 0),
  };
}

export function summarizePacketGraph(
  packets: readonly ResearchEvidencePacket[]
): ResearchGraphSummary {
  const byVerdict: ResearchGraphSummary['byVerdict'] = {
    reachable: 0,
    'candidate-dead': 0,
    'transitive-dead': 0,
    'candidate-unused-file': 0,
    'candidate-unused-dependency': 0,
    unknown: 0,
  };
  let facts = 0;
  let edges = 0;
  for (const packet of packets) {
    byVerdict[packet.verdict] += 1;
    facts += packet.why.length;
    edges += packet.retainedBy.length + (packet.retains?.length ?? 0);
  }
  return {
    subjects: packets.length,
    facts,
    edges,
    byVerdict,
  };
}

const NATIVE_EDGE_RELATIONS = new Set<EvidenceRelation>([
  'contains',
  'defines',
  'exports',
  'imports',
  'references',
  'calls',
  'constructs',
  'extends',
  'implements',
  'typeUses',
]);

function addNativeGraphEdges(
  root: string,
  graphFacts: Awaited<ReturnType<typeof analyzeResearchFlow>>['graphFacts'],
  visibleNodeIds: ReadonlySet<string>,
  nodes: Map<string, EvidenceSubject>,
  edges: Map<string, EvidenceEdge>,
  filters: GraphFilters
): void {
  if (visibleNodeIds.size === 0) return;
  for (const fileFacts of graphFacts) {
    for (const edge of fileFacts.edges) {
      const relation = nativeEdgeRelation(edge.relation);
      if (!relationAllowed(relation, filters)) continue;
      const from = nativeEndpointSubject(edge.from, root, edge.line);
      const to = nativeEndpointSubject(edge.to, root, edge.line);
      if (!visibleNodeIds.has(from.id) && !visibleNodeIds.has(to.id)) continue;
      addEdge(
        nodes,
        edges,
        {
          id: `ast:${from.id}->${to.id}:${relation}:${edge.line}`,
          from,
          to,
          relation,
          source: 'ast',
          confidence: 'exact',
          via: {
            uri: fileFacts.file,
            range: { start: { line: edge.line } },
          },
        },
        filters
      );
    }
  }
}

function nativeEdgeRelation(relation: string): EvidenceRelation {
  const normalized = relation.trim();
  if (NATIVE_EDGE_RELATIONS.has(normalized as EvidenceRelation)) {
    return normalized as EvidenceRelation;
  }
  return 'references';
}

function nativeEndpointSubject(
  endpoint: string,
  root: string,
  line: number
): EvidenceSubject {
  const symbol = parseNativeSymbolEndpoint(endpoint, root);
  if (symbol) {
    return {
      id: `sym:${symbol.uri}#${symbol.name}`,
      kind: 'symbol',
      name: symbol.name,
      uri: symbol.uri,
      range: { start: { line } },
    };
  }
  return {
    id: `ast:${endpoint}`,
    kind: 'symbol',
    name: endpoint,
    uri: endpoint,
    range: { start: { line } },
  };
}

function parseNativeSymbolEndpoint(
  endpoint: string,
  root: string
): { uri: string; name: string } | undefined {
  if (!endpoint.startsWith('symbol:')) return undefined;
  const raw = endpoint.slice('symbol:'.length);
  const hash = raw.lastIndexOf('#');
  if (hash < 1 || hash === raw.length - 1) return undefined;
  const file = raw.slice(0, hash);
  const name = raw.slice(hash + 1);
  return {
    uri: nodePath.isAbsolute(file) ? nodePath.relative(root, file) : file,
    name,
  };
}
