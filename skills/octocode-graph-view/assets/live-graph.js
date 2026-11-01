// WebGL code graph (3d-force-graph, global ForceGraph3D) that grows in place, in 2D or 3D. Drawn nodes and links
// are live-overview.js pool objects, never the store's, so positions persist across syncs; once metrics arrive the
// overview folds files into communities. One graph per page: G holds its state, the functions below share it.
/* global d3, ForceGraph3D */
import { REST, announceCommunity, anchors, bucketOf, buildView, drawnIds, labelBox, overlaps, tipOf } from './live-overview.js';

const SLOTS = 7;
// Above these drawn-link counts links render as 1px lines and without arrows: each cylinder or cone is a draw call.
const WIDE_LINKS = 2500;
const ARROW_LINKS = 1500;
// After growth the settled nodes keep a quarter of their velocity for a while, so new nodes find their place
// without shaking the rest of the layout (the library reheats to alpha 1 on every data change).
const CALM = 0.25;
const CALM_TICKS = 90;
const LAYER_GAP = 40;
const LABELS = 60;
let tokens = null;
const css = (name) => (tokens ??= getComputedStyle(document.documentElement)).getPropertyValue(name).trim();
const faded = new Map();
// WebGL has no dashes: link and node emphasis is carried by opacity, encoded in the color (rgba) per mark.
function fade(color, alpha) {
  const k = `${color}|${alpha}`;
  if (!faded.has(k)) { const c = d3.color(color); faded.set(k, c ? c.copy({ opacity: alpha }).formatRgb() : color); }
  return faded.get(k);
}

const G = {
  S: null, select: () => {}, fg: null, el: null, labelsEl: null, vec: null,
  V: { pool: new Map(), links: new Map(), expanded: new Set() },
  view: { nodes: [], links: [], shown: new Map(), adj: new Map(), maxWeight: 1 },
  anchors: new Map(), color: new Map(), picks: [], labelEls: new Map(),
  toggles: { imports: true, calls: true, weak: true, tests: true, docs: true, labels: true },
  colorBy: 'group', layout: 'force', dims: 2, overview: false, metricsSeen: false, minWeight: 0,
  simNodes: [], hover: null, focusId: null, keep: null, queued: false, styled: 0, autoFit: true, lastFit: 0, calm: 0, density: 1,
};

// Categorical slot of a key: folders use the plan-time slots; chunk and language keys take the next free slot in
// order of first appearance and keep it; past seven slots a key is "Other". Never cycled.
function slotOf(kind, key) {
  if (kind === 'group' && G.S.groupSlot.size) return G.S.groupSlot.get(key) ?? 0;
  const slots = G.color.get(kind) ?? new Map();
  G.color.set(kind, slots);
  if (!slots.has(key)) slots.set(key, slots.size < SLOTS ? slots.size + 1 : 0);
  return slots.get(key);
}
function colorOf(kind, key) {
  const slot = slotOf(kind, key);
  return css(slot ? `--s${slot}` : '--other');
}

const ACTIVE = new Set(['ingesting', 'paging']);
const STATUS = { stale: '--warning', missing: '--critical', lines: '--critical' };
const colorKey = (n) => (G.colorBy === 'chunk' ? n.chunk ?? n.planChunk ?? '-' : G.colorBy === 'language' ? n.language ?? '-' : n.group);
// Community ids are ranked by size, so the seven largest take slots 1-7; depth is a one-hue sequential ramp.
const communityColor = (id) => css(id != null && id < SLOTS ? `--s${id + 1}` : '--other');
function fillOf(n) {
  if (G.colorBy === 'community') return n.metrics ? communityColor(n.metrics.community) : css('--other');
  if (G.colorBy === 'depth') {
    const max = G.S.metrics?.stats.maxDepth || 1;
    return n.metrics ? d3.interpolateRgb(css('--seq-lo'), css('--seq-hi'))(n.metrics.depth / max) : css('--other');
  }
  return colorOf(G.colorBy, colorKey(n));
}
function baseColor(o) {
  if (o.kind === 'community') return o.community == null ? css('--mute') : communityColor(o.community);
  const n = o.ref;
  if (STATUS[n.check?.status]) return css(STATUS[n.check.status]);
  if (!n.inGraph) return css(ACTIVE.has(G.S.chunks.get(n.planChunk)?.state) ? '--acc' : '--mute');
  return fillOf(n);
}
function nodeColor(o) {
  const c = baseColor(o);
  if (G.keep) return G.keep.has(o.id) ? c : fade(c, 0.15);
  return o.kind === 'community' || o.ref.inGraph ? c : fade(c, 0.6);
}

