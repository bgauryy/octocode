/**
 * Research-target adapters: semantics (LSP), repositories, packages,
 * pullRequests, commits, artifacts, diff, and smart research packets.
 *
 * Each compiles a canonical OQL query (from + scope + `params` bag) into the
 * existing bulk tool runner and maps the single query's `data` payload into
 * generic record rows. Remote semantics route through materialization first
 * (clone → local LSP). This keeps the planner/dispatch uniform; per-target
 * specifics live behind one `params` bag validated by the backing tool.
 */
import { statSync } from 'node:fs';
import nodePath from 'node:path';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { runDirect } from './runner.js';
import { toOqlPagination, type ToolPaginationPayload } from './pagination.js';
import { diagnostic } from '../diagnostics.js';
import { classifyDiffLane } from '../diffLanes.js';
import { toGithubRepositoryLanguage } from '../transformers/language.js';
import { analyzeResearchFlow } from '../research/analyze.js';
import {
  buildResearchPackets,
  type EvidenceEdge,
  type EvidenceFact,
  type EvidenceRelation,
  type EvidenceSubject,
  type MissingProof,
  type ResearchEvidencePacket,
  type ResearchGraphSummary,
} from '../research/packets.js';
import type { AdapterResult } from './local.js';
import type {
  OqlGraphData,
  OqlDiagnostic,
  OqlContinuation,
  Pagination,
  OqlQuery,
  OqlRecordResultRow,
  QuerySource,
} from '../types.js';

/* ------------------------------ helpers --------------------------------- */

function firstQueryData<T = Record<string, unknown>>(
  result: CallToolResult
): { data?: T; status?: string } {
  const sc = result.structuredContent as
    | { results?: Array<{ status?: string; data?: unknown }> }
    | undefined;
  const first = sc?.results?.[0];
  return { data: first?.data as T | undefined, status: first?.status };
}

/**
 * Pull file content/status/error out of a ghGetFileContent (or localGetFileContent)
 * result. The row sits directly under structuredContent.results[0] with the file
 * in files[0] (no nested `.data`); some shapes nest under `.data` or `.results`.
 * Used by the direct two-ref diff lanes — reading `.data.content` is always
 * undefined for this tool and previously masqueraded as "files identical".
 */
function ghFileContentResult(result: CallToolResult): {
  content?: string;
  status?: string;
  error?: unknown;
} {
  const sc = result.structuredContent as
    | { results?: Array<Record<string, unknown>> }
    | undefined;
  const row = sc?.results?.[0];
  if (!row) return {};
  const data = ('data' in row ? row.data : row) as
    | Record<string, unknown>
    | undefined;
  const fileRow =
    (data?.files as Array<Record<string, unknown>> | undefined)?.[0] ??
    (data?.results as Array<Record<string, unknown>> | undefined)?.[0] ??
    data ??
    {};
  const content = fileRow.content;
  return {
    content: typeof content === 'string' ? content : undefined,
    status: row.status as string | undefined,
    error: fileRow.error ?? data?.error ?? row.error,
  };
}

function errorText(value: unknown, fallback: string): string {
  if (typeof value === 'string' && value.trim()) return value;
  if (value instanceof Error && value.message) return value.message;
  if (value && typeof value === 'object') {
    const record = value as { error?: unknown; message?: unknown };
    if (typeof record.error === 'string' && record.error.trim()) {
      return record.error;
    }
    if (typeof record.message === 'string' && record.message.trim()) {
      return record.message;
    }
    const serialized = JSON.stringify(value);
    if (serialized) return serialized;
  }
  return fallback;
}

function materializedClonePath(
  result: CallToolResult,
  localPath: string | undefined
): string | undefined {
  if (!localPath || nodePath.isAbsolute(localPath)) return localPath;
  const sc = result.structuredContent as { base?: string } | undefined;
  return sc?.base ? nodePath.join(sc.base, localPath) : localPath;
}

function resolveSemanticSourceUri(
  reportedUri: string | undefined,
  fallbackUri: string | undefined,
  workspaceRoot: string | undefined
): string {
  const fallback = fallbackUri ?? workspaceRoot ?? '.';
  if (!reportedUri) return fallback;
  if (nodePath.isAbsolute(reportedUri)) return reportedUri;

  if (workspaceRoot && nodePath.isAbsolute(workspaceRoot)) {
    return nodePath.resolve(workspaceRoot, reportedUri);
  }

  if (fallbackUri && nodePath.isAbsolute(fallbackUri)) {
    const base = isExistingDirectory(fallbackUri)
      ? fallbackUri
      : nodePath.dirname(fallbackUri);
    return nodePath.resolve(base, reportedUri);
  }

  return reportedUri;
}

function firstScopePath(query: OqlQuery): string | undefined {
  const path = query.scope?.path;
  return Array.isArray(path) ? path[0] : path;
}

/** Known array-valued payload fields, in priority order. */
const RECORD_ARRAY_KEYS = [
  'repositories',
  'pull_requests',
  'commits',
  'packages',
  'results',
  'locations',
  'references',
  'symbols',
  'strings',
  'entries',
  'incomingCalls',
  'outgoingCalls',
];

const RECORD_PARENT_METADATA_EXCLUDE = new Set([
  ...RECORD_ARRAY_KEYS,
  'pagination',
  'contentPagination',
  'next',
]);

/** Expand a tool `data` payload into row items (an inner array if present). */
function expandData(data: Record<string, unknown> | undefined): unknown[] {
  if (!data) return [];
  for (const key of RECORD_ARRAY_KEYS) {
    const v = (data as Record<string, unknown>)[key];
    if (Array.isArray(v)) return v;
  }
  return [data];
}

function parentMetadata(
  data: Record<string, unknown> | undefined
): Record<string, unknown> | undefined {
  if (!data) return undefined;
  const metadata: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(data)) {
    if (RECORD_PARENT_METADATA_EXCLUDE.has(key)) continue;
    if (
      value === null ||
      typeof value === 'string' ||
      typeof value === 'number' ||
      typeof value === 'boolean'
    ) {
      metadata[key] = value;
    }
  }
  return Object.keys(metadata).length ? metadata : undefined;
}

function records(
  items: unknown[],
  recordType: OqlRecordResultRow['recordType'],
  source?: QuerySource,
  metadata?: Record<string, unknown>
): OqlRecordResultRow[] {
  return items.map(item => {
    const data = (
      item && typeof item === 'object'
        ? (item as Record<string, unknown>)
        : { value: item }
    ) as Record<string, unknown>;
    const id = stableId(recordType, data);
    return {
      kind: 'record' as const,
      recordType,
      ...(id ? { id } : {}),
      ...(source ? { source } : {}),
      ...(metadata ? { metadata } : {}),
      data,
    };
  });
}

const DEFAULT_RESEARCH_PACKET_PAGE_SIZE = 25;

function requestedResearchMode(mode: unknown): 'plan' | 'analyze' | 'prove' {
  if (mode === 'plan' || mode === 'prove') return mode;
  return 'analyze';
}

