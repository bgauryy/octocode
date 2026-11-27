// Display model of the WebGL graph: which nodes and links are drawn, as persistent objects the layout owns.
// The store (live.ts) is never mutated: every drawn node and link is a pool object keyed by its id, so positions
// survive every sync and the graph grows in place. Overview folds files into one node per community (size > 1),
// singletons into one "isolated files" node and files without metrics into one "outside the graph" node;
// links between folded nodes carry the summed weight, direction kept.
import type { Community } from '../runtime/types.ts';
import type {
  CommunityDetail,
  EdgeKind,
  LiveState,
  ViewEdge,
  ViewNode,
  Via,
} from './live-state.ts';

export const ISO = 'c:iso';
export const REST = 'c:rest';
const FOLDER = '\0folder/';
const BUCKET_LABEL: Record<string, string> = {
  [ISO]: 'isolated files',
  [REST]: 'outside the graph',
};

/** The store fields the display model reads. */
export type OverviewStore = Pick<LiveState, 'nodes' | 'edges' | 'metrics'>;
export interface Toggles {
  imports: boolean;
  calls: boolean;
  weak: boolean;
  tests: boolean;
  docs: boolean;
  labels: boolean;
}
export interface Point {
  x: number;
  y: number;
  z: number;
}
/** A position the layout has given a drawn node (seed sets x, y and z together). */
export interface Placed {
  x: number;
  y: number;
  z?: number;
}

interface DrawnBase {
  id: string;
  label: string;
  x?: number;
  y?: number;
  z?: number;
  vx?: number;
  vy?: number;
  vz?: number;
  /** Drawn in the latest view. */
  shown?: boolean;
  /** Drawn before the latest growth: calmed while new nodes find their place. */
  settled?: boolean;
  labelText?: string;
}
/** A folded group: a metrics community, the isolated or outside-the-graph bucket, or (before metrics) a folder. */
export interface BucketNode extends DrawnBase {
  kind: 'community';
  community: number | null;
  folder: string | null;
  info: Community | null;
  size: number;
  parsed: number;
  pr: number;
  depthSum: number;
  depthN: number;
  members: string[];
  ref?: never;
  bucket?: never;
}
/** A drawn file or package: `ref` is its store node, `bucket` the group it folds into. */
export interface FileDrawn extends DrawnBase {
  kind: ViewNode['kind'];
  ref: ViewNode;
  bucket?: string | null;
  size?: never;
}
export type DrawnNode = BucketNode | FileDrawn;

interface LinkCore {
  key: string;
  src: string;
  dst: string;
  /** Drawn ids; the layout's link force replaces them with the drawn nodes. */
  source: string | DrawnNode;
  target: string | DrawnNode;
}
export interface DrawnLink extends LinkCore {
  /** The store edge a file-to-file link draws. */
  ref?: ViewEdge;
  agg: boolean;
  kind: EdgeKind;
  via: Via | 'agg';
  w: number;
  weak: boolean;
  count: number;
}
/** A link touching a folded node: the summed weight of the store edges it stands for, per kind. */
interface AggLink extends DrawnLink {
  agg: true;
  imports: number;
  calls: number;
}

/** Drawn objects that persist across views, by id. File ids never collide with bucket ids (`c:…`, folder keys). */
export interface ViewPool {
  pool: Map<string, DrawnNode>;
  links: Map<string, LinkCore>;
  expanded: Set<string>;
  metricsRef?: LiveState['metrics'];
  communities?: Map<number, Community>;
}
export interface ViewOptions {
  overview: boolean;
  reveal: string | null;
  toggles: Toggles;
  dims: number;
  anchors: Map<string, Point>;
}
export interface View {
  nodes: DrawnNode[];
  links: DrawnLink[];
  /** The drawn id standing for each visible store node. */
  shown: Map<string, string>;
  adj: Map<string, Set<string>>;
  maxWeight: number;
}
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

const strong = (e: ViewEdge) =>
  e.confidence === 'high' || e.via === 'both' || e.via === 'lsp';
export const visibleNode = (n: ViewNode, t: Toggles) => t.docs || n.inGraph;
export const visibleEdge = (e: ViewEdge, t: Toggles) =>
  t[e.kind] && (t.tests || !e.test) && (t.weak || strong(e));