// Node area follows weighted PageRank once metrics arrive (relative to the mean 1/n); before that, importers.
// A folded community sums its members' PageRank.
function radius(o) {
  const N = G.S.metrics?.ids.length;
  if (o.kind === 'community') return o.id === REST || !N ? 4 + Math.sqrt(o.size) / 2 : 3 + 3 * Math.log2(1 + o.pr * N);
  const n = o.ref;
  if (!n.inGraph) return 2.5;
  return n.metrics && N ? 3 + 3 * Math.log2(1 + n.metrics.pagerank * N) : 3.5 + 1.8 * Math.sqrt(n.fanIn);
}
const nodeVal = (o) => radius(o) ** 3;

const EDGE = { both: '--e-both', lsp: '--e-lsp', stitched: '--e-stitch' };
const linkVar = (l) => (l.via === 'stitched' ? EDGE.stitched : l.kind === 'imports' ? '--e-imp' : EDGE[l.via] ?? '--e-call');
const inKeep = (l) => G.keep.has(l.src) && G.keep.has(l.dst);
const touchesFocus = (l) => G.focusId != null && (l.src === G.focusId || l.dst === G.focusId);
// Thousands of overlapping links would hide every node: resting opacity falls with the drawn link count.
function linkColor(l) {
  const c = css(linkVar(l));
  if (G.keep) return fade(c, inKeep(l) ? 0.95 : 0.05);
  return fade(c, Math.round((l.weak ? 20 : 55) * G.density) / 100);
}
// Edge width follows the weight rule of graph-model.mjs (imports 1, evidence-weighted calls), log-scaled.
const linkWidth = (l) => (G.view.links.length > WIDE_LINKS ? 0 : 0.6 + 0.7 * Math.round(Math.log2(1 + l.w)));
const particles = (l) => (touchesFocus(l) ? 2 : 0);
const particleColor = (l) => css(linkVar(l));
const arrowLength = () => (G.view.links.length > ARROW_LINKS ? 0 : 3);
const linkShown = (l) => l.w >= G.minWeight;

// The hovered node (else the selection) and its neighbors stay bright, plus any highlighted set; the rest dims.
function focusSet() {
  const sel = G.S.selected ? G.view.shown.get(G.S.selected) : null;
  G.focusId = G.hover?.id ?? sel ?? null;
  if (G.focusId == null && !G.S.highlight.size) return null;
  const keep = drawnIds(G.view, G.S.highlight);
  for (const id of new Set([sel, G.focusId])) {
    if (id == null) continue;
    keep.add(id);
    for (const x of G.view.adj.get(id) ?? []) keep.add(x);
  }
  return keep;
}

const bigger = (a, b) => radius(b) - radius(a);
// Labels: the focus set, else every community and the biggest files.
function pickLabels() {
  if (!G.toggles.labels) return [];
  const sel = G.view.shown.get(G.S.selected);
  if (G.keep) return G.view.nodes.filter((o) => G.keep.has(o.id)).toSorted((a, b) => (b.id === sel) - (a.id === sel) || bigger(a, b)).slice(0, LABELS);
  const groups = G.view.nodes.filter((o) => o.kind === 'community').toSorted(bigger).slice(0, LABELS);
  const files = G.view.nodes.filter((o) => o.kind !== 'community' && o.ref.inGraph).toSorted(bigger).slice(0, groups.length ? 12 : 30);
  return [...groups, ...files];
}
function labelEl(o) {
  let el = G.labelEls.get(o.id);
  if (!el) { el = document.createElement('div'); G.labelEls.set(o.id, el); G.labelsEl.append(el); }
  el.className = `gv-label${o.kind === 'community' ? ' comm' : ''}${o.id === G.S.selected ? ' sel' : ''}`;
  o.labelText = o.kind === 'community' ? `${o.label} · ${o.size}` : o.label;
  el.textContent = o.labelText;
  return el;
}
function restyleLabels() {
  G.picks = pickLabels();
  const keepIds = new Set(G.picks.map((o) => o.id));
  for (const [id, el] of G.labelEls) if (!keepIds.has(id)) { el.remove(); G.labelEls.delete(id); }
  for (const o of G.picks) labelEl(o);
}
// Labels are HTML over the canvas, projected with the camera every frame (a few dozen at most); a label that
// would overlap a bigger node's label waits until the camera makes room.
function project(o, cam, size) {
  const v = G.vec.set(o.x, o.y, o.z ?? 0).project(cam);
  if (v.z > 1 || Math.abs(v.x) > 1 || Math.abs(v.y) > 1) return null;
  const x = ((v.x + 1) / 2) * size.w, y = ((1 - v.y) / 2) * size.h;
  const edge = G.vec.set(o.x, o.y + radius(o), o.z ?? 0).project(cam);
  return { x, y, r: Math.abs(y - ((1 - edge.y) / 2) * size.h) };
}
function placeLabels() {
  requestAnimationFrame(placeLabels);
  if (!G.picks.length || !G.fg) return;
  const cam = G.fg.camera(), size = { w: G.el.clientWidth, h: G.el.clientHeight }, boxes = [];
  G.vec ??= cam.position.clone();
  for (const o of G.picks) {
    const el = G.labelEls.get(o.id);
    if (!el) continue;
    const p = o.x == null ? null : project(o, cam, size);
    const box = p && labelBox(o, p);
    el.hidden = !box || boxes.some((b) => overlaps(b, box));
    if (el.hidden) continue;
    boxes.push(box);
    el.style.transform = `translate(${Math.round(box.x)}px, ${Math.round(box.y)}px)`;
  }
}