function packetPage(
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

/** Domains the `view:"detailed"` research record can expand. */
const DETAILED_DOMAINS = [
  'manifests',
  'files',
  'dependencies',
  'symbols',
  'graphFacts',
] as const;
type DetailedDomain = (typeof DETAILED_DOMAINS)[number];

/**
 * Which detailed domains the agent asked for via `select`. Accepts both the bare
 * form (`select:["symbols"]`) and the dotted record-data form
 * (`select:["data.symbols"]`). Returns `undefined` when no domain selector is
 * present (→ include all domains).
 */
function requestedDetailedDomains(
  select: string[] | undefined
): ReadonlySet<DetailedDomain> | undefined {
  if (!select || select.length === 0) return undefined;
  const requested = new Set<DetailedDomain>();
  for (const raw of select) {
    const token = raw.trim();
    const bare = token.startsWith('data.') ? token.slice(5) : token;
    if ((DETAILED_DOMAINS as readonly string[]).includes(bare)) {
      requested.add(bare as DetailedDomain);
    }
  }
  return requested.size > 0 ? requested : undefined;
}

/**
 * Build the `view:"detailed"` payload as per-domain *windows* instead of whole
 * arrays (P1). Each requested domain emits a sliced `data.<domain>` window plus
 * a typed `data.<domain>Page` pagination object, all sharing the query's
 * page/itemsPerPage. Returns the combined pagination (max totalPages, OR-ed
 * hasMore) so a single `next.page` advances every detailed domain together.
 */
function buildDetailedDomains(
  query: OqlQuery,
  data: Awaited<ReturnType<typeof analyzeResearchFlow>>
): { fields: Record<string, unknown>; pagination?: Pagination } {
  const requested = requestedDetailedDomains(query.select);
  const arrays: Record<DetailedDomain, readonly unknown[]> = {
    manifests: data.manifests,
    files: data.files,
    dependencies: data.dependencies,
    symbols: data.symbols,
    graphFacts: data.graphFacts,
  };

  const fields: Record<string, unknown> = {};
  const currentPage = Math.max(1, query.page ?? 1);
  let itemsPerPage: number | undefined;
  let maxTotalPages = 1;
  let anyMore = false;
  for (const domain of DETAILED_DOMAINS) {
    if (requested && !requested.has(domain)) continue;
    const items = arrays[domain] ?? [];
    const { packetsStart, packetsEnd, pagination } = packetPage(
      query,
      items.length
    );
    fields[domain] = items.slice(packetsStart, packetsEnd);
    fields[`${domain}Page`] = pagination;
    itemsPerPage = pagination.itemsPerPage;
    maxTotalPages = Math.max(maxTotalPages, pagination.totalPages ?? 1);
    if (pagination.hasMore) anyMore = true;
  }

  if (Object.keys(fields).length === 0) return { fields };
  return {
    fields,
    pagination: {
      currentPage,
      ...(itemsPerPage !== undefined ? { itemsPerPage } : {}),
      totalPages: maxTotalPages,
      hasMore: anyMore || currentPage < maxTotalPages,
    },
  };
}

/** Combine the packet-page window with the detailed-domain window so the
 *  envelope's `hasMore` (and thus `next.page`) reflects either having more. */
function combinePagination(
  a: Pagination | undefined,
  b: Pagination | undefined
): Pagination | undefined {
  if (!a) return b;
  if (!b) return a;
  const currentPage = a.currentPage ?? b.currentPage;
  const itemsPerPage = a.itemsPerPage ?? b.itemsPerPage;
  return {
    ...(currentPage !== undefined ? { currentPage } : {}),
    ...(itemsPerPage !== undefined ? { itemsPerPage } : {}),
    totalPages: Math.max(a.totalPages ?? 1, b.totalPages ?? 1),
    ...(a.totalItems !== undefined ? { totalItems: a.totalItems } : {}),
    hasMore: Boolean(a.hasMore || b.hasMore),
  };
}

type GraphDirection = 'incoming' | 'outgoing' | 'both';

interface GraphFilters {
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

function graphFilters(p: Record<string, unknown>): GraphFilters {
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

function packetMatchesGraphFilters(
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

function buildGraphView(
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

function nativeGraphSummary(
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

function summarizePacketGraph(
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

function shouldRunLspProof(
  mode: 'plan' | 'analyze' | 'prove',
  p: Record<string, unknown>
): boolean {
  if (mode === 'plan') return false;
  if (p.proof === 'none') return false;
  return p.proof === 'lsp' || mode === 'prove';
}

function graphProofLimit(query: OqlQuery, p: Record<string, unknown>): number {
  if (typeof p.proofLimit === 'number') return Math.min(25, p.proofLimit);
  const pageSize = query.itemsPerPage ?? query.limit ?? 5;
  return Math.max(1, Math.min(5, pageSize));
}

async function escalateGraphPacketsWithLsp(
  root: string,
  query: OqlQuery,
  packets: ResearchEvidencePacket[],
  filters: GraphFilters,
  limit: number
): Promise<OqlDiagnostic[]> {
  const filteredPackets = packets.filter(packet =>
    packetMatchesGraphFilters(packet, filters)
  );
  const pageWindow = packetPage(query, filteredPackets.length);
  const pagePackets = filteredPackets
    .slice(pageWindow.packetsStart, pageWindow.packetsEnd)
    .filter(packet => packet.subject.kind === 'symbol')
    .slice(0, limit);

  const diagnostics: OqlDiagnostic[] = [];
  for (const packet of pagePackets) {
    const proof = await proveSymbolPacketWithLsp(root, packet);
    packet.proof = { ...(packet.proof ?? {}), lsp: proof };

    if (proof.status === 'unavailable' || proof.status === 'error') {
      diagnostics.push(
        diagnostic(
          proof.status === 'unavailable' ? 'lspUnavailable' : 'partialResult',
          proof.message ?? 'LSP proof escalation did not complete.',
          { backend: 'lspGetSemantics', severity: 'warning' }
        )
      );
      continue;
    }

    if (typeof proof.totalReferences !== 'number') {
      diagnostics.push(
        diagnostic(
          'partialResult',
          'LSP proof escalation returned without a numeric reference count.',
          { backend: 'lspGetSemantics', blocksAnswer: true }
        )
      );
      continue;
    }

    packet.missingProof = packet.missingProof.filter(
      item => item.kind !== 'lsp-unavailable'
    );
    attachLspReferenceEdges(packet, proof);
    if (proof.paginationOpen) {
      packet.missingProof.push({
        kind: 'pagination-open',
        severity: 'high',
        location: packet.subject,
      });
      diagnostics.push(
        diagnostic(
          'partialResult',
          'LSP proof result is paginated; follow the semantic continuation before deletion.',
          { backend: 'lspGetSemantics', blocksAnswer: true }
        )
      );
    }

    if (proof.totalReferences === 0) {
      packet.proofStatus = 'confirmed-by-lsp';
      packet.risk = {
        deleteRisk: packet.verdict === 'reachable' ? 'high' : 'medium',
        reason:
          'LSP references found zero non-declaration references for this symbol. Still verify dynamic/framework retention before deleting.',
      };
    } else if (typeof proof.totalReferences === 'number') {
      packet.proofStatus =
        packet.verdict === 'reachable'
          ? 'confirmed-by-lsp'
          : 'conflicting-evidence';
      packet.risk = {
        deleteRisk: 'high',
        reason:
          'LSP found non-declaration references. Inspect proof.lsp.files and next.fetch before deleting.',
      };
    }
  }
  return diagnostics;
}

function attachLspReferenceEdges(
  packet: ResearchEvidencePacket,
  proof: LspPacketProof
): void {
  if (
    proof.status !== 'ok' ||
    !proof.totalReferences ||
    proof.files.length === 0
  ) {
    return;
  }
  const existing = new Set(packet.retainedBy.map(edge => edge.id));
  for (const [i, file] of proof.files.entries()) {
    const from: EvidenceSubject = {
      id: `file:${file}`,
      kind: 'file',
      uri: file,
    };
    const edge: EvidenceEdge = {
      id: `${packet.subject.id}:lsp-ref:${i}`,
      from,
      to: packet.subject,
      relation: 'references',
      source: 'lsp',
      confidence: 'exact',
      flags: file === packet.subject.uri ? ['same-file'] : ['external'],
    };
    if (!existing.has(edge.id)) {
      packet.retainedBy.push(edge);
      existing.add(edge.id);
    }
  }
}

type LspPacketProof = {
  readonly status: 'ok' | 'unavailable' | 'error';
  readonly totalReferences?: number;
  readonly files: readonly string[];
  readonly paginationOpen: boolean;
  readonly message?: string;
};

async function proveSymbolPacketWithLsp(
  root: string,
  packet: ResearchEvidencePacket
): Promise<LspPacketProof> {
  const symbolName = packet.subject.name;
  const lineHint = packet.subject.range?.start.line;
  if (!symbolName || typeof lineHint !== 'number') {
    return {
      status: 'error',
      files: [],
      paginationOpen: false,
      message: 'Symbol packet has no name or line hint for LSP proof.',
    };
  }

  const uri = nodePath.isAbsolute(packet.subject.uri)
    ? packet.subject.uri
    : nodePath.resolve(root, packet.subject.uri);
  try {
    const result = await runDirect('lspGetSemantics', {
      type: 'references',
      uri,
      symbolName,
      lineHint,
      includeDeclaration: false,
      groupByFile: true,
      itemsPerPage: 50,
    });
    const directError = directToolError(result);
    if (directError) {
      return {
        status:
          directError.code === 'localToolsDisabled' ? 'unavailable' : 'error',
        files: [],
        paginationOpen: false,
        message: directError.message,
      };
    }
    const { data, status } = firstQueryData<Record<string, unknown>>(result);
    if (status === 'error') {
      return {
        status: 'error',
        files: [],
        paginationOpen: false,
        message: stringFrom(data?.error) ?? 'lspGetSemantics returned error.',
      };
    }
    const lsp = data?.lsp as
      | { serverAvailable?: boolean; source?: string }
      | undefined;
    if (lsp?.serverAvailable === false) {
      return {
        status: 'unavailable',
        files: [],
        paginationOpen: false,
        message:
          lsp.source === 'native'
            ? 'Language server unavailable; native fallback cannot prove cross-file references.'
            : 'Language server unavailable; reference proof is incomplete.',
      };
    }

    const payload =
      data?.payload && typeof data.payload === 'object'
        ? (data.payload as Record<string, unknown>)
        : undefined;
    const pagination = data?.pagination as
      | { hasMore?: boolean; totalItems?: number }
      | undefined;
    const totalReferences =
      numberFrom(data?.totalReferences) ??
      numberFrom(payload?.totalReferences) ??
      numberFrom(data?.referenceCount) ??
      numberFrom(payload?.referenceCount) ??
      numberFrom(pagination?.totalItems) ??
      countReferenceLikeItems(payload) ??
      countReferenceLikeItems(data);
    return {
      status: 'ok',
      ...(typeof totalReferences === 'number' ? { totalReferences } : {}),
      files: referenceFiles(data, root),
      paginationOpen: pagination?.hasMore === true,
    };
  } catch (err) {
    return {
      status: 'error',
      files: [],
      paginationOpen: false,
      message: err instanceof Error ? err.message : 'Could not run LSP proof.',
    };
  }
}

function directToolError(
  result: CallToolResult
): { code?: string; message: string } | undefined {
  const sc = result.structuredContent;
  if (!sc || typeof sc !== 'object') return undefined;
  const record = sc as Record<string, unknown>;
  if (record.status !== 'error') return undefined;
  const error =
    record.error && typeof record.error === 'object'
      ? (record.error as Record<string, unknown>)
      : undefined;
  return {
    ...(typeof error?.code === 'string' ? { code: error.code } : {}),
    message:
      (typeof error?.message === 'string' && error.message) ||
      (typeof record.code === 'string' && record.code) ||
      'Direct tool call failed.',
  };
}

function countReferenceLikeItems(
  data: Record<string, unknown> | undefined
): number | undefined {
  if (!data) return undefined;
  for (const key of ['references', 'locations', 'results', 'byFile']) {
    const value = data[key];
    if (!Array.isArray(value)) continue;
    if (key !== 'byFile') return value.length;
    return value.reduce((total, item) => {
      if (!item || typeof item !== 'object') return total + 1;
      const count = (item as Record<string, unknown>).count;
      return total + (typeof count === 'number' ? count : 1);
    }, 0);
  }
  return undefined;
}

function referenceFiles(
  data: Record<string, unknown> | undefined,
  root: string
): readonly string[] {
  const out = new Set<string>();
  collectReferenceFiles(data, out, root);
  return [...out].slice(0, 25);
}

function collectReferenceFiles(
  value: unknown,
  out: Set<string>,
  root: string
): void {
  if (out.size >= 25 || value === null || value === undefined) return;
  if (Array.isArray(value)) {
    for (const item of value) collectReferenceFiles(item, out, root);
    return;
  }
  if (typeof value !== 'object') return;
  const record = value as Record<string, unknown>;
  for (const key of ['uri', 'file', 'path']) {
    const maybeFile = record[key];
    if (typeof maybeFile === 'string' && looksLikePath(maybeFile)) {
      out.add(
        nodePath.isAbsolute(maybeFile)
          ? nodePath.relative(root, maybeFile)
          : maybeFile
      );
    }
  }
  for (const key of [
    'references',
    'locations',
    'byFile',
    'results',
    'files',
    'groups',
    'items',
  ]) {
    collectReferenceFiles(record[key], out, root);
  }
}

function looksLikePath(value: string): boolean {
  return (
    value.includes('/') || value.includes('\\') || /\.[cm]?[jt]sx?$/.test(value)
  );
}

function numberFrom(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : undefined;
}

function stringFrom(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Citeable identity per record type, extracted from the backend payload. */
function stableId(
  recordType: OqlRecordResultRow['recordType'],
  d: Record<string, unknown>
): string | undefined {
  const s = (k: string): string | undefined =>
    typeof d[k] === 'string' || typeof d[k] === 'number'
      ? String(d[k])
      : undefined;
  switch (recordType) {
    case 'repository':
      return (
        s('fullName') ??
        (s('owner') && s('repo') ? `${s('owner')}/${s('repo')}` : s('url'))
      );
    case 'package': {
      const name = s('name') ?? s('packageName');
      const ver = s('version');
      return name ? (ver ? `${name}@${ver}` : name) : undefined;
    }
    case 'pullRequest':
      return s('number') ? `#${s('number')}` : s('url');
    case 'commit':
      return s('sha')?.slice(0, 12) ?? s('oid')?.slice(0, 12);
    case 'artifact':
      return s('localPath') ?? s('path');
    case 'materialized':
      return s('localPath') ?? s('repoRoot');
    case 'diff':
      return s('path') ?? s('filename');
    case 'semantics': {
      const uri = s('uri');
      const line = s('line') ?? s('startLine');
      return uri ? (line ? `${uri}:${line}` : uri) : undefined;
    }
    case 'research':
      return s('intent') ?? s('goal') ?? 'research';
    case 'graph':
      return s('intent') ? `graph:${s('intent')}` : 'graph';
  }
  return undefined;
}

function statusDiagnostics(
  result: CallToolResult,
  backend: string
): OqlDiagnostic[] {
  const { status, data } = firstQueryData<{ error?: unknown }>(result);
  if (status === 'error') {
    return [
      diagnostic('invalidQuery', errorText(data?.error, `${backend} failed`), {
        backend,
      }),
    ];
  }
  if (status === 'empty') {
    return [
      diagnostic('zeroMatches', 'Query ran and matched nothing.', {
        backend,
        severity: 'info',
        blocksAnswer: false,
      }),
    ];
  }
  return [];
}

function isExistingDirectory(path: string): boolean {
  try {
    const resolved = nodePath.isAbsolute(path) ? path : nodePath.resolve(path);
    return statSync(resolved).isDirectory();
  } catch {
    return false;
  }
}

function splitRepo(source: QuerySource | undefined): {
  owner?: string;
  repo?: string;
} {
  if (!source || source.kind !== 'github') return {};
  if (source.repo && source.repo.includes('/')) {
    const [owner, repo] = source.repo.split('/');
    return { owner, repo };
  }
  return { owner: source.owner };
}

function params(query: OqlQuery): Record<string, unknown> {
  return query.params ?? {};
}

function firstScopeLanguage(query: OqlQuery): string | undefined {
  const lang = query.scope?.language;
  if (!lang) return undefined;
  return Array.isArray(lang) ? lang[0] : lang;
}

function withOqlPaging(
  query: OqlQuery,
  limitKey?: 'limit' | 'perPage'
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...params(query) };
  if (out.page === undefined && query.page !== undefined) {
    out.page = query.page;
  }
  if (limitKey && out[limitKey] === undefined) {
    const limit = query.limit ?? query.itemsPerPage;
    if (limit !== undefined) out[limitKey] = limit;
  }
  return out;
}

/**
 * Build an AdapterResult from a backing-tool result: map records (none on
 * error), carry status diagnostics, and emit `zeroMatches` on a clean empty so
 * an empty result is never read as silent proof.
 */
function finishRecords(
  result: CallToolResult,
  recordType: OqlRecordResultRow['recordType'],
  backend: string,
  source?: QuerySource
): AdapterResult {
  const { data, status } = firstQueryData(result);
  const diagnostics = statusDiagnostics(result, backend);
  const items = status === 'error' ? [] : expandData(data);
  if (
    items.length === 0 &&
    !diagnostics.some(d => d.code === 'zeroMatches' || d.severity === 'error')
  ) {
    diagnostics.push(
      diagnostic('zeroMatches', `${backend} returned no results.`, {
        backend,
        severity: 'info',
        blocksAnswer: false,
      })
    );
  }
  // Promote the backing tool's pagination into the OQL envelope so run.ts can
  // emit a first-class next.page (instead of leaking raw data.next).
  const pag = (data as { pagination?: ToolPaginationPayload })?.pagination;
  const hasMore =
    pag?.hasMore === true ||
    Boolean((data as { next?: unknown })?.next) ||
    (typeof pag?.currentPage === 'number' &&
      typeof pag?.totalPages === 'number' &&
      pag.currentPage < pag.totalPages);
  const pagination = toOqlPagination(pag, hasMore);
  return {
    results: records(items, recordType, source, parentMetadata(data)),
    ...(pagination ? { pagination } : {}),
    diagnostics,
    provenance: [{ backend, source }],
  };
}

function semanticDiagnostics(
  data: Record<string, unknown> | undefined,
  query: OqlQuery
): OqlDiagnostic[] {
  const diagnostics: OqlDiagnostic[] = [];
  const lsp = data?.lsp as
    | { serverAvailable?: boolean; source?: string }
    | undefined;
  if (lsp?.serverAvailable === false) {
    diagnostics.push(
      diagnostic(
        'lspUnavailable',
        lsp.source === 'native'
          ? 'Language server was unavailable; native fallback returned partial semantic data.'
          : 'Language server was unavailable; semantic proof is incomplete.',
        { backend: 'lspGetSemantics' }
      )
    );
  }

  const pag = data?.pagination as
    | {
        hasMore?: boolean;
        currentPage?: number;
        nextPage?: number;
        totalPages?: number;
      }
    | undefined;
  if (pag?.hasMore) {
    diagnostics.push(
      diagnostic(
        'partialResult',
        'Semantic result is paginated; follow the continuation before treating it as complete proof.',
        {
          backend: 'lspGetSemantics',
          blocksAnswer: true,
          continuation: semanticPageContinuation(pag, query),
        }
      )
    );
  }
  return diagnostics;
}

function semanticPageContinuation(
  pag: {
    currentPage?: number;
    nextPage?: number;
  },
  query: OqlQuery
) {
  const nextPage =
    typeof pag.nextPage === 'number'
      ? pag.nextPage
      : typeof pag.currentPage === 'number'
        ? pag.currentPage + 1
        : (query.page ?? 1) + 1;
  return {
    query: {
      ...query,
      params: { ...(query.params ?? {}), page: nextPage },
    },
    why: 'Continue the LSP semantic result page.',
    confidence: 'exact' as const,
  };
}

function semanticPagination(
  data: Record<string, unknown> | undefined,
  query: OqlQuery
): Pagination | undefined {
  const pag = data?.pagination as
    | {
        hasMore?: boolean;
        currentPage?: number;
        nextPage?: number;
        totalPages?: number;
        itemsPerPage?: number;
        totalItems?: number;
      }
    | undefined;
  if (!pag?.hasMore) return undefined;
  return {
    hasMore: true,
    ...(pag.currentPage !== undefined ? { currentPage: pag.currentPage } : {}),
    ...(pag.totalPages !== undefined ? { totalPages: pag.totalPages } : {}),
    ...(pag.itemsPerPage !== undefined
      ? { itemsPerPage: pag.itemsPerPage }
      : {}),
    ...(pag.totalItems !== undefined ? { totalItems: pag.totalItems } : {}),
    next: semanticPageContinuation(pag, query),
  };
}

/* --------------------------- target adapters ---------------------------- */

export async function executeRepositories(
  query: OqlQuery
): Promise<AdapterResult> {
  const { owner } = splitRepo(query.from);
  const forwarded = withOqlPaging(query, 'limit');
  const rawLanguage =
    typeof forwarded.language === 'string'
      ? forwarded.language
      : firstScopeLanguage(query);
  const language = toGithubRepositoryLanguage(rawLanguage);
  if (language) forwarded.language = language;
  const result = await runDirect('ghSearchRepos', {
    ...(owner ? { owner } : {}),
    ...forwarded,
  });
  return finishRecords(
    result,
    'repository',
    'ghSearchRepos',
    query.from ?? { kind: 'github' }
  );
}

export async function executePackages(query: OqlQuery): Promise<AdapterResult> {
  const result = await runDirect('npmSearch', { ...withOqlPaging(query) });
  return finishRecords(
    result,
    'package',
    'npmSearch',
    query.from ?? { kind: 'npm' }
  );
}

export async function executeHistory(query: OqlQuery): Promise<AdapterResult> {
  const { owner, repo } = splitRepo(query.from);
  const commits = query.target === 'commits';

  // P4: `matchString` is an OQL-layer *content* filter applied to fetched
  // bodies — never a backing search-index claim. Strip it (and matchScope) from
  // the params forwarded to ghHistoryResearch for BOTH lanes so the tool is not
  // asked to interpret it as a query field, then apply it client-side with
  // honest partial/zero-match diagnostics. (Commits previously forwarded
  // matchString raw and never filtered — a silent drop if the backend ignored
  // it; PRs and commits now share the same content-filter discipline.)
  const pr = !commits ? pullRequestMatch(query) : undefined;
  const commitNeedle = commits ? commitMatchNeedle(query) : undefined;
  const forwarded = withOqlPaging(query, commits ? 'perPage' : 'limit');
  if (pr || commitNeedle) {
    delete forwarded.matchString;
    delete forwarded.matchScope;
  }

  const result = await runDirect('ghHistoryResearch', {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    ...(commits ? { type: 'commits' } : {}),
    ...forwarded,
  });
  const mapped = finishRecords(
    result,
    commits ? 'commit' : 'pullRequest',
    'ghHistoryResearch',
    query.from ?? { kind: 'github' }
  );
  if (pr) return filterPullRequestsByMatch(mapped, pr);
  if (commitNeedle) return filterCommitsByMatch(mapped, commitNeedle);
  return mapped;
}

/** Read the validated commit content-match needle, if present. */
function commitMatchNeedle(query: OqlQuery): string | undefined {
  const p = params(query);
  return typeof p.matchString === 'string' && p.matchString.length > 0
    ? p.matchString
    : undefined;
}

/**
 * Keep only commit records whose message contains `needle` (case-insensitive
 * substring), spotlight where it matched, and surface honest diagnostics — a
 * `partialResult` when some were dropped, `zeroMatches` when none matched.
 * Mirrors {@link filterPullRequestsByMatch}; commit text is the commit message.
 */
export function filterCommitsByMatch(
  result: AdapterResult,
  needle: string
): AdapterResult {
  const needleLower = needle.toLowerCase();
  const total = result.results.length;
  const kept = result.results.filter(row => {
    if (row.kind !== 'record') return false;
    const data = (row as OqlRecordResultRow).data;
    const messageVal = (data as Record<string, unknown>).message;
    const haystack = typeof messageVal === 'string' ? messageVal : '';
    const idx = haystack.toLowerCase().indexOf(needleLower);
    if (idx < 0) return false;
    const start = Math.max(0, idx - 80);
    const end = Math.min(haystack.length, idx + needle.length + 80);
    (data as Record<string, unknown>).match = {
      matchString: needle,
      scope: 'message',
      spotlight:
        (start > 0 ? '…' : '') +
        haystack.slice(start, end) +
        (end < haystack.length ? '…' : ''),
    };
    return true;
  });

  const diagnostics = result.diagnostics.filter(d => d.code !== 'zeroMatches');
  if (kept.length === 0) {
    diagnostics.push(
      diagnostic(
        'zeroMatches',
        `No commit message matched "${needle}" (content filter over ${total} fetched commit(s); not a search-index query). Broaden the fetch (branch/perPage/page).`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  } else if (kept.length < total) {
    diagnostics.push(
      diagnostic(
        'partialResult',
        `Content filter kept ${kept.length} of ${total} fetched commit(s) matching "${needle}" in message. This filters fetched content only — page the fetch to widen the candidate set.`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  }

  return { ...result, results: kept, diagnostics };
}

export interface PullRequestMatch {
  needle: string;
  scope: 'body' | 'title' | 'comments' | 'reviews' | 'all';
}

/** Read the validated PR content-match params, if present. */
function pullRequestMatch(query: OqlQuery): PullRequestMatch | undefined {
  const p = params(query);
  const needle = typeof p.matchString === 'string' ? p.matchString : undefined;
  if (!needle) return undefined;
  const scope =
    p.matchScope === 'title' ||
    p.matchScope === 'comments' ||
    p.matchScope === 'reviews' ||
    p.matchScope === 'all'
      ? p.matchScope
      : 'body';
  return { needle, scope };
}

/** Collect the searchable text for a PR record under the requested scope. */
function pullRequestScopeText(
  data: Record<string, unknown>,
  scope: PullRequestMatch['scope']
): string {
  const parts: string[] = [];
  const push = (v: unknown) => {
    if (typeof v === 'string' && v.length > 0) parts.push(v);
  };
  const bodies = (key: string) => {
    const list = data[key];
    if (Array.isArray(list)) {
      for (const c of list) {
        if (c && typeof c === 'object') push((c as { body?: unknown }).body);
      }
    }
  };
  if (scope === 'body' || scope === 'all') push(data.body);
  if (scope === 'title' || scope === 'all') push(data.title);
  if (scope === 'comments' || scope === 'all') bodies('comments');
  if (scope === 'reviews' || scope === 'all') bodies('reviews');
  return parts.join('\n');
}

/**
 * Keep only PR records whose scope text contains `matchString` (case-insensitive
 * substring), spotlight where each matched, and surface honest diagnostics: a
 * `partialResult` when some were dropped, `zeroMatches` when none matched.
 */
export function filterPullRequestsByMatch(
  result: AdapterResult,
  match: PullRequestMatch
): AdapterResult {
  const needleLower = match.needle.toLowerCase();
  const total = result.results.length;
  const kept = result.results.filter(row => {
    if (row.kind !== 'record') return false;
    const data = (row as OqlRecordResultRow).data;
    const haystack = pullRequestScopeText(data, match.scope);
    const idx = haystack.toLowerCase().indexOf(needleLower);
    if (idx < 0) return false;
    // Additive spotlight: a bounded window around the first hit (full body/
    // comment text is left intact on the record).
    const start = Math.max(0, idx - 80);
    const end = Math.min(haystack.length, idx + match.needle.length + 80);
    (data as Record<string, unknown>).match = {
      matchString: match.needle,
      scope: match.scope,
      spotlight:
        (start > 0 ? '…' : '') +
        haystack.slice(start, end) +
        (end < haystack.length ? '…' : ''),
    };
    return true;
  });

  const diagnostics = result.diagnostics.filter(d => d.code !== 'zeroMatches');
  if (kept.length === 0) {
    diagnostics.push(
      diagnostic(
        'zeroMatches',
        `No pull request ${match.scope} matched "${match.needle}" (content filter over ${total} fetched PR(s); not a search-index query). Broaden the fetch (state/keywordsToSearch/page) or the match scope.`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  } else if (kept.length < total) {
    diagnostics.push(
      diagnostic(
        'partialResult',
        `Content filter kept ${kept.length} of ${total} fetched PR(s) matching "${match.needle}" in ${match.scope}. This filters fetched content only — page the fetch to widen the candidate set.`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  }

  return { ...result, results: kept, diagnostics };
}

/**
 * `target:"diff"` has two typed lanes, discriminated by params shape:
 *   - PR patch:    { prNumber, files? }            -> ghHistoryResearch patches
 *   - direct file: { baseRef, headRef, path }      -> two ghGetFileContent reads
 *                                                     + a pure local line diff
 * A request that fits neither returns a repair diagnostic rather than silently
 * falling through to a PR-patch call.
 */
export async function executeDiff(query: OqlQuery): Promise<AdapterResult> {
  const p = params(query);
  const { owner, repo } = splitRepo(query.from);
  // Lane discriminant is shared with the planner (diffLanes.ts) — one source of
  // truth, so dry-run plan and execution can never disagree.
  const lane = classifyDiffLane(p);

  if (lane.kind === 'prPatch') {
    // PR patch lane (unchanged behavior).
    const result = await runDirect('ghHistoryResearch', {
      ...(owner ? { owner } : {}),
      ...(repo ? { repo } : {}),
      content: { patches: { mode: 'all' } },
      ...p,
    });
    return finishRecords(
      result,
      'diff',
      'ghHistoryResearch',
      query.from ?? { kind: 'github' }
    );
  }

  if (lane.kind === 'directFile') {
    if (query.from?.kind === 'local' || query.from?.kind === 'materialized') {
      return executeLocalDirectFileDiff(query, {
        baseRef: lane.baseRef,
        headRef: lane.headRef,
        path: lane.path,
      });
    }
    return executeDirectFileDiff(query, owner, repo, {
      baseRef: lane.baseRef,
      headRef: lane.headRef,
      path: lane.path,
    });
  }

  return {
    results: [],
    diagnostics: [
      diagnostic(
        'invalidQuery',
        'target:"diff" needs either {prNumber} (PR patch diff) or {baseRef,headRef,path} (direct file diff between two refs).',
        {
          backend: 'ghHistoryResearch',
          repair: {
            message:
              'Add params.prNumber for a PR patch, or params.baseRef + params.headRef + params.path for a direct file diff.',
          },
        }
      ),
    ],
    provenance: [],
  };
}

/** Direct two local files via two content reads + a pure local line diff. */
async function executeLocalDirectFileDiff(
  query: OqlQuery,
  refs: { baseRef: string; headRef: string; path: string }
): Promise<AdapterResult> {
  const source = query.from;
  const basePath =
    source?.kind === 'local'
      ? source.path
      : source?.kind === 'materialized'
        ? source.localPath
        : undefined;
  if (!basePath) {
    return {
      results: [],
      diagnostics: [
        diagnostic('invalidQuery', 'Local direct file diff needs from.path.', {
          backend: 'localGetFileContent',
        }),
      ],
      provenance: [],
    };
  }

  const read = (path: string) =>
    runDirect('localGetFileContent', {
      path,
      fullContent: true,
      minify: 'none',
    });

  const [baseRes, headRes] = await Promise.all([
    read(basePath),
    read(refs.path),
  ]);
  const base = firstQueryData<{ content?: unknown; error?: unknown }>(baseRes);
  const head = firstQueryData<{ content?: unknown; error?: unknown }>(headRes);
  const baseContent =
    typeof base.data?.content === 'string' ? base.data.content : undefined;
  const headContent =
    typeof head.data?.content === 'string' ? head.data.content : undefined;

  // Guard against a missing read (status error OR absent content) so an
  // unresolved file can't silently diff empty-vs-empty and report "identical".
  if (
    base.status === 'error' ||
    head.status === 'error' ||
    baseContent === undefined ||
    headContent === undefined
  ) {
    const err = errorText(
      base.data?.error ?? head.data?.error,
      'Could not read local file.'
    );
    return {
      results: [],
      diagnostics: [
        diagnostic('invalidQuery', err, { backend: 'localGetFileContent' }),
      ],
      provenance: [{ backend: 'localGetFileContent', source: query.from }],
    };
  }

  const diff = computeLineDiff(baseContent, headContent);
  const row: OqlRecordResultRow = {
    kind: 'record',
    recordType: 'diff',
    id: `${basePath}..${refs.path}`,
    ...(query.from ? { source: query.from } : {}),
    data: {
      path: refs.path,
      basePath,
      headPath: refs.path,
      baseRef: refs.baseRef,
      headRef: refs.headRef,
      additions: diff.additions,
      deletions: diff.deletions,
      patch: diff.patch,
      unchanged: diff.unchanged,
    },
  };

  return {
    results: [row],
    diagnostics:
      diff.additions === 0 && diff.deletions === 0
        ? [
            diagnostic('zeroMatches', 'Files are identical.', {
              backend: 'localGetFileContent',
              severity: 'info',
              blocksAnswer: false,
            }),
          ]
        : [],
    provenance: [{ backend: 'localGetFileContent', source: query.from }],
  };
}

/** Direct two-ref GitHub file diff via two content reads + a pure local line diff. */
async function executeDirectFileDiff(
  query: OqlQuery,
  owner: string | undefined,
  repo: string | undefined,
  refs: { baseRef: string; headRef: string; path: string }
): Promise<AdapterResult> {
  if (!owner || !repo) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'Direct file diff needs a concrete owner/repo.',
          { backend: 'ghGetFileContent' }
        ),
      ],
      provenance: [],
    };
  }

  const read = (ref: string) =>
    runDirect('ghGetFileContent', {
      owner,
      repo,
      path: refs.path,
      branch: ref,
      fullContent: true,
      minify: 'none',
    });

  const [baseRes, headRes] = await Promise.all([
    read(refs.baseRef),
    read(refs.headRef),
  ]);

  // ghGetFileContent returns the row directly under structuredContent.results[0]
  // (keys: id/owner/repo/files) with the file content in files[0].content — there
  // is no nested `.data`, so firstQueryData(...).data is empty. Reading it as
  // `.content` was always undefined, which previously masqueraded as "identical".
  const base = ghFileContentResult(baseRes);
  const head = ghFileContentResult(headRes);
  const unresolvedRef = [
    { label: 'base', ref: refs.baseRef, ...base },
    { label: 'head', ref: refs.headRef, ...head },
  ].find(item => item.status === 'error' || typeof item.content !== 'string');

  if (unresolvedRef) {
    const err = errorText(
      unresolvedRef.error,
      `Could not read ${unresolvedRef.label} ref "${unresolvedRef.ref}" for ${refs.path}.`
    );
    return {
      results: [],
      diagnostics: [
        diagnostic('invalidQuery', err, { backend: 'ghGetFileContent' }),
      ],
      provenance: [{ backend: 'ghGetFileContent', source: query.from }],
    };
  }

  const diff = computeLineDiff(base.content ?? '', head.content ?? '');
  const row: OqlRecordResultRow = {
    kind: 'record',
    recordType: 'diff',
    id: refs.path,
    ...(query.from ? { source: query.from } : {}),
    data: {
      path: refs.path,
      baseRef: refs.baseRef,
      headRef: refs.headRef,
      additions: diff.additions,
      deletions: diff.deletions,
      patch: diff.patch,
      unchanged: diff.unchanged,
    },
  };
  return {
    results: [row],
    diagnostics:
      diff.additions === 0 && diff.deletions === 0
        ? [
            diagnostic('zeroMatches', 'Files are identical at both refs.', {
              backend: 'ghGetFileContent',
              severity: 'info',
              blocksAnswer: false,
            }),
          ]
        : [],
    provenance: [{ backend: 'ghGetFileContent', source: query.from }],
  };
}

export interface LineDiff {
  additions: number;
  deletions: number;
  unchanged: number;
  /** Unified-style patch text (`+`/`-`/` ` line prefixes). */
  patch: string;
}

/**
 * Minimal LCS-based line diff between two file bodies. Pure and dependency-free
 * so it is unit-testable without any backend. Not a byte-perfect git patch —
 * a line-granular additions/deletions view for direct two-ref comparison.
 */
export function computeLineDiff(baseText: string, headText: string): LineDiff {
  const a = baseText === '' ? [] : baseText.split('\n');
  const b = headText === '' ? [] : headText.split('\n');
  const n = a.length;
  const m = b.length;

  // LCS length table.
  const lcs: number[][] = Array.from({ length: n + 1 }, () =>
    new Array<number>(m + 1).fill(0)
  );
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i]![j] =
        a[i] === b[j]
          ? lcs[i + 1]![j + 1]! + 1
          : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
    }
  }

  const lines: string[] = [];
  let additions = 0;
  let deletions = 0;
  let unchanged = 0;
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      lines.push(`  ${a[i]}`);
      unchanged++;
      i++;
      j++;
    } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) {
      lines.push(`- ${a[i]}`);
      deletions++;
      i++;
    } else {
      lines.push(`+ ${b[j]}`);
      additions++;
      j++;
    }
  }
  while (i < n) {
    lines.push(`- ${a[i++]}`);
    deletions++;
  }
  while (j < m) {
    lines.push(`+ ${b[j++]}`);
    additions++;
  }

  return { additions, deletions, unchanged, patch: lines.join('\n') };
}

export async function executeArtifacts(
  query: OqlQuery
): Promise<AdapterResult> {
  const path =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;
  if (!path) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'target:"artifacts" needs a local file `from` (path).',
          { backend: 'localBinaryInspect' }
        ),
      ],
      provenance: [],
    };
  }
  const result = await runDirect('localBinaryInspect', {
    path,
    ...params(query),
  });
  // An artifact is a single entity: keep ONE record row carrying the full
  // payload (mode, entries/strings/symbols, derived localPath, nextScanOffset)
  // rather than expanding inner arrays into rows — otherwise parent-level
  // metadata (localPath, scan cursor) is lost to the continuation builders.
  const { data, status } = firstQueryData(result);
  const diagnostics = statusDiagnostics(result, 'localBinaryInspect');
  if (status === 'error' || !data) {
    return {
      results: [],
      diagnostics: diagnostics.length
        ? diagnostics
        : [
            diagnostic('zeroMatches', 'localBinaryInspect returned no data.', {
              backend: 'localBinaryInspect',
              severity: 'info',
              blocksAnswer: false,
            }),
          ],
      provenance: [{ backend: 'localBinaryInspect', source: query.from }],
    };
  }
  return {
    results: records([data], 'artifact', query.from),
    diagnostics: [...diagnostics, ...artifactPartialDiagnostics(data, query)],
    provenance: [{ backend: 'localBinaryInspect', source: query.from }],
  };
}

type ArtifactTextPagination = {
  hasMore?: boolean;
  nextCharOffset?: number;
  charLength?: number;
};

function artifactPartialDiagnostics(
  data: Record<string, unknown>,
  query: OqlQuery
): OqlDiagnostic[] {
  const pagination =
    data.pagination && typeof data.pagination === 'object'
      ? (data.pagination as ArtifactTextPagination)
      : undefined;
  if (data.isPartial !== true && pagination?.hasMore !== true) return [];
  return [
    diagnostic(
      'partialResult',
      'Artifact text is paginated; follow the artifact continuation before treating the inline content as complete.',
      {
        backend: 'localBinaryInspect',
        blocksAnswer: true,
        continuation: artifactContentContinuation(query, pagination),
      }
    ),
  ];
}

function artifactContentContinuation(
  query: OqlQuery,
  pagination: ArtifactTextPagination | undefined
): OqlContinuation | undefined {
  if (
    pagination?.hasMore !== true ||
    typeof pagination.nextCharOffset !== 'number'
  ) {
    return undefined;
  }
  return {
    query: {
      ...query,
      params: {
        ...(query.params ?? {}),
        charOffset: pagination.nextCharOffset,
        ...(typeof pagination.charLength === 'number'
          ? { charLength: pagination.charLength }
          : {}),
      },
    },
    why: 'Read the next inline artifact text window.',
    confidence: 'exact',
  };
}

export async function executeSemantics(
  query: OqlQuery
): Promise<AdapterResult> {
  let uri: string | undefined;
  let workspaceRoot: string | undefined;
  const provenance: AdapterResult['provenance'] = [];
  const diagnostics: OqlDiagnostic[] = [];
  const semanticParams = params(query) as {
    uri?: string;
    type?: string;
    workspaceRoot?: string;
  } & Record<string, unknown>;
  const isWorkspaceSymbol = semanticParams.type === 'workspaceSymbol';
  const explicitUri =
    typeof semanticParams.uri === 'string' ? semanticParams.uri : undefined;
  const explicitWorkspaceRoot =
    typeof semanticParams.workspaceRoot === 'string'
      ? semanticParams.workspaceRoot
      : undefined;

  if (query.from?.kind === 'local') {
    if (isWorkspaceSymbol) {
      const fromPath = query.from.path;
      const fromIsDirectory = isExistingDirectory(fromPath);
      workspaceRoot =
        explicitWorkspaceRoot ??
        (fromIsDirectory ? nodePath.resolve(fromPath) : undefined);
      uri = explicitUri ?? (fromIsDirectory ? undefined : fromPath);
    } else {
      uri = explicitUri ?? query.from.path;
    }
  } else if (query.from?.kind === 'materialized') {
    const scopePath = firstScopePath(query);
    const scopedUri = scopePath
      ? nodePath.isAbsolute(scopePath)
        ? scopePath
        : nodePath.join(query.from.localPath, scopePath)
      : undefined;
    if (isWorkspaceSymbol) {
      workspaceRoot = explicitWorkspaceRoot ?? query.from.localPath;
      uri = explicitUri ?? scopedUri;
    } else {
      uri = explicitUri ?? scopedUri ?? query.from.localPath;
    }
  } else if (query.from?.kind === 'github') {
    // remote semantics: materialize the file, then run LSP locally.
    const { owner, repo } = splitRepo(query.from);
    if (!owner || !repo) {
      diagnostics.push(
        diagnostic('invalidQuery', 'Remote semantics needs owner/repo.', {
          backend: 'lspGetSemantics',
        })
      );
      return { results: [], diagnostics, provenance };
    }
    const requestedUri =
      typeof semanticParams.uri === 'string' ? semanticParams.uri : undefined;
    const scopePath = firstScopePath(query);
    const sparsePath =
      requestedUri && !nodePath.isAbsolute(requestedUri)
        ? requestedUri
        : scopePath;
    const clone = await runDirect('ghCloneRepo', {
      owner,
      repo,
      ...(query.from.ref ? { branch: query.from.ref } : {}),
      ...(sparsePath ? { sparsePath } : {}),
    });
    const cloneData = firstQueryData<{ localPath?: string }>(clone).data;
    const cloneLocalPath = materializedClonePath(clone, cloneData?.localPath);
    if (!cloneLocalPath) {
      diagnostics.push(
        diagnostic(
          'materializationFailed',
          'Could not materialize repo for remote LSP.',
          { backend: 'ghCloneRepo' }
        )
      );
      return { results: [], diagnostics, provenance };
    }
    provenance.push({
      backend: 'ghCloneRepo',
      source: query.from,
      materializedPath: cloneLocalPath,
    });
    if (isWorkspaceSymbol) {
      workspaceRoot = explicitWorkspaceRoot ?? cloneLocalPath;
      if (requestedUri) {
        uri = nodePath.isAbsolute(requestedUri)
          ? requestedUri
          : nodePath.join(cloneLocalPath, requestedUri);
      } else if (scopePath) {
        uri = nodePath.isAbsolute(scopePath)
          ? scopePath
          : nodePath.join(cloneLocalPath, scopePath);
      }
    } else if (requestedUri) {
      uri = nodePath.isAbsolute(requestedUri)
        ? requestedUri
        : nodePath.join(cloneLocalPath, requestedUri);
    } else if (scopePath) {
      uri = nodePath.isAbsolute(scopePath)
        ? scopePath
        : nodePath.join(cloneLocalPath, scopePath);
    } else {
      uri = cloneLocalPath;
    }
  }

  if (!uri && !workspaceRoot) {
    diagnostics.push(
      diagnostic('invalidQuery', 'target:"semantics" needs a `from` anchor.', {
        backend: 'lspGetSemantics',
      })
    );
    return { results: [], diagnostics, provenance };
  }

  // params carry the LSP operation (type, symbolName, lineHint, …); for local
  // and materialized queries params.uri may override a directory/root `from`
  // anchor. For remote queries, params.uri has already been lowered to the
  // cloned sparse path above.
  const {
    uri: _ignoredUri,
    symbolKind,
    workspaceRoot: _ignoredWorkspaceRoot,
    ...lspParams
  } = semanticParams;
  const result = await runDirect('lspGetSemantics', {
    ...lspParams,
    ...(workspaceRoot ? { workspaceRoot } : {}),
    ...(uri ? { uri } : {}),
  });
  const { data, status } = firstQueryData(result);
  const recordData = data as Record<string, unknown> | undefined;
  const pagination = semanticPagination(recordData, query);
  const sourceUri = resolveSemanticSourceUri(
    stringFrom(recordData?.uri),
    uri,
    workspaceRoot
  );
  const source = semanticSource(query, sourceUri);
  const semanticItems = filterSemanticItemsByKind(
    expandSemanticData(recordData),
    symbolKind
  );
  return {
    results:
      status === 'error' ? [] : records(semanticItems, 'semantics', source),
    ...(pagination ? { pagination } : {}),
    diagnostics: [
      ...diagnostics,
      ...statusDiagnostics(result, 'lspGetSemantics'),
      ...semanticDiagnostics(recordData, query),
    ],
    provenance: [
      ...provenance,
      {
        backend: 'lspGetSemantics',
        source,
      },
    ],
  };
}

function expandSemanticData(
  data: Record<string, unknown> | undefined
): unknown[] {
  if (!data) return [];
  const payload = isRecord(data.payload) ? data.payload : undefined;
  const symbols = payload?.symbols;
  if (Array.isArray(symbols)) {
    const uri = stringFrom(data.uri);
    return symbols.map(symbol =>
      isRecord(symbol)
        ? {
            ...(uri && typeof symbol.uri !== 'string' ? { uri } : {}),
            ...symbol,
          }
        : symbol
    );
  }
  return expandData(data);
}

function filterSemanticItemsByKind(
  items: unknown[],
  symbolKind: unknown
): unknown[] {
  if (typeof symbolKind !== 'string' || !symbolKind.trim()) return items;
  const wanted = symbolKind.trim().toLowerCase();
  return items.filter(item => {
    if (!item || typeof item !== 'object') return false;
    const kind = (item as Record<string, unknown>).kind;
    return String(kind ?? '').toLowerCase() === wanted;
  });
}

function semanticSource(query: OqlQuery, uri: string): QuerySource {
  if (query.from?.kind === 'local') {
    return { ...query.from, path: uri };
  }
  if (query.from?.kind === 'materialized') {
    return { ...query.from, localPath: uri };
  }
  if (query.from?.kind === 'github') {
    return { kind: 'materialized', localPath: uri, source: query.from };
  }
  return query.from ?? { kind: 'local', path: uri };
}

export async function executeResearch(query: OqlQuery): Promise<AdapterResult> {
  const p = params(query);
  const root =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;

  if (!root) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'requiresMaterialization',
          'target:"research" needs a complete local file universe. Use a local/materialized source, or materialize a bounded GitHub corpus first.',
          {
            backend: 'smartOqlResearch',
            repair: {
              message:
                'Run target:"materialize" for a bounded GitHub repo/subtree, then run target:"research" against the returned localPath.',
            },
          }
        ),
      ],
      provenance: [],
    };
  }

  const facets = Array.isArray(p.facets)
    ? p.facets.filter((facet): facet is string => typeof facet === 'string')
    : undefined;
  const mode = requestedResearchMode(p.mode);
  if (mode === 'prove' && typeof p.intent !== 'string') {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'target:"research" mode:"prove" requires params.intent so the proof lane is deterministic. Use intent:"reachability"|"dependencies"|"symbols"|"general", then follow packet next.semantic/next.fetch continuations for missing proof.',
          {
            backend: 'smartOqlResearch',
            queryPath: 'params.intent',
            repair: {
              message:
                'Add params.intent. Example: params:{ mode:"prove", intent:"reachability", facets:["symbols","files","relations"] }.',
            },
          }
        ),
      ],
      provenance: [{ backend: 'smartOqlResearch', source: query.from }],
    };
  }

  let data: Awaited<ReturnType<typeof analyzeResearchFlow>>;
  try {
    data = await analyzeResearchFlow({
      root,
      goal: typeof p.goal === 'string' ? p.goal : undefined,
      intent: typeof p.intent === 'string' ? p.intent : undefined,
      facets,
      mode,
      maxFiles: typeof p.maxFiles === 'number' ? p.maxFiles : undefined,
    });
  } catch (err) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          err instanceof Error
            ? err.message
            : 'Could not analyze the requested research root.',
          { backend: 'smartOqlResearch' }
        ),
      ],
      provenance: [{ backend: 'smartOqlResearch', source: query.from }],
    };
  }

  // Plan mode returns the flow only (no scan), so there is nothing to packetize.
  const { packets, graphSummary } =
    data.mode === 'plan'
      ? { packets: [], graphSummary: undefined }
      : buildResearchPackets(data);

  const caveats = [...data.caveats];
  if (p.mode === 'prove') {
    caveats.push(
      'mode:"prove" requested on target:"research": packets are candidate-grade unless LSP proof is attached. Native AST facts are included where available, but LSP reference proof is not run here. Use target:"graph" with proof:"lsp" or follow each packet\'s next.semantic.'
    );
  }
  const pageWindow = graphSummary
    ? packetPage(query, packets.length)
    : undefined;
  const pagedPackets = pageWindow
    ? packets.slice(pageWindow.packetsStart, pageWindow.packetsEnd)
    : [];
  if (
    pageWindow &&
    packets.length > 0 &&
    pageWindow.packetsStart >= packets.length
  ) {
    caveats.push(
      `Packet page ${pageWindow.pagination.currentPage} is outside the available packet range (${pageWindow.pagination.totalPages} page(s)).`
    );
  }

  // P1: detailed view returns per-domain *windows* (sliced + paged), not whole
  // arrays — honoring `select` so a narrow projection drops unrequested domains.
  const detailed =
    query.view === 'detailed'
      ? buildDetailedDomains(query, data)
      : { fields: {} as Record<string, unknown> };

  const enriched: Record<string, unknown> = {
    kind: data.kind,
    goal: data.goal,
    intent: data.intent,
    facets: data.facets,
    mode: data.mode,
    root: data.root,
    flow: data.flow,
    summary: data.summary,
    graphCapabilities: data.graphCapabilities,
    nativeGraphSummary: nativeGraphSummary(data.graphFacts),
    caveats,
    ...(graphSummary
      ? {
          graphSummary,
          packetPage: pageWindow?.pagination,
          packets: pagedPackets,
        }
      : {}),
    ...detailed.fields,
  };

  // The envelope pagination drives `next.page`; for detailed view it must
  // advance the packet window AND every detailed domain together.
  const pagination = combinePagination(
    pageWindow?.pagination,
    detailed.pagination
  );

  return {
    results: records([enriched], 'research', query.from),
    ...(pagination ? { pagination } : {}),
    diagnostics: [],
    provenance: [{ backend: 'smartOqlResearch', source: query.from }],
  };
}

