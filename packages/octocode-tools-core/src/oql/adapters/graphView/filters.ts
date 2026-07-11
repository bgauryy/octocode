/**
 * Graph-view filter logic: parsing `GraphFilters` from raw query params,
 * matching packets/subjects/relations against them, and the node/fact/edge
 * accumulator helpers shared by the graph builder and the native-edge
 * overlay (nativeEdges.ts).
 */
import type {
  EvidenceEdge,
  EvidenceFact,
  EvidenceSubject,
  MissingProof,
  ResearchEvidencePacket,
} from '../../research/packets.js';

export type GraphDirection = 'incoming' | 'outgoing' | 'both';

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

export function relationAllowed(
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

export function addNode(
  nodes: Map<string, EvidenceSubject>,
  subject: EvidenceSubject
): void {
  nodes.set(subject.id, subject);
}

export function addFact(
  facts: Map<string, EvidenceFact>,
  fact: EvidenceFact,
  filters: GraphFilters
): void {
  if (relationAllowed(fact.claim, filters)) facts.set(fact.id, fact);
}

export function addEdge(
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

export function missingProofKey(proof: MissingProof): string {
  const line = proof.location?.range?.start.line;
  return [
    proof.kind,
    proof.severity,
    proof.location?.uri ?? '',
    line === undefined ? '' : String(line),
  ].join(':');
}
