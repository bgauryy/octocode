/**
 * Native AST edge overlay: turns `analyzeResearchFlow`'s per-file graph facts
 * (calls/imports/exports/etc.) into `EvidenceEdge`s layered on top of the
 * research-packet graph, restricted to nodes already visible in the view.
 */
import nodePath from 'node:path';
import type { analyzeResearchFlow } from '../../research/analyze.js';
import type {
  EvidenceEdge,
  EvidenceRelation,
  EvidenceSubject,
} from '../../research/packets.js';
import { addEdge, relationAllowed, type GraphFilters } from './filters.js';

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

export function addNativeGraphEdges(
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
