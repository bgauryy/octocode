// Display model of the WebGL graph: which nodes and links are drawn, as persistent objects the layout owns.
// The store (live.js) is never mutated: every drawn node and link is a pool object keyed by its id, so positions
// survive every sync and the graph grows in place. Overview folds files into one node per community (size > 1),
// singletons into one "isolated files" node and files without metrics into one "outside the graph" node;
// links between folded nodes carry the summed weight, direction kept.
export const ISO = 'c:iso';
export const REST = 'c:rest';
const BUCKET_LABEL = { [ISO]: 'isolated files', [REST]: 'outside the graph' };

const endId = (x) => x?.id ?? x;
const strong = (e) => e.confidence === 'high' || e.via === 'both' || e.via === 'lsp';
export const visibleNode = (n, t) => t.docs || n.inGraph;
export const visibleEdge = (e, t) => t[e.kind] && (t.tests || !e.test) && (t.weak || strong(e));
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const fmt = (x) => (x == null ? '–' : Number.isInteger(x) || Math.abs(x) >= 100 ? Math.round(x).toLocaleString() : Number(x).toPrecision(3));
const jitter = (spread) => (Math.random() - 0.5) * spread;

// Communities that fold (size > 1), by id; cached per metrics event. New metrics renumber communities, so the
// expanded set starts over.
function communities(V, S) {
  if (V.metricsRef !== S.metrics) {
    if (V.metricsRef) V.expanded.clear();
    V.metricsRef = S.metrics;
    V.communities = new Map((S.metrics?.communities ?? []).filter((c) => c.size > 1).map((c) => [c.id, c]));
  }
  return V.communities;
}
// Other views (the wiki drawer) follow community clicks: a bucket id `c:<n>` names metrics community n; the
// isolated and outside-the-graph buckets name none.
export function announceCommunity(bucket) {
  const id = Number(String(bucket).slice(2));
  document.dispatchEvent(new CustomEvent('gv:community', { detail: { bucket, community: Number.isInteger(id) ? id : null } }));
}

export function bucketOf(V, S, n) {
  if (!n.metrics) return REST;
  return communities(V, S).has(n.metrics.community) ? `c:${n.metrics.community}` : ISO;
}

function bucketNode(V, S, id) {
  let b = V.pool.get(id);
  if (!b) { b = { id, kind: 'community' }; V.pool.set(id, b); }
  // A later metrics event (a refresh) may relabel or renumber communities: read them each time.
  const c = communities(V, S).get(Number(id.slice(2)));
  Object.assign(b, { community: c ? c.id : null, label: c?.label ?? BUCKET_LABEL[id], info: c ?? null });
  Object.assign(b, { size: 0, pr: 0, depthSum: 0, depthN: 0, members: [] });
  return b;
}
function fileNode(V, n) {
  let o = V.pool.get(n.id);
  if (!o) { o = { id: n.id, ref: n }; V.pool.set(n.id, o); }
  o.kind = n.kind;
  return o;
}

// A drawn node appears next to what it stands for: an expanded file at its community, a folded community at its
// members' centroid, any other file at its folder's centroid (or anchor).
function seed(o, at, view) {
  const spread = 12 + 2 * Math.sqrt(o.size ?? 1);
  const p = at ?? { x: 0, y: 0, z: 0 };
  o.x = p.x + jitter(spread);
  o.y = p.y + jitter(spread);
  o.z = view.dims === 3 ? (p.z ?? 0) + jitter(spread) : 0;
  o.vx = 0; o.vy = 0; o.vz = 0;
}
function centroids(list, keyOf) {
  const acc = new Map();
  for (const o of list) {
    if (o.x == null || !o.shown) continue;
    const k = keyOf(o), c = acc.get(k) ?? { x: 0, y: 0, z: 0, n: 0 };
    c.x += o.x; c.y += o.y; c.z += o.z ?? 0; c.n++;
    acc.set(k, c);
  }
  for (const c of acc.values()) { c.x /= c.n; c.y /= c.n; c.z /= c.n; }
  return acc;
}

