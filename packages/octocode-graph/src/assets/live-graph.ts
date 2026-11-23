// WebGL code graph (bundled 3d-force-graph) that grows in place, in 2D or 3D. Drawn nodes and links
// are live-overview.ts pool objects, never the store's, so positions persist across syncs; once metrics arrive the
// overview folds files into communities. One graph per page: G holds its state, the functions below share it.
import type { ForceGraph3DInstance } from '3d-force-graph';
import type { FileCheckStatus } from '../runtime/types.ts';
import {
  ForceGraph3D,
  color as parseColor,
  interpolateRgb,
  forceCollide,
} from './renderer.generated.js';
import {
  REST,
  announceCommunity,
  anchors,
  buildView,
  drawnIds,
  isPlaced,
  labelBox,
  overlaps,
  tipOf,
} from './live-overview.ts';
import type {
  Box,
  DrawnLink,
  DrawnNode,
  Placed,
  Point,
  Toggles,
  View,
  ViewPool,
} from './live-overview.ts';
import { byId } from './live-dom.ts';
import type {
  ChunkStatus,
  LiveState,
  SelectFn,
  ViewNode,
} from './live-state.ts';

type Graph = ForceGraph3DInstance<DrawnNode, DrawnLink>;
// A simulated node: d3-force-3d has given it a position and velocity (vz is read without a default only in 3D).
type SimNode = DrawnNode & Placed & { vx: number; vy: number; vz: number };
// A drawn link once the link force has replaced its endpoint ids with the drawn nodes.
type ResolvedLink = DrawnLink & { source: DrawnNode; target: DrawnNode };
// The three.js objects the labels and camera use (the bundle ships three; its typings are not installed).
interface Vec3 {
  x: number;
  y: number;
  z: number;
  set(x: number, y: number, z: number): Vec3;
  project(camera: Camera): Vec3;
  clone(): Vec3;
}
interface Camera {
  position: Vec3;
  up: Vec3;
}
// controlType 'trackball' is three's TrackballControls; the library types controls() as object.
interface Trackball {
  noRotate: boolean;
  mouseButtons: { LEFT: number; MIDDLE: number; RIGHT: number };
  addEventListener(type: 'start' | 'change', listener: () => void): void;
}
// The library's built-in d3-force-3d forces, as the layout configures them (d3Force() returns a loose type).
interface ChargeForce {
  (alpha: number): void;
  strength(value: (o: DrawnNode) => number): ChargeForce;
  distanceMax(distance: number): ChargeForce;
}
interface LinkForce {
  (alpha: number): void;
  distance(value: (l: ResolvedLink) => number): LinkForce;
  strength(value: (l: ResolvedLink) => number): LinkForce;
}
interface Anchor {
  x: number;
  y: number;
  z: number | null;
  k: number;
}

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
let tokens: CSSStyleDeclaration | null = null;
const css = (name: string) =>
  (tokens ??= getComputedStyle(document.documentElement))
    .getPropertyValue(name)
    .trim();
const faded = new Map<string, string>();
// WebGL has no dashes: link and node emphasis is carried by opacity, encoded in the color (rgba) per mark.
function fade(color: string, alpha: number) {
  const k = `${color}|${alpha}`;
  let out = faded.get(k);
  if (out === undefined) {
    const c = parseColor(color);
    out = c ? c.copy({ opacity: alpha }).formatRgb() : color;
    faded.set(k, out);
  }
  return out;
}