function restyle() {
  G.styled = 0;
  G.keep = focusSet();
  G.fg.nodeColor(nodeColor).linkColor(linkColor).linkDirectionalArrowColor(linkColor).linkDirectionalParticles(particles);
  restyleLabels();
}
function redraw() {
  if (!G.styled) G.styled = requestAnimationFrame(restyle);
}

const sameList = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);
// Store -> drawn objects. Unchanged membership only restyles; growth feeds the same objects back to the layout.
function rebuild() {
  G.queued = false;
  G.anchors = anchors(G.S, G.toggles);
  const prev = G.view;
  G.view = buildView(G.V, G.S, { overview: G.overview, toggles: G.toggles, dims: G.dims, anchors: G.anchors });
  document.getElementById('empty').hidden = G.view.nodes.length > 0;
  G.density = Math.min(1, Math.sqrt(1500 / Math.max(1, G.view.links.length)));
  G.fg.nodeVal(nodeVal);
  if (!sameList(prev.nodes, G.view.nodes) || !sameList(prev.links, G.view.links)) {
    const before = new Set(prev.nodes);
    for (const o of G.view.nodes) o.settled = before.has(o);
    G.calm = before.size ? CALM_TICKS : 0;
    G.fg.graphData({ nodes: G.view.nodes, links: G.view.links }).linkWidth(linkWidth).linkDirectionalArrowLength(arrowLength);
  }
  restyle();
  announce();
}
// The toolbar and legend (live-panels.js) follow graph state that canvas clicks change too.
function announce() {
  const state = `${G.overview}|${G.V.expanded.size}|${G.dims}`;
  if (state === G.announced) return;
  G.announced = state;
  document.dispatchEvent(new CustomEvent('gv:graph'));
}
// Overview turns on by itself once, when metrics first arrive (synchronously, so the panels render it this frame).
function sync() {
  if (G.S.metrics && !G.metricsSeen) { G.metricsSeen = true; G.overview = true; }
  if (G.queued) return;
  G.queued = true;
  requestAnimationFrame(rebuild);
}