// Which drawn id stands for each visible store node.
function assign(V, S, view) {
  const shown = new Map(), drawn = new Map();
  const overview = view.overview && S.metrics;
  for (const n of S.nodes.values()) {
    if (!visibleNode(n, view.toggles)) continue;
    const bucket = overview ? bucketOf(V, S, n) : null;
    const id = bucket && !V.expanded.has(bucket) ? bucket : n.id;
    shown.set(n.id, id);
    if (!drawn.has(id)) drawn.set(id, id === n.id ? fileNode(V, n) : bucketNode(V, S, id));
    const o = drawn.get(id);
    if (o.kind === 'community') addMember(o, n);
    else o.bucket = bucket;
  }
  return { shown, drawn };
}
function addMember(b, n) {
  b.size++;
  b.members.push(n.id);
  b.pr += n.metrics?.pagerank ?? 0;
  if (n.metrics) { b.depthSum += n.metrics.depth; b.depthN++; }
}

// Only nodes that were not drawn last time move: a community seeds at its members' centroid, a file at the
// community it unfolds from, else at its folder's centroid or anchor.
function seedNode(V, S, o, at) {
  if (o.kind === 'community') { seed(o, at.byBucket.get(o.id) ?? (o.x == null ? null : o), at.view); return; }
  const b = o.ref.metrics && S.metrics ? V.pool.get(bucketOf(V, S, o.ref)) : null;
  if (b?.x != null && (b.shown || o.bucket)) seed(o, b, at.view);
  else if (o.x == null) seed(o, at.byGroup.get(o.ref.group) ?? at.view.anchors.get(o.ref.group), at.view);
}
function place(V, S, drawn, view) {
  const prior = [...V.pool.values()].filter((o) => o.kind !== 'community' && o.shown);
  const at = {
    view,
    byGroup: centroids(prior, (o) => o.ref.group),
    byBucket: view.overview && S.metrics ? centroids(prior, (o) => bucketOf(V, S, o.ref)) : new Map(),
  };
  for (const o of drawn.values()) if (!o.shown || o.x == null) seedNode(V, S, o, at);
  for (const o of V.pool.values()) o.shown = drawn.has(o.id);
}

function linkObj(V, key, src, dst) {
  let l = V.links.get(key);
  if (!l) { l = { key, src, dst, source: src, target: dst }; V.links.set(key, l); }
  return l;
}
// File links are the store's edges; any link touching a folded node sums the weights it stands for.
function collectLinks(V, S, shown, view) {
  const links = new Map();
  for (const e of S.edges.values()) {
    if (!visibleEdge(e, view.toggles)) continue;
    const s = endId(e.source), t = endId(e.target), a = shown.get(s), b = shown.get(t);
    if (!a || !b || a === b) continue;
    const w = e.w ?? 1;
    if (a === s && b === t) {
      links.set(e.key, Object.assign(linkObj(V, e.key, a, b), { ref: e, agg: false, kind: e.kind, via: e.via, w, weak: !strong(e), count: 1 }));
      continue;
    }
    const key = `agg|${a}|${b}`;
    let l = links.get(key);
    if (!l) { l = Object.assign(linkObj(V, key, a, b), { agg: true, via: 'agg', w: 0, count: 0, weak: false, imports: 0, calls: 0 }); links.set(key, l); }
    l.w += w; l.count++; l[e.kind] += w;
    l.kind = l.imports >= l.calls ? 'imports' : 'calls';
  }
  return [...links.values()];
}

// Repeated file names are labelled with as many trailing path segments as it takes (up to five), so ten
// `index.ts` hubs stay distinguishable.
const tail = (o, k) => (o.kind === 'file' && k > 1 ? o.ref.path.split('/').slice(-k).join('/') : o.ref.label);
function labelFiles(drawn) {
  const files = [...drawn.values()].filter((o) => o.kind !== 'community');
  for (const o of files) o.label = o.ref.label;
  for (const k of [2, 3, 4, 5]) {
    const count = new Map();
    for (const o of files) count.set(o.label, (count.get(o.label) ?? 0) + 1);
    for (const o of files) if (count.get(o.label) > 1) o.label = tail(o, k);
  }
}