export async function executeGraph(query: OqlQuery): Promise<AdapterResult> {
  const p = params(query);
  const root =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;

  if (!root) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'requiresMaterialization',
          'target:"graph" needs a complete local file universe. Use a local/materialized source, or materialize a bounded GitHub corpus first.',
          {
            backend: 'smartOqlGraph',
            repair: {
              message:
                'Run target:"materialize" for a bounded GitHub repo/subtree, then run target:"graph" against the returned localPath.',
            },
          }
        ),
      ],
      provenance: [],
    };
  }

  const facets = Array.isArray(p.facets)
    ? p.facets.filter((facet): facet is string => typeof facet === 'string')
    : undefined;
  const mode = requestedResearchMode(p.mode);
  if (mode === 'prove' && typeof p.intent !== 'string') {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'target:"graph" mode:"prove" requires params.intent so the proof lane is deterministic. Use intent:"reachability"|"dependencies"|"symbols"|"general", then follow graph packet next.semantic/next.fetch continuations for missing proof.',
          {
            backend: 'smartOqlGraph',
            queryPath: 'params.intent',
            repair: {
              message:
                'Add params.intent. Example: params:{ mode:"prove", intent:"reachability", direction:"incoming" }.',
            },
          }
        ),
      ],
      provenance: [{ backend: 'smartOqlGraph', source: query.from }],
    };
  }

  let analysis: Awaited<ReturnType<typeof analyzeResearchFlow>>;
  try {
    analysis = await analyzeResearchFlow({
      root,
      goal: typeof p.goal === 'string' ? p.goal : undefined,
      intent: typeof p.intent === 'string' ? p.intent : undefined,
      facets,
      mode,
      maxFiles: typeof p.maxFiles === 'number' ? p.maxFiles : undefined,
    });
  } catch (err) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          err instanceof Error
            ? err.message
            : 'Could not analyze the requested graph root.',
          { backend: 'smartOqlGraph' }
        ),
      ],
      provenance: [{ backend: 'smartOqlGraph', source: query.from }],
    };
  }

  const bundle =
    analysis.mode === 'plan' ? undefined : buildResearchPackets(analysis);
  const filters = graphFilters(p);
  const packets = bundle?.packets ?? [];
  const proofDiagnostics = shouldRunLspProof(analysis.mode, p)
    ? await escalateGraphPacketsWithLsp(
        root,
        query,
        packets,
        filters,
        graphProofLimit(query, p)
      )
    : [];
  const graphSummary = summarizePacketGraph(packets);
  const view = buildGraphView(
    query,
    packets,
    graphSummary,
    filters,
    analysis.graphFacts,
    root
  );

  const caveats = [
    ...(view.data.caveats ?? []),
    ...analysis.caveats,
    ...(analysis.mode === 'plan'
      ? ['mode:"plan" requested: graph packets were not built.']
      : []),
    ...(p.mode === 'prove'
      ? [
          shouldRunLspProof(analysis.mode, p)
            ? 'mode:"prove" requested: LSP proof escalation ran for the current graph page only. Follow next.page and next.semantic for remaining/open proof.'
            : 'mode:"prove" requested: graph rows are candidate-grade only. Follow packet next.semantic to confirm references.',
        ]
      : []),
  ];

  const enriched: OqlGraphData = {
    ...view.data,
    goal: analysis.goal,
    intent: analysis.intent,
    facets: analysis.facets,
    mode: analysis.mode,
    root: analysis.root,
    flow: analysis.flow,
    graphCapabilities: analysis.graphCapabilities,
    nativeGraphSummary: nativeGraphSummary(analysis.graphFacts),
    caveats,
  };

  return {
    results: records([enriched], 'graph', query.from),
    pagination: view.pagination,
    diagnostics: proofDiagnostics,
    provenance: [{ backend: 'smartOqlGraph', source: query.from }],
  };
}

/** Dispatch map: target -> adapter. */
export const RESEARCH_TARGET_ADAPTERS: Record<
  string,
  (q: OqlQuery) => Promise<AdapterResult>
> = {
  repositories: executeRepositories,
  packages: executePackages,
  pullRequests: executeHistory,
  commits: executeHistory,
  diff: executeDiff,
  artifacts: executeArtifacts,
  semantics: executeSemantics,
  research: executeResearch,
  graph: executeGraph,
};
