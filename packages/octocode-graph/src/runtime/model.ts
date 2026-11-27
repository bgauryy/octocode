import type { GraphData } from './types.ts';

/** A `path:line` citation (model.json `$defs.evidence`); resolveModel warns when one is missing. */
interface ModelEvidence {
  path?: string;
  line?: number;
}

export interface ModelLayer {
  id: string;
  label?: string;
  order: number;
  match: string[];
  color?: string;
  exempt?: boolean;
}

interface ModelNode {
  id: string;
  label?: string;
  layer?: string;
  description?: string;
}

interface ModelEdge {
  source: string;
  target: string;
  kind?: string;
  action?: string;
  evidence?: ModelEvidence;
}

interface FlowStep {
  from: string;
  to: string;
  action?: string;
  kind?: string;
  async?: boolean;
  evidence?: ModelEvidence;
}

interface ModelFlow {
  id: string;
  name?: string;
  steps: FlowStep[];
}

interface ModelFinding {
  title: string;
  confidence: string;
  detail?: string;
  evidence?: ModelEvidence[];
}

/** model.json (scheme/model.json): the semantics an agent adds over the graph. resolveModel checks it against the graph. */
export interface Model {
  meta?: { summary?: string };
  layers?: ModelLayer[];
  assign?: { id: string; layer: string }[];
  nodes?: ModelNode[];
  edges?: ModelEdge[];
  allowed?: { source: string; target: string; reason?: string }[];
  flows?: ModelFlow[];
  findings?: ModelFinding[];
}

/** A layer while it is resolved: its globs compiled. */
type MatchedLayer = ModelLayer & { matchers?: RegExp[] };

interface LayerState {
  ids: Set<string>;
  byId: Map<string | undefined, MatchedLayer>;
  layerOf: Record<string, string | undefined>;
  warnings: string[];
}

function assignLayers(M: Model, { ids, byId, layerOf, warnings }: LayerState) {
  for (const a of M.assign ?? []) {
    if (!ids.has(a.id)) warnings.push(`assign: unknown node ${a.id}`);
    else if (!byId.has(a.layer))
      warnings.push(`assign ${a.id}: unknown layer ${a.layer}`);
    else layerOf[a.id] = a.layer;
  }
  for (const n of M.nodes ?? []) {
    if (n.layer && !byId.has(n.layer))
      warnings.push(`node ${n.id}: unknown layer ${n.layer}`);
    else if (n.layer) layerOf[n.id] = n.layer;
  }
}
const glob = (g: string) =>
  new RegExp(
    `^${g
      .replace(/[.+^${}()|[\]\\]/g, '\\$&')
      .replace(/\*\*\//g, '\x01')
      .replace(/\*\*/g, '\0')
      .replace(/\*/g, '[^/]*')
      .replaceAll('\0', '.*')
      .replaceAll('\x01', '(?:.*/)?')}$`
  );
const ev = (e: ModelEvidence | undefined) => e && e.path && e.line;
function validateEvidence(M: Model, ids: Set<string>, warnings: string[]) {
  for (const e of M.edges ?? []) {
    for (const k of ['source', 'target'] as const)
      if (!ids.has(e[k]))
        warnings.push(`edge ${e.source} > ${e.target}: unknown ${k} ${e[k]}`);
    if (!ev(e.evidence))
      warnings.push(`edge ${e.source} > ${e.target}: missing evidence`);
  }
  for (const fl of M.flows ?? []) {
    fl.steps.forEach((st, i) => {
      for (const k of ['from', 'to'] as const)
        if (!ids.has(st[k]))
          warnings.push(`flow ${fl.id} step ${i + 1}: unknown ${k} ${st[k]}`);
      if (!ev(st.evidence))
        warnings.push(`flow ${fl.id} step ${i + 1}: missing evidence`);
    });
  }
  for (const x of M.findings ?? [])
    if (
      x.confidence === 'confirmed' &&
      (!x.evidence?.length || !x.evidence.every(ev))
    )
      warnings.push(`finding "${x.title}": confirmed needs evidence`);
}
// Overlay: validate model.json against the graph, assign layers, derive upward imports.
export function resolveModel(
  D: Pick<GraphData, 'files' | 'imports'>,
  M: Model
) {
  const warnings: string[] = [];
  const ids = new Set([
    ...D.files.map(n => n.id),
    ...(M.nodes ?? []).map(n => n.id),
  ]);
  const layers: MatchedLayer[] = [...(M.layers ?? [])].toSorted(
    (a, b) => a.order - b.order
  );
  const byId = new Map<string | undefined, MatchedLayer>(
    layers.map(l => [l.id, l])
  );
  const palette = [
    '#4e79a7',
    '#59a14f',
    '#edc948',
    '#e15759',
    '#b07aa1',
    '#76b7b2',
    '#ff9da7',
    '#9c755f',
  ];
  layers.forEach((l, i) => {
    l.color ??= palette[i % palette.length];
    l.matchers = l.match.map(glob);
  });
  const layerOf: Record<string, string | undefined> = {};
  // Every layer's matchers are compiled above.
  for (const n of D.files)
    layerOf[n.id] = layers.find(l => l.matchers?.some(m => m.test(n.id)))?.id;
  assignLayers(M, { ids, byId, layerOf, warnings });

  validateEvidence(M, ids, warnings);
  const allowed = new Set(
    (M.allowed ?? []).map(a => `${a.source}>${a.target}`)
  );
  const order = (id: string) => byId.get(layerOf[id])?.order;
  const upward = D.imports
    .filter(e => {
      const [a, b] = [order(e.source), order(e.target)];
      return (
        a != null &&
        b != null &&
        a > b &&
        !byId.get(layerOf[e.source])?.exempt &&
        !allowed.has(`${e.source}>${e.target}`)
      );
    })
    .map(e => ({ source: e.source, target: e.target, line: e.line }));
  const unplaced = D.files.filter(n => !layerOf[n.id]).map(n => n.id);
  if (layers.length && unplaced.length)
    warnings.push(
      `${unplaced.length} files match no layer (first: ${unplaced[0]})`
    );
  return {
    meta: M.meta ?? {},
    layers: layers.map(({ matchers, ...l }) => l),
    layerOf,
    nodes: M.nodes ?? [],
    edges: M.edges ?? [],
    flows: M.flows ?? [],
    findings: M.findings ?? [],
    upward,
    warnings,
  };
}

/** A model.json checked against the graph: layers placed, upward imports derived, every problem a warning. */
export type ResolvedModel = ReturnType<typeof resolveModel>;