interface GraphState {
  S: LiveState;
  select: SelectFn;
  fg: Graph;
  el: HTMLDivElement;
  labelsEl: HTMLDivElement;
  vec: Vec3 | null;
  V: ViewPool;
  view: View;
  reveal: string | null;
  anchors: Map<string, Point>;
  color: Map<string, Map<string, number>>;
  picks: DrawnNode[];
  labelEls: Map<string, HTMLDivElement>;
  toggles: Toggles;
  colorBy: string;
  layout: string;
  dims: 2 | 3;
  overview: boolean;
  manualOverview: boolean;
  minWeight: number;
  simNodes: SimNode[];
  hover: DrawnNode | null;
  focusId: string | null;
  keep: Set<string> | null;
  queued: boolean;
  styled: number;
  autoFit: boolean;
  lastFit: number;
  calm: number;
  density: number;
  labelFrame?: number;
  announced?: string;
}
// createGraph supplies the store and the mounted renderer before any function below runs.
const unmounted: Omit<GraphState, 'S' | 'fg' | 'el' | 'labelsEl'> = {
  select: () => {},
  vec: null,
  V: { pool: new Map(), links: new Map(), expanded: new Set() },
  view: {
    nodes: [],
    links: [],
    shown: new Map(),
    adj: new Map(),
    maxWeight: 1,
  },
  reveal: null,
  anchors: new Map(),
  color: new Map(),
  picks: [],
  labelEls: new Map(),
  toggles: {
    imports: true,
    calls: true,
    weak: true,
    tests: true,
    docs: true,
    labels: true,
  },
  colorBy: 'group',
  layout: 'force',
  dims: 2,
  overview: false,
  manualOverview: false,
  minWeight: 0,
  simNodes: [],
  hover: null,
  focusId: null,
  keep: null,
  queued: false,
  styled: 0,
  autoFit: true,
  lastFit: 0,
  calm: 0,
  density: 1,
};
const G = unmounted as GraphState;

// Categorical slot of a key: folders use the plan-time slots; chunk and language keys take the next free slot in
// order of first appearance and keep it; past seven slots a key is "Other". Never cycled.
function slotOf(kind: string, key: string) {
  if (kind === 'group' && G.S.groupSlot.size)
    return G.S.groupSlot.get(key) ?? 0;
  const slots = G.color.get(kind) ?? new Map<string, number>();
  G.color.set(kind, slots);
  let slot = slots.get(key);
  if (slot === undefined) {
    slot = slots.size < SLOTS ? slots.size + 1 : 0;
    slots.set(key, slot);
  }
  return slot;
}
function colorOf(kind: string, key: string) {
  const slot = slotOf(kind, key);
  return css(slot ? `--s${slot}` : '--other');
}

const ACTIVE = new Set<ChunkStatus>(['ingesting', 'paging']);
const STATUS: Partial<Record<FileCheckStatus, string>> = {
  unverified: '--warning',
  stale: '--warning',
  missing: '--critical',
  lines: '--critical',
};
const colorKey = (n: ViewNode) =>
  G.colorBy === 'chunk'
    ? (n.chunk ?? n.planChunk ?? '-')
    : G.colorBy === 'language'
      ? (n.language ?? '-')
      : n.group;
// Community ids are ranked by size, so the seven largest take slots 1-7; depth is a one-hue sequential ramp.
const communityColor = (id: number | null) =>
  css(id != null && id < SLOTS ? `--s${id + 1}` : '--other');
function fillOf(n: ViewNode) {
  if (G.colorBy === 'community')
    return n.metrics ? communityColor(n.metrics.community) : css('--other');
  if (G.colorBy === 'depth') {
    const max = G.S.metrics?.stats.maxDepth || 1;
    return n.metrics
      ? interpolateRgb(css('--seq-lo'), css('--seq-hi'))(n.metrics.depth / max)
      : css('--other');
  }
  return colorOf(G.colorBy, colorKey(n));
}
function baseColor(o: DrawnNode) {
  if (o.kind === 'community' && o.folder)
    return css(o.parsed ? '--acc' : '--mute');
  if (o.kind === 'community')
    return o.community == null ? css('--mute') : communityColor(o.community);
  const n = o.ref;
  const status = n.check ? STATUS[n.check.status] : undefined;
  if (status) return css(status);
  if (!n.inGraph) {
    const chunk = n.planChunk == null ? undefined : G.S.chunks.get(n.planChunk);
    return css(chunk && ACTIVE.has(chunk.state) ? '--acc' : '--mute');
  }
  return fillOf(n);
}
function nodeColor(o: DrawnNode) {
  const c = baseColor(o);
  if (G.keep) return G.keep.has(o.id) ? c : fade(c, 0.15);
  return o.kind === 'community' || o.ref.inGraph ? c : fade(c, 0.6);
}