// Communities drift toward the middle (a folded node without links stays in view); expanded files hold to their
// community's last place; other files to their folder's anchor; layers set y by depth (depth 0, which depends on
// nothing, at the bottom).
function anchorOf(o) {
  if (o.kind === 'community') return { x: 0, y: 0, z: 0, k: 0.02 };
  const b = o.bucket ? G.V.pool.get(o.bucket) : null;
  if (b?.x != null) return { x: b.x, y: b.y, z: b.z ?? 0, k: 0.08 };
  // A file with no drawn link (non-code, unparsed) has nothing else holding it: it stays close to its folder.
  const a = G.anchors.get(o.ref.group);
  return a ? { x: a.x, y: a.y, z: null, k: G.view.adj.has(o.id) ? 0.05 : 0.2 } : null;
}
const depthOf = (o) => (o.kind === 'community' ? (o.depthN ? o.depthSum / o.depthN : null) : o.ref.metrics?.depth ?? null);
function pull(o, alpha, layers) {
  const a = anchorOf(o);
  if (!a) return;
  o.vx += (a.x - o.x) * a.k * alpha;
  if (!layers) o.vy += (a.y - o.y) * a.k * alpha;
  if (G.dims === 3 && a.z != null) o.vz += (a.z - (o.z ?? 0)) * a.k * alpha;
}
function placeForce(alpha) {
  const layers = G.layout === 'layers' && G.S.metrics;
  const max = G.S.metrics?.stats.maxDepth ?? 0;
  for (const o of G.simNodes) {
    pull(o, alpha, layers);
    // Files without a depth (outside the graph) sit one gap above the top layer.
    if (layers) o.vy += (((depthOf(o) ?? max + 1) - max / 2) * LAYER_GAP - o.y) * 0.5 * alpha;
  }
}
function calmForce() {
  if (G.calm <= 0) return;
  G.calm--;
  for (const o of G.simNodes) if (o.settled) { o.vx *= CALM; o.vy *= CALM; o.vz = (o.vz ?? 0) * CALM; }
}
const sameGroup = (l) => l.source.ref && l.source.ref.group === l.target.ref?.group;
const linkDistance = (l) => radius(l.source) + radius(l.target) + (l.agg ? 40 : sameGroup(l) ? 20 : 60);
const linkStrength = (l) => (l.agg ? 0.08 : sameGroup(l) ? 0.35 : 0.06);
const charge = (o) => -20 - radius(o) ** 1.5;
// The simulation hands its node list to every force; the custom forces walk that list, not the next view.
function adopt(nodes) { G.simNodes = nodes; }
placeForce.initialize = adopt;
function forces(fg) {
  fg.d3Force('charge').strength(charge).distanceMax(600);
  fg.d3Force('link').distance(linkDistance).strength(linkStrength);
  fg.d3Force('place', placeForce);
  fg.d3Force('calm', calmForce);
}

// Camera: it follows the growing graph until the user moves it by hand.
function follow() {
  const now = performance.now();
  if (!G.autoFit || !G.view.nodes.length || now - G.lastFit < 1500) return;
  G.lastFit = now;
  G.fg.zoomToFit(500, 80);
}
function fit() { G.autoFit = true; G.lastFit = performance.now(); G.fg.zoomToFit(600, 80); }
function fly(o) {
  if (G.dims === 2) { G.fg.cameraPosition({ x: o.x, y: o.y, z: 320 }, { x: o.x, y: o.y, z: 0 }, 800); return; }
  const r = 1 + 180 / (Math.hypot(o.x, o.y, o.z ?? 0) || 1);
  G.fg.cameraPosition({ x: o.x * r, y: o.y * r, z: (o.z ?? 0) * r }, { x: o.x, y: o.y, z: o.z ?? 0 }, 1000);
}
// A file folded into a community is expanded first, so search and panel links always land on the file itself.
function focus(id) {
  const n = G.S.nodes.get(id);
  if (!n) return;
  G.autoFit = false;
  if (G.overview && G.S.metrics && G.view.shown.get(id) !== id) { G.V.expanded.add(bucketOf(G.V, G.S, n)); rebuild(); }
  const o = G.V.pool.get(id);
  if (o?.x != null && G.view.shown.get(id) === id) fly(o);
}
function focusAll(ids) {
  const set = drawnIds(G.view, ids);
  if (!set.size) return;
  G.autoFit = false;
  G.fg.zoomToFit(700, 60, (o) => set.has(o.id));
}

function controls2d() {
  const c = G.fg.controls();
  c.noRotate = G.dims === 2;
  c.mouseButtons = G.dims === 2 ? { LEFT: 2, MIDDLE: 1, RIGHT: 2 } : { LEFT: 0, MIDDLE: 1, RIGHT: 2 };
  if (G.dims !== 2) return;
  G.fg.camera().up.set(0, 1, 0);
  G.fg.cameraPosition({ x: 0, y: 0, z: 900 }, { x: 0, y: 0, z: 0 });
}
function setDimensions(d) {
  const dims = d === 3 ? 3 : 2;
  if (dims === G.dims) return G.dims;
  G.dims = dims;
  for (const o of G.V.pool.values()) {
    o.vz = 0;
    o.z = dims === 2 || o.x == null ? 0 : (Math.random() - 0.5) * 80;
  }
  controls2d();
  G.calm = 0;
  G.fg.numDimensions(dims);
  G.autoFit = true;
  G.lastFit = 0;
  announce();
  return G.dims;
}