const ESC: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
};
const esc = (s: string | number | null | undefined) =>
  String(s ?? '').replace(/[&<>"]/g, c => ESC[c]);
const fmt = (x: number | null | undefined) =>
  x == null
    ? '–'
    : Number.isInteger(x) || Math.abs(x) >= 100
      ? Math.round(x).toLocaleString()
      : x.toPrecision(3);
const jitter = (spread: number) => (Math.random() - 0.5) * spread;
export const isPlaced = <T extends DrawnNode>(o: T): o is T & Placed =>
  o.x != null;

// Communities that fold (size > 1), by id; cached per metrics event. New metrics renumber communities, so the
// expanded set starts over.
function communities(V: ViewPool, S: OverviewStore) {
  if (V.metricsRef !== S.metrics || !V.communities) {
    if (V.communities) V.expanded.clear();
    V.metricsRef = S.metrics;
    V.communities = new Map(
      (S.metrics?.communities ?? []).filter(c => c.size > 1).map(c => [c.id, c])
    );
  }
  return V.communities;
}
// Other views (the wiki drawer) follow community clicks: a bucket id `c:<n>` names metrics community n; the
// isolated and outside-the-graph buckets name none.
export function announceCommunity(bucket: string) {
  const id = Number(bucket.slice(2));
  document.dispatchEvent(
    new CustomEvent<CommunityDetail>('gv:community', {
      detail: { bucket, community: Number.isInteger(id) ? id : null },
    })
  );
}

export function bucketOf(V: ViewPool, S: OverviewStore, n: ViewNode) {
  if (!S.metrics) {
    if (n.kind !== 'file') return null;
    const parts = n.path.split('/');
    const folder = parts.slice(0, Math.min(2, parts.length - 1)).join('/');
    return folder ? FOLDER + folder : null;
  }
  if (!n.metrics) return REST;
  return communities(V, S).has(n.metrics.community)
    ? `c:${n.metrics.community}`
    : ISO;
}

function bucketNode(V: ViewPool, S: OverviewStore, id: string) {
  let b = V.pool.get(id) as BucketNode | undefined;
  if (!b) {
    // The assignments below fill every other field.
    b = { id, kind: 'community' } as BucketNode;
    V.pool.set(id, b);
  }
  // A later metrics event (a refresh) may relabel or renumber communities: read them each time.
  const folder = id.startsWith(FOLDER) ? id.slice(FOLDER.length) : null;
  const c = folder ? null : communities(V, S).get(Number(id.slice(2)));
  Object.assign(b, {
    community: c ? c.id : null,
    folder,
    label: folder ?? c?.label ?? BUCKET_LABEL[id],
    info: c ?? null,
  });
  Object.assign(b, {
    size: 0,
    parsed: 0,
    pr: 0,
    depthSum: 0,
    depthN: 0,
    members: [],
  });
  return b;
}
function fileNode(V: ViewPool, n: ViewNode) {
  let o = V.pool.get(n.id) as FileDrawn | undefined;
  if (!o) {
    // kind is set below and label by labelFiles before anything reads it.
    o = { id: n.id, ref: n } as FileDrawn;
    V.pool.set(n.id, o);
  }
  o.kind = n.kind;
  return o;
}

// A drawn node appears next to what it stands for: an expanded file at its community, a folded community at its
// members' centroid, any other file at its folder's centroid (or anchor).
function seed(
  o: DrawnNode,
  at: Placed | null | undefined,
  view: Pick<ViewOptions, 'dims'>
) {
  const spread = 12 + 2 * Math.sqrt(o.size ?? 1);
  const p = at ?? { x: 0, y: 0, z: 0 };
  o.x = p.x + jitter(spread);
  o.y = p.y + jitter(spread);
  o.z = view.dims === 3 ? (p.z ?? 0) + jitter(spread) : 0;
  o.vx = 0;
  o.vy = 0;
  o.vz = 0;
}
function centroids<T extends DrawnNode>(
  list: T[],
  keyOf: (o: T) => string | null
) {
  const acc = new Map<string | null, Point & { n: number }>();
  for (const o of list) {
    if (!isPlaced(o) || !o.shown) continue;
    const k = keyOf(o),
      c = acc.get(k) ?? { x: 0, y: 0, z: 0, n: 0 };
    c.x += o.x;
    c.y += o.y;
    c.z += o.z ?? 0;
    c.n++;
    acc.set(k, c);
  }
  for (const c of acc.values()) {
    c.x /= c.n;
    c.y /= c.n;
    c.z /= c.n;
  }
  return acc;
}

// Which drawn id stands for each visible store node.
function assign(V: ViewPool, S: OverviewStore, view: ViewOptions) {
  const shown = new Map<string, string>(),
    drawn = new Map<string, DrawnNode>();
  communities(V, S);
  for (const n of S.nodes.values()) {
    if (!visibleNode(n, view.toggles)) continue;
    const bucket = view.overview ? bucketOf(V, S, n) : null;
    const id =
      bucket && n.id !== view.reveal && !V.expanded.has(bucket) ? bucket : n.id;
    shown.set(n.id, id);
    let o = drawn.get(id);
    if (!o) {
      o = id === n.id ? fileNode(V, n) : bucketNode(V, S, id);
      drawn.set(id, o);
    }
    if (o.kind === 'community') addMember(o, n);
    else o.bucket = bucket;
  }
  return { shown, drawn };
}
function addMember(b: BucketNode, n: ViewNode) {
  b.size++;
  if (n.inGraph) b.parsed++;
  b.members.push(n.id);
  b.pr += n.metrics?.pagerank ?? 0;
  if (n.metrics) {
    b.depthSum += n.metrics.depth;
    b.depthN++;
  }
}

interface Seeds {
  view: ViewOptions;
  byGroup: Map<string | null, Point>;
  byBucket: Map<string | null, Point>;
}
// Only nodes that were not drawn last time move: a community seeds at its members' centroid, a file at the
// community it unfolds from, else at its folder's centroid or anchor.
function seedNode(V: ViewPool, o: DrawnNode, at: Seeds) {
  if (o.kind === 'community') {
    seed(o, at.byBucket.get(o.id) ?? (isPlaced(o) ? o : null), at.view);
    return;
  }
  const b = o.bucket ? V.pool.get(o.bucket) : undefined;
  if (b && isPlaced(b) && (b.shown || o.bucket)) seed(o, b, at.view);
  else if (o.x == null)
    seed(
      o,
      at.byGroup.get(o.ref.group) ?? at.view.anchors.get(o.ref.group),
      at.view
    );
}
function place(
  V: ViewPool,
  S: OverviewStore,
  drawn: Map<string, DrawnNode>,
  view: ViewOptions
) {
  const prior = [...V.pool.values()].filter(
    (o): o is FileDrawn => o.kind !== 'community' && Boolean(o.shown)
  );
  const at: Seeds = {
    view,
    byGroup: centroids(prior, o => o.ref.group),
    byBucket: view.overview
      ? centroids(prior, o => bucketOf(V, S, o.ref))
      : new Map<string | null, Point>(),
  };
  for (const o of drawn.values())
    if (!o.shown || o.x == null) seedNode(V, o, at);
  for (const o of V.pool.values()) o.shown = drawn.has(o.id);
}

function linkObj(V: ViewPool, key: string, src: string, dst: string) {
  let l = V.links.get(key);
  if (!l) {
    l = { key, src, dst, source: src, target: dst };
    V.links.set(key, l);
  }
  return l;
}
// File links are the store's edges; any link touching a folded node sums the weights it stands for.
function collectLinks(
  V: ViewPool,
  S: OverviewStore,
  shown: Map<string, string>,
  view: ViewOptions
) {
  const links = new Map<string, DrawnLink>();
  for (const e of S.edges.values()) {
    if (!visibleEdge(e, view.toggles)) continue;
    const s = e.source,
      t = e.target,
      a = shown.get(s),
      b = shown.get(t);
    if (!a || !b || a === b) continue;
    const w = e.w ?? 1;
    if (a === s && b === t) {
      links.set(
        e.key,
        Object.assign(linkObj(V, e.key, a, b), {
          ref: e,
          agg: false,
          kind: e.kind,
          via: e.via,
          w,
          weak: !strong(e),
          count: 1,
        })
      );
      continue;
    }
    const key = `agg|${a}|${b}`;
    // `agg|…` keys hold only aggregate links.
    let l = links.get(key) as AggLink | undefined;
    if (!l) {
      l = Object.assign(linkObj(V, key, a, b), {
        agg: true as const,
        via: 'agg' as const,
        w: 0,
        count: 0,
        weak: false,
        imports: 0,
        calls: 0,
        kind: 'imports' as const,
      });
      links.set(key, l);
    }
    l.w += w;
    l.count++;
    l[e.kind] += w;
    l.kind = l.imports >= l.calls ? 'imports' : 'calls';
  }
  return [...links.values()];
}

// Repeated file names are labelled with as many trailing path segments as it takes (up to five), so ten
// `index.ts` hubs stay distinguishable.
const tail = (o: FileDrawn, k: number) =>
  o.kind === 'file' && k > 1
    ? o.ref.path.split('/').slice(-k).join('/')
    : o.ref.label;
function labelFiles(drawn: Map<string, DrawnNode>) {
  const files = [...drawn.values()].filter(
    (o): o is FileDrawn => o.kind !== 'community'
  );
  for (const o of files) o.label = o.ref.label;
  for (const k of [2, 3, 4, 5]) {
    const count = new Map<string, number>();
    for (const o of files) count.set(o.label, (count.get(o.label) ?? 0) + 1);
    for (const o of files)
      if ((count.get(o.label) ?? 0) > 1) o.label = tail(o, k);
  }
}

export function adjacency(links: DrawnLink[]) {
  const adj = new Map<string, Set<string>>();
  const of = (id: string) => {
    let set = adj.get(id);
    if (!set) {
      set = new Set();
      adj.set(id, set);
    }
    return set;
  };
  for (const l of links) {
    const src = of(l.src),
      dst = of(l.dst);
    src.add(l.dst);
    dst.add(l.src);
  }
  return adj;
}

// Folder anchors on a sunflower spiral, largest folder in the middle: stable as folders appear.
export function anchors(S: Pick<LiveState, 'nodes'>, toggles: Toggles) {
  const sizes = new Map<string, number>();
  for (const n of S.nodes.values())
    if (visibleNode(n, toggles))
      sizes.set(n.group, (sizes.get(n.group) ?? 0) + 1);
  const groups = [...sizes].toSorted((a, b) => b[1] - a[1]).map(([g]) => g);
  const spread =
    40 + 9 * Math.sqrt([...sizes.values()].reduce((a, b) => a + b, 0));
  return new Map(
    groups.map((g, i): [string, Point] => {
      const r = spread * Math.sqrt(i / Math.max(1, groups.length)) * 1.6,
        a = i * 2.39996;
      return [g, { x: r * Math.cos(a), y: r * Math.sin(a), z: 0 }];
    })
  );
}

// V: { pool, links, expanded } persists across calls; view: { overview, toggles, dims, anchors }.
export function buildView(
  V: ViewPool,
  S: OverviewStore,
  view: ViewOptions
): View {
  const { shown, drawn } = assign(V, S, view);
  place(V, S, drawn, view);
  labelFiles(drawn);
  const links = collectLinks(V, S, shown, view);
  const nodes = [...drawn.values()];
  return {
    nodes,
    links,
    shown,
    adj: adjacency(links),
    maxWeight: links.reduce((max, l) => Math.max(max, l.w), 1),
  };
}

// Drawn ids for store ids (a folded file maps to its community node).
export const drawnIds = (view: Pick<View, 'shown'>, ids: Iterable<string>) =>
  new Set(
    [...ids]
      .map(id => view.shown.get(id))
      .filter((id): id is string => Boolean(id))
  );

// Tooltips: a community's size, purity and weights with its first members; a file's place, metrics and check.
function communityTip(o: BucketNode) {
  const c = o.info;
  const head = `<b>${esc(o.label)}</b><div class="note">${o.size} files${o.folder ? ` · ${o.parsed} parsed` : ''}${c ? ` · ${Math.round(c.purity * 100)}% in ${esc(c.chunk)} · weight inside ${fmt(c.internal)} · to others ${fmt(c.external)}` : ''}</div>`;
  return `${head}<div class="mono">${esc(o.members.slice(0, 4).join(', '))}${o.size > 4 ? ` … +${o.size - 4}` : ''}</div><div class="note">click: expand in place</div>`;
}
// Label layout geometry: a label's screen box beside its projected node (below a community), and box overlap.
export function labelBox(
  o: DrawnNode,
  p: { x: number; y: number; r: number }
): Box {
  const comm = o.kind === 'community',
    w = (comm ? 7.4 : 6.4) * (o.labelText?.length ?? 8),
    h = comm ? 16 : 14;
  return comm
    ? { x: p.x - w / 2, y: p.y + p.r + 2, w, h }
    : { x: p.x + p.r + 3, y: p.y - h / 2, w, h };
}
export const overlaps = (a: Box, b: Box) =>
  a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

export function tipOf(o: DrawnNode, S: OverviewStore, V: ViewPool) {
  if (o.kind === 'community') return communityTip(o);
  const n = o.ref,
    m = n.metrics,
    N = S.metrics?.ids.length ?? 0;
  const stats = n.inGraph
    ? `${n.fanIn} importers · ${n.fanOut} imports${n.language ? ` · ${n.language}` : ''}`
    : 'listed by structureSearch; the graph parsed no code here';
  const label = m && V.communities?.get(m.community)?.label;
  const metrics = m
    ? `<div class="note">PageRank ${fmt(m.pagerank * N)}× mean · betweenness ${fmt(m.betweenness)} · depth ${m.depth} · k-core ${m.core}${label ? ` · ${esc(label)}` : ''}${m.cycle == null ? '' : ' · in a cycle'}</div>`
    : '';
  const check = n.check
    ? `<div class="note">check: ${esc(n.check.status)}</div>`
    : '';
  return `<b>${esc(o.label)}</b><div class="mono">${esc(n.manifest ?? n.path)}</div><div class="note">${stats}</div>${metrics}${check}${o.bucket ? '<div class="note">right-click: fold this group</div>' : ''}`;
}