// Node area follows weighted PageRank once metrics arrive (relative to the mean 1/n); before that, importers.
// A folded community sums its members' PageRank.
function radius(o: DrawnNode) {
  const N = G.S.metrics?.ids.length;
  if (o.kind === 'community')
    return o.folder
      ? 4 + Math.sqrt(o.parsed) / 2
      : o.id === REST || !N
        ? 4 + Math.sqrt(o.size) / 2
        : 3 + 3 * Math.log2(1 + o.pr * N);
  const n = o.ref;
  if (!n.inGraph) return 2.5;
  return n.metrics && N
    ? 3 + 3 * Math.log2(1 + n.metrics.pagerank * N)
    : 3.5 + 1.8 * Math.sqrt(n.fanIn);
}
const nodeVal = (o: DrawnNode) => radius(o) ** 3;

const EDGE: Record<string, string> = {
  both: '--e-both',
  lsp: '--e-lsp',
  stitched: '--e-stitch',
};
const linkVar = (l: DrawnLink) =>
  l.via === 'stitched'
    ? EDGE.stitched
    : l.kind === 'imports'
      ? '--e-imp'
      : (EDGE[l.via] ?? '--e-call');
const inKeep = (l: DrawnLink) => G.keep?.has(l.src) && G.keep.has(l.dst);
const touchesFocus = (l: DrawnLink) =>
  G.focusId != null && (l.src === G.focusId || l.dst === G.focusId);
// Thousands of overlapping links would hide every node: resting opacity falls with the drawn link count.
function linkColor(l: DrawnLink) {
  const c = css(linkVar(l));
  if (G.keep) return fade(c, inKeep(l) ? 0.95 : 0.05);
  return fade(c, Math.round((l.weak ? 20 : 55) * G.density) / 100);
}
// Edge width follows the weight rule of graph-model.ts (imports 1, evidence-weighted calls), log-scaled.
const linkWidth = (l: DrawnLink) =>
  G.view.links.length > WIDE_LINKS
    ? 0
    : 0.6 + 0.7 * Math.round(Math.log2(1 + l.w));
const particles = (l: DrawnLink) => (touchesFocus(l) ? 2 : 0);
const particleColor = (l: DrawnLink) => css(linkVar(l));
const arrowLength = () => (G.view.links.length > ARROW_LINKS ? 0 : 3);
const linkShown = (l: DrawnLink) => l.w >= G.minWeight;

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