export function adjacency(links) {
  const adj = new Map();
  for (const l of links) {
    if (!adj.has(l.src)) adj.set(l.src, new Set());
    if (!adj.has(l.dst)) adj.set(l.dst, new Set());
    adj.get(l.src).add(l.dst);
    adj.get(l.dst).add(l.src);
  }
  return adj;
}

// Folder anchors on a sunflower spiral, largest folder in the middle: stable as folders appear.
export function anchors(S, toggles) {
  const sizes = new Map();
  for (const n of S.nodes.values()) if (visibleNode(n, toggles)) sizes.set(n.group, (sizes.get(n.group) ?? 0) + 1);
  const groups = [...sizes].toSorted((a, b) => b[1] - a[1]).map(([g]) => g);
  const spread = 40 + 9 * Math.sqrt([...sizes.values()].reduce((a, b) => a + b, 0));
  return new Map(groups.map((g, i) => {
    const r = spread * Math.sqrt(i / Math.max(1, groups.length)) * 1.6, a = i * 2.39996;
    return [g, { x: r * Math.cos(a), y: r * Math.sin(a), z: 0 }];
  }));
}

// V: { pool, links, expanded } persists across calls; view: { overview, toggles, dims, anchors }.
export function buildView(V, S, view) {
  const { shown, drawn } = assign(V, S, view);
  place(V, S, drawn, view);
  labelFiles(drawn);
  const links = collectLinks(V, S, shown, view);
  const nodes = [...drawn.values()];
  return { nodes, links, shown, adj: adjacency(links), maxWeight: Math.max(1, ...links.map((l) => l.w)) };
}

// Drawn ids for store ids (a folded file maps to its community node).
export const drawnIds = (view, ids) => new Set([...ids].map((id) => view.shown.get(id)).filter(Boolean));

// Tooltips: a community's size, purity and weights with its first members; a file's place, metrics and check.
function communityTip(o) {
  const c = o.info;
  const head = `<b>${esc(o.label)}</b><div class="note">${o.size} files${c ? ` · ${Math.round(c.purity * 100)}% in ${esc(c.chunk)} · weight inside ${fmt(c.internal)} · to others ${fmt(c.external)}` : ''}</div>`;
  return `${head}<div class="mono">${esc(o.members.slice(0, 4).join(', '))}${o.size > 4 ? ` … +${o.size - 4}` : ''}</div><div class="note">click: expand in place</div>`;
}
// Label layout geometry: a label's screen box beside its projected node (below a community), and box overlap.
export function labelBox(o, p) {
  const comm = o.kind === 'community', w = (comm ? 7.4 : 6.4) * (o.labelText?.length ?? 8), h = comm ? 16 : 14;
  return comm ? { x: p.x - w / 2, y: p.y + p.r + 2, w, h } : { x: p.x + p.r + 3, y: p.y - h / 2, w, h };
}
export const overlaps = (a, b) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

export function tipOf(o, S, V) {
  if (o.kind === 'community') return communityTip(o);
  const n = o.ref, m = n.metrics, N = S.metrics?.ids.length ?? 0;
  const stats = n.inGraph ? `${n.fanIn} importers · ${n.fanOut} imports${n.language ? ` · ${n.language}` : ''}` : 'listed by structureSearch; the graph parsed no code here';
  const label = m && V.communities?.get(m.community)?.label;
  const metrics = m ? `<div class="note">PageRank ${fmt(m.pagerank * N)}× mean · betweenness ${fmt(m.betweenness)} · depth ${m.depth} · k-core ${m.core}${label ? ` · ${esc(label)}` : ''}${m.cycle == null ? '' : ' · in a cycle'}</div>` : '';
  const check = n.check ? `<div class="note">check: ${esc(n.check.status)}</div>` : '';
  return `<b>${esc(o.label)}</b><div class="mono">${esc(n.manifest ?? n.path)}</div><div class="note">${stats}</div>${metrics}${check}${o.bucket ? '<div class="note">right-click: fold its community</div>' : ''}`;
}