// Tooltip HTML (live-overview.js) for the drawn node under the pointer.
const tooltip = (o) => tipOf(o, G.S, G.V);

function onClick(o) {
  if (o.kind === 'community') {
    G.V.expanded.add(o.id);
    sync();
    announceCommunity(o.id);
    return;
  }
  G.select(o.ref.id);
  if (G.dims === 3) { G.autoFit = false; fly(o); }
}
function onRightClick(o) {
  if (o.bucket && G.V.expanded.delete(o.bucket)) sync();
}
function onHover(o) {
  G.hover = o ?? null;
  G.el.style.cursor = o ? 'pointer' : '';
  redraw();
}
function onBackground() { G.select(null); }
function stopFollowing() { G.autoFit = false; }

function configure(fg) {
  return fg.nodeId('id').nodeRelSize(1).nodeVal(nodeVal).nodeResolution(12).nodeOpacity(1).nodeColor(nodeColor).nodeLabel(tooltip)
    .linkOpacity(1).linkColor(linkColor).linkWidth(linkWidth).linkResolution(4).linkVisibility(linkShown)
    .linkDirectionalParticles(particles).linkDirectionalParticleWidth(1.8).linkDirectionalParticleSpeed(0.008).linkDirectionalParticleColor(particleColor)
    .linkDirectionalArrowLength(arrowLength).linkDirectionalArrowRelPos(1).linkDirectionalArrowColor(linkColor)
    .backgroundColor(css('--surface')).showNavInfo(false).numDimensions(G.dims)
    .d3AlphaDecay(0.03).d3VelocityDecay(0.35).warmupTicks(0).cooldownTime(20000)
    .onNodeClick(onClick).onNodeRightClick(onRightClick).onNodeHover(onHover).onBackgroundClick(onBackground)
    .onEngineTick(follow).onEngineStop(follow);
}
function resize() {
  const r = G.el.getBoundingClientRect();
  if (r.width && r.height) G.fg.width(r.width).height(r.height);
}
function mount() {
  const stage = document.getElementById('stage');
  document.getElementById('canvas')?.remove();
  G.el = document.createElement('div');
  G.el.id = 'graph3d';
  G.labelsEl = document.createElement('div');
  G.labelsEl.className = 'gv-labels';
  stage.prepend(G.el, G.labelsEl);
}

export function createGraph(S, { select }) {
  Object.assign(G, { S, select });
  mount();
  G.fg = configure(new ForceGraph3D(G.el, { controlType: 'trackball' }));
  forces(G.fg);
  controls2d();
  G.fg.controls().addEventListener('start', stopFollowing);
  new ResizeObserver(resize).observe(G.el);
  resize();
  requestAnimationFrame(placeLabels);
  return {
    sync, redraw, fit, focus, focusAll, colorOf, setDimensions,
    repaint() { tokens = null; faded.clear(); G.fg.backgroundColor(css('--surface')); restyle(); },
    get toggles() { return G.toggles; },
    get colorBy() { return G.colorBy; },
    set colorBy(v) { G.colorBy = v; redraw(); },
    get layout() { return G.layout; },
    setLayout(name) { G.layout = name; G.calm = 0; G.autoFit = true; G.fg.d3ReheatSimulation(); },
    reheat() { G.calm = 0; G.fg.nodeVal(nodeVal).linkWidth(linkWidth); forces(G.fg); G.fg.d3ReheatSimulation(); },
    toggle(name) { G.toggles[name] = !G.toggles[name]; sync(); return G.toggles[name]; },
    get dimensions() { return G.dims; },
    get overview() { return G.overview; },
    get hasMetrics() { return Boolean(S.metrics); },
    setOverview(on) { G.metricsSeen = true; G.overview = Boolean(on && S.metrics); sync(); return G.overview; },
    collapseAll() { G.V.expanded.clear(); sync(); },
    get expanded() { return G.V.expanded.size; },
    get maxWeight() { return G.view.maxWeight; },
    // fraction in [0, 1] on a log scale of the heaviest drawn link; 0 shows every link.
    setMinWeight(fraction) {
      G.minWeight = fraction > 0 ? 2 ** (fraction * Math.log2(1 + G.view.maxWeight)) - 1 : 0;
      G.fg.linkVisibility(linkShown);
      return G.minWeight;
    },
  };
}