const bigger = (a: DrawnNode, b: DrawnNode) => radius(b) - radius(a);
// Labels: the focus set, else every community and the biggest files.
function pickLabels() {
  if (!G.toggles.labels) return [];
  const sel = G.S.selected == null ? undefined : G.view.shown.get(G.S.selected);
  const keep = G.keep;
  if (keep)
    return G.view.nodes
      .filter(o => keep.has(o.id))
      .toSorted(
        (a, b) => Number(b.id === sel) - Number(a.id === sel) || bigger(a, b)
      )
      .slice(0, LABELS);
  const groups = G.view.nodes
    .filter(o => o.kind === 'community')
    .toSorted(bigger)
    .slice(0, LABELS);
  const files = G.view.nodes
    .filter(o => o.kind !== 'community' && o.ref.inGraph)
    .toSorted(bigger)
    .slice(0, groups.length ? 12 : 30);
  return [...groups, ...files];
}
function labelEl(o: DrawnNode) {
  let el = G.labelEls.get(o.id);
  if (!el) {
    el = document.createElement('div');
    G.labelEls.set(o.id, el);
    G.labelsEl.append(el);
  }
  el.className = `gv-label${o.kind === 'community' ? ' comm' : ''}${o.id === G.S.selected ? ' sel' : ''}`;
  o.labelText =
    o.kind === 'community'
      ? `${o.label} · ${o.folder ? `${o.parsed}/` : ''}${o.size}`
      : o.label;
  el.textContent = o.labelText;
  return el;
}
function restyleLabels() {
  G.picks = pickLabels();
  const keepIds = new Set(G.picks.map(o => o.id));
  for (const [id, el] of G.labelEls)
    if (!keepIds.has(id)) {
      el.remove();
      G.labelEls.delete(id);
    }
  for (const o of G.picks) labelEl(o);
  scheduleLabels();
}
// Labels are HTML over the canvas, projected with the camera every frame (a few dozen at most); a label that
// would overlap a bigger node's label waits until the camera makes room.
function project(
  o: DrawnNode & Placed,
  cam: Camera,
  size: { w: number; h: number },
  vec: Vec3
) {
  const v = vec.set(o.x, o.y, o.z ?? 0).project(cam);
  if (v.z > 1 || Math.abs(v.x) > 1 || Math.abs(v.y) > 1) return null;
  const x = ((v.x + 1) / 2) * size.w,
    y = ((1 - v.y) / 2) * size.h;
  const edge = vec.set(o.x, o.y + radius(o), o.z ?? 0).project(cam);
  return { x, y, r: Math.abs(y - ((1 - edge.y) / 2) * size.h) };
}
function scheduleLabels() {
  if (!G.labelFrame) G.labelFrame = requestAnimationFrame(placeLabels);
}
function placeLabels() {
  G.labelFrame = 0;
  if (!G.picks.length || !G.fg) return;
  const cam: Camera = G.fg.camera(),
    size = { w: G.el.clientWidth, h: G.el.clientHeight },
    boxes: Box[] = [];
  const vec = (G.vec ??= cam.position.clone());
  for (const o of G.picks) {
    const el = G.labelEls.get(o.id);
    if (!el) continue;
    const p = isPlaced(o) ? project(o, cam, size, vec) : null;
    const box = p && labelBox(o, p);
    el.hidden = !box || boxes.some(b => overlaps(b, box));
    if (!box || el.hidden) continue;
    boxes.push(box);
    el.style.transform = `translate(${Math.round(box.x)}px, ${Math.round(box.y)}px)`;
  }
}

function restyle() {
  G.styled = 0;
  G.keep = focusSet();
  G.fg
    .nodeColor(nodeColor)
    .linkColor(linkColor)
    .linkDirectionalArrowColor(linkColor)
    .linkDirectionalParticles(particles);
  restyleLabels();
}
function redraw() {
  if (G.reveal && G.reveal !== G.S.selected) {
    G.reveal = null;
    sync();
  }
  if (!G.styled) G.styled = requestAnimationFrame(restyle);
}

const sameList = <T>(a: T[], b: T[]) =>
  a.length === b.length && a.every((x, i) => x === b[i]);
// Store -> drawn objects. Unchanged membership only restyles; growth feeds the same objects back to the layout.
function rebuild() {
  G.queued = false;
  const newMetrics = G.V.metricsRef !== G.S.metrics && Boolean(G.S.metrics);
  if (
    newMetrics &&
    G.S.selected &&
    G.view.shown.get(G.S.selected) === G.S.selected
  )
    G.reveal = G.S.selected;
  // Full-file anchors span the entire inventory. In overview the camera fits the drawn groups.
  G.anchors = G.overview ? new Map() : anchors(G.S, G.toggles);
  const prev = G.view;
  G.view = buildView(G.V, G.S, {
    overview: G.overview,
    reveal: G.reveal,
    toggles: G.toggles,
    dims: G.dims,
    anchors: G.anchors,
  });
  byId('empty').hidden = G.view.nodes.length > 0;
  G.density = Math.min(1, Math.sqrt(1500 / Math.max(1, G.view.links.length)));
  G.fg.nodeVal(nodeVal).linkWidth(linkWidth);
  configureSpacing();
  if (
    !sameList(prev.nodes, G.view.nodes) ||
    !sameList(prev.links, G.view.links)
  ) {
    const before = new Set(prev.nodes);
    for (const o of G.view.nodes) o.settled = before.has(o);
    G.calm = before.size ? CALM_TICKS : 0;
    G.fg
      .graphData({ nodes: G.view.nodes, links: G.view.links })
      .linkDirectionalArrowLength(arrowLength);
  } else if (newMetrics) {
    forces(G.fg);
    G.fg.d3ReheatSimulation();
  }
  if (G.hover && !G.hover.shown) G.hover = null;
  restyle();
  announce();
}
// The toolbar and legend (live-panels.ts) follow graph state that canvas clicks change too.
function announce() {
  const state = `${G.overview}|${G.V.expanded.size}|${Boolean(G.reveal)}|${G.dims}|${G.view.nodes.length}|${G.view.links.length}`;
  if (state === G.announced) return;
  G.announced = state;
  document.dispatchEvent(new CustomEvent('gv:graph'));
}
// Leave room to point at individual marks. Explicit file/overview choices survive later batches and metrics.
function sync() {
  if (!G.manualOverview && !G.overview) {
    const area = G.el.clientWidth * G.el.clientHeight;
    G.overview = Boolean(
      G.S.metrics || (area && G.S.nodes.size * 40 ** 2 > area)
    );
  }
  if (G.queued) return;
  G.queued = true;
  requestAnimationFrame(rebuild);
}

// Communities drift toward the middle (a folded node without links stays in view); expanded files hold to their
// community's last place; other files to their folder's anchor; layers set y by depth (depth 0, which depends on
// nothing, at the bottom).
const center: Anchor = { x: 0, y: 0, z: 0, k: 0.02 };
function anchorOf(o: SimNode): Anchor | null {
  if (o.kind === 'community') return center;
  const b = o.bucket ? G.V.pool.get(o.bucket) : null;
  if (b && isPlaced(b)) return { x: b.x, y: b.y, z: b.z ?? 0, k: 0.08 };
  if (G.overview) return center;
  // A file with no drawn link (non-code, unparsed) has nothing else holding it: it stays close to its folder.
  const a = G.anchors.get(o.ref.group);
  return a
    ? { x: a.x, y: a.y, z: null, k: G.view.adj.has(o.id) ? 0.05 : 0.2 }
    : null;
}
const depthOf = (o: DrawnNode) =>
  o.kind === 'community'
    ? o.depthN
      ? o.depthSum / o.depthN
      : null
    : (o.ref.metrics?.depth ?? null);
function pull(o: SimNode, alpha: number, layers: boolean) {
  const a = anchorOf(o);
  if (!a) return;
  o.vx += (a.x - o.x) * a.k * alpha;
  if (!layers) o.vy += (a.y - o.y) * a.k * alpha;
  if (G.dims === 3 && a.z != null) o.vz += (a.z - (o.z ?? 0)) * a.k * alpha;
}
function placeForce(alpha: number) {
  const layers = G.layout === 'layers' && Boolean(G.S.metrics);
  const max = G.S.metrics?.stats.maxDepth ?? 0;
  for (const o of G.simNodes) {
    pull(o, alpha, layers);
    // Files without a depth (outside the graph) sit one gap above the top layer.
    if (layers)
      o.vy +=
        (((depthOf(o) ?? max + 1) - max / 2) * LAYER_GAP - o.y) * 0.5 * alpha;
  }
}
function calmForce() {
  if (G.calm <= 0) return;
  G.calm--;
  for (const o of G.simNodes)
    if (o.settled) {
      o.vx *= CALM;
      o.vy *= CALM;
      o.vz = (o.vz ?? 0) * CALM;
    }
}
const sameGroup = (l: ResolvedLink) =>
  l.source.ref && l.source.ref.group === l.target.ref?.group;
const linkDistance = (l: ResolvedLink) =>
  radius(l.source) + radius(l.target) + (l.agg ? 40 : sameGroup(l) ? 20 : 60);
const linkStrength = (l: ResolvedLink) =>
  l.agg ? 0.08 : sameGroup(l) ? 0.35 : 0.06;
const charge = (o: DrawnNode) => -20 - radius(o) ** 1.5;
// The simulation hands its node list to every force; the custom forces walk that list, not the next view.
function adopt(nodes: DrawnNode[]) {
  // d3-force-3d initializes every node's position and velocity before any force runs.
  G.simNodes = nodes as SimNode[];
}
placeForce.initialize = adopt;
// Folded circles remain individually selectable even where many communities are connected.
function configureSpacing() {
  const collide =
    G.overview && G.dims === 2
      ? forceCollide((o: DrawnNode) => radius(o) + 4)
      : null;
  G.fg.d3Force('spacing', collide);
}
function forces(fg: Graph) {
  (fg.d3Force('charge') as ChargeForce).strength(charge).distanceMax(600);
  (fg.d3Force('link') as LinkForce)
    .distance(linkDistance)
    .strength(linkStrength);
  fg.d3Force('place', placeForce);
  fg.d3Force('calm', calmForce);
}

// Camera: it follows the growing graph until the user moves it by hand.
function follow() {
  scheduleLabels();
  const now = performance.now();
  if (!G.autoFit || !G.view.nodes.length || now - G.lastFit < 1500) return;
  G.lastFit = now;
  G.fg.zoomToFit(500, 80);
}
function fit() {
  G.autoFit = true;
  G.lastFit = performance.now();
  G.fg.zoomToFit(600, 80);
}
function fly(o: Placed) {
  if (G.dims === 2) {
    G.fg.cameraPosition(
      { x: o.x, y: o.y, z: 320 },
      { x: o.x, y: o.y, z: 0 },
      800
    );
    return;
  }
  const r = 1 + 180 / (Math.hypot(o.x, o.y, o.z ?? 0) || 1);
  G.fg.cameraPosition(
    { x: o.x * r, y: o.y * r, z: (o.z ?? 0) * r },
    { x: o.x, y: o.y, z: o.z ?? 0 },
    1000
  );
}
// Reveal only the selected file. A search inside a large folded group keeps its peers folded.
function focus(id: string) {
  const n = G.S.nodes.get(id);
  if (!n) return;
  G.autoFit = false;
  if (G.overview && G.view.shown.has(id) && G.view.shown.get(id) !== id) {
    G.reveal = id;
    rebuild();
  }
  const o = G.V.pool.get(id);
  if (o && isPlaced(o) && G.view.shown.get(id) === id) fly(o);
}
function focusAll(ids: string[]) {
  const set = drawnIds(G.view, ids);
  if (!set.size) return;
  G.autoFit = false;
  G.fg.zoomToFit(700, 60, o => set.has(o.id));
}

function controls2d() {
  const c = G.fg.controls() as Trackball;
  c.noRotate = G.dims === 2;
  c.mouseButtons =
    G.dims === 2
      ? { LEFT: 2, MIDDLE: 1, RIGHT: 2 }
      : { LEFT: 0, MIDDLE: 1, RIGHT: 2 };
  if (G.dims !== 2) return;
  (G.fg.camera() as Camera).up.set(0, 1, 0);
  G.fg.cameraPosition({ x: 0, y: 0, z: 900 }, { x: 0, y: 0, z: 0 });
}
function setDimensions(d: number) {
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
  configureSpacing();
  G.autoFit = true;
  G.lastFit = 0;
  announce();
  return G.dims;
}

// Tooltip HTML (live-overview.ts) for the drawn node under the pointer.
const tooltip = (o: DrawnNode) => tipOf(o, G.S, G.V);

function onClick(o: DrawnNode) {
  if (o.kind === 'community') {
    G.V.expanded.add(o.id);
    sync();
    if (!o.folder) announceCommunity(o.id);
    return;
  }
  G.select(o.ref.id);
  if (G.dims === 3) {
    G.autoFit = false;
    if (isPlaced(o)) fly(o);
  }
}
function onRightClick(o: DrawnNode) {
  if (!o.bucket) return;
  if (G.reveal === o.id) G.reveal = null;
  G.V.expanded.delete(o.bucket);
  sync();
}
function onHover(o: DrawnNode | null) {
  G.hover = o ?? null;
  G.el.style.cursor = o ? 'pointer' : '';
  redraw();
}
function onBackground() {
  G.select(null);
}
function stopFollowing() {
  G.autoFit = false;
}

function configure(fg: Graph) {
  return fg
    .nodeId('id')
    .nodeRelSize(1)
    .nodeVal(nodeVal)
    .nodeResolution(12)
    .nodeOpacity(1)
    .nodeColor(nodeColor)
    .nodeLabel(tooltip)
    .linkOpacity(1)
    .linkColor(linkColor)
    .linkWidth(linkWidth)
    .linkResolution(4)
    .linkVisibility(linkShown)
    .linkDirectionalParticles(particles)
    .linkDirectionalParticleWidth(1.8)
    .linkDirectionalParticleSpeed(0.008)
    .linkDirectionalParticleColor(particleColor)
    .linkDirectionalArrowLength(arrowLength)
    .linkDirectionalArrowRelPos(1)
    .linkDirectionalArrowColor(linkColor)
    .backgroundColor(css('--surface'))
    .showNavInfo(false)
    .numDimensions(G.dims)
    .d3AlphaDecay(0.03)
    .d3VelocityDecay(0.35)
    .warmupTicks(0)
    .cooldownTime(20000)
    .onNodeClick(onClick)
    .onNodeRightClick(onRightClick)
    .onNodeHover(onHover)
    .onBackgroundClick(onBackground)
    .onEngineTick(follow)
    .onEngineStop(follow);
}
function resize() {
  const r = G.el.getBoundingClientRect();
  if (r.width && r.height) G.fg.width(r.width).height(r.height);
  scheduleLabels();
}
function mount() {
  const stage = byId('viewport');
  G.el = document.createElement('div');
  G.el.id = 'graph3d';
  G.labelsEl = document.createElement('div');
  G.labelsEl.className = 'gv-labels';
  stage.prepend(G.el, G.labelsEl);
}

export type GraphApi = ReturnType<typeof createGraph>;
export function createGraph(S: LiveState, { select }: { select: SelectFn }) {
  Object.assign(G, { S, select });
  mount();
  // The library's constructor is not generic: this graph draws the pool nodes and links of live-overview.ts.
  G.fg = configure(
    new ForceGraph3D(G.el, { controlType: 'trackball' }) as unknown as Graph
  );
  forces(G.fg);
  controls2d();
  const controls = G.fg.controls() as Trackball;
  controls.addEventListener('start', stopFollowing);
  controls.addEventListener('change', scheduleLabels);
  new ResizeObserver(resize).observe(G.el);
  resize();
  return {
    sync,
    redraw,
    fit,
    focus,
    focusAll,
    colorOf,
    setDimensions,
    repaint() {
      tokens = null;
      faded.clear();
      G.fg.backgroundColor(css('--surface'));
      restyle();
    },
    get toggles() {
      return G.toggles;
    },
    get colorBy() {
      return G.colorBy;
    },
    set colorBy(v: string) {
      G.colorBy = v;
      redraw();
    },
    get layout() {
      return G.layout;
    },
    setLayout(name: string) {
      G.layout = name;
      G.calm = 0;
      G.autoFit = true;
      G.fg.d3ReheatSimulation();
    },
    toggle(name: keyof Toggles) {
      G.toggles[name] = !G.toggles[name];
      sync();
      return G.toggles[name];
    },
    get dimensions() {
      return G.dims;
    },
    get overview() {
      return G.overview;
    },
    get drawnNodes() {
      return G.view.nodes.length;
    },
    get drawnLinks() {
      return G.view.links.length;
    },
    get hasMetrics() {
      return Boolean(S.metrics);
    },
    setOverview(on: boolean) {
      G.manualOverview = true;
      G.overview = on;
      sync();
      return G.overview;
    },
    collapseAll() {
      G.reveal = null;
      G.V.expanded.clear();
      sync();
    },
    get expanded() {
      return G.V.expanded.size + Number(Boolean(G.reveal));
    },
    get maxWeight() {
      return G.view.maxWeight;
    },
    // fraction in [0, 1] on a log scale of the heaviest drawn link; 0 shows every link.
    setMinWeight(fraction: number) {
      G.minWeight =
        fraction > 0
          ? 2 ** (fraction * Math.log2(1 + G.view.maxWeight)) - 1
          : 0;
      G.fg.linkVisibility(linkShown);
      return G.minWeight;
    },
  };
}
