// Header, phases, toolbar, search and the four side panels. Lists page in place ("Show more"); nothing is dropped.
// One panel set per page: P holds its state, the functions below share it.
import type {
  CallStats,
  Community,
  CycleSet,
  Declaration,
  FileCheck,
  FileMetrics,
  PackageCoupling,
  FileCheckStatus,
} from '../runtime/types.ts';
import { byId as $, closest, find } from './live-dom.ts';
import type { GraphApi } from './live-graph.ts';
import type { Toggles } from './live-overview.ts';
import type {
  AskApi,
  AstItem,
  ChunkState,
  FeedLevel,
  HighlightFn,
  LiveState,
  LspRequest,
  LspResult,
  MetricsPayload,
  SelectFn,
  ToolCall,
  ToolSummary,
  ViewEdge,
  ViewNode,
} from './live-state.ts';

type Text = string | number | null | undefined;
const HTML: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};
const esc = (s: Text) => String(s ?? '').replace(/[&<>"']/g, c => HTML[c]);
const attr = (v: unknown) => esc(JSON.stringify(v));
const num = (n: number | null | undefined) =>
  n == null ? '–' : n.toLocaleString();
const secs = (ms: number | null | undefined) =>
  ms == null ? '' : ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
const kb = (b: number | null | undefined) =>
  b == null
    ? ''
    : b < 1024
      ? `${b} B`
      : b < 1 << 20
        ? `${(b / 1024).toFixed(1)} KB`
        : `${(b / (1 << 20)).toFixed(1)} MB`;
const nodeLink = (id: string) =>
  `<span class="k mono" data-select="${esc(id)}">${esc(id)}</span>`;
const bar = (pct: number, color: string) =>
  `<div class="hbar"><i style="width:${pct}%;background:var(${color})"></i></div>`;
const PHASES = [
  ['list', 'Files'],
  ['plan', 'Plan'],
  ['graph', 'Graph'],
  ['stitch', 'Stitch'],
  ['lsp', 'LSP'],
  ['verify', 'Verify'],
  ['metrics', 'Metrics'],
];
const WEIGHT: Record<string, number> = {
  list: 5,
  plan: 2,
  graph: 55,
  stitch: 4,
  lsp: 22,
  verify: 8,
  metrics: 4,
};
const PAGE = {
  checks: 25,
  cycles: 15,
  communities: 15,
  ranks: 25,
  packages: 20,
  tools: 100,
  edges: 40,
  functions: 60,
  chunks: 60,
};
type PageName = keyof typeof PAGE;
const isPage = (name: string): name is PageName => Object.hasOwn(PAGE, name);
const EDITORS: Record<string, (abs: string, line: number) => string> = {
  vscode: (abs, line) => `vscode://file${abs}:${line}`,
  cursor: (abs, line) => `cursor://file${abs}:${line}`,
  idea: (abs, line) =>
    `idea://open?file=${encodeURIComponent(abs)}&line=${line}`,
  file: abs => `file://${abs}`,
};
type RankKey = Exclude<keyof FileMetrics, 'community' | 'cycle'>;
type TabName = 'progress' | 'metrics' | 'tools' | 'node';
interface Hit {
  id: string;
  text: string;
  note: string;
}
interface PanelDeps {
  select: SelectFn;
  highlight: HighlightFn;
  ask: AskApi;
  graph: GraphApi;
}
interface PanelState extends PanelDeps {
  S: LiveState;
  tab: TabName;
  timer: number;
  shown: Map<PageName, number>;
  expanded: Set<string>;
  editor: string;
  toolFilter: string;
  errorsOnly: boolean;
  hits: Hit[];
  cursor: number;
  rankBy: RankKey;
}

// createPanels supplies the store and the viewer's callbacks before any function below runs.
const unbound: Omit<PanelState, 'S' | keyof PanelDeps> = {
  tab: 'progress',
  timer: 0,
  shown: new Map(),
  expanded: new Set(),
  editor: 'vscode',
  toolFilter: '',
  errorsOnly: false,
  hits: [],
  cursor: 0,
  rankBy: 'pagerank',
};
const P = unbound as PanelState;

const limit = (name: PageName) => P.shown.get(name) ?? PAGE[name];
const more = (name: PageName, total: number) =>
  total > limit(name)
    ? `<button class="more" data-more="${name}">Show ${Math.min(PAGE[name], total - limit(name))} more of ${num(total - limit(name))}</button>`
    : '';
const abs = (path: string) => `${P.S.root}/${path}`;
const href = (path: string, line = 1) => EDITORS[P.editor](abs(path), line);
const at = (path: string, line: number) =>
  `<a href="${esc(href(path, line))}" class="mono" title="open ${esc(path)}:${line} in the editor">${esc(path)}:${line}</a>`;
const lineLinks = (path: string, lines: Iterable<number>) =>
  [...lines]
    .toSorted((a, b) => a - b)
    .map(l => `<a href="${esc(href(path, l))}">${l}</a>`)
    .join(', ');
function serverNow() {
  const S = P.S;
  if (S.done) return S.done.ms;
  return S.error
    ? S.t
    : S.t + (performance.now() - (S.clockAt ?? performance.now()));
}

function counters() {
  const S = P.S;
  const inGraph = [...S.nodes.values()].filter(n => n.inGraph).length;
  const imports = [...S.edges.values()].filter(
    e => e.kind === 'imports'
  ).length;
  const declarations = [...S.symbols.values()].reduce(
    (a, l) => a + l.length,
    0
  );
  const v = S.verification;
  return [
    [`${num(inGraph)}/${num(S.nodes.size)}`, 'files'],
    ...(v ? [[`${num(v.ok)}/${num(v.checked)}`, 'match disk']] : []),
    [num(imports), 'imports'],
    [num(S.calls.size), 'calls'],
    [num(declarations), 'declarations'],
    [num(S.tools.length), 'tool calls'],
    [secs(Math.round(serverNow())), S.done ? 'total' : 'elapsed'],
  ]
    .map(
      ([value, k]) =>
        `<span class="counter"><b>${value}</b><span>${k}</span></span>`
    )
    .join('');
}

function header() {
  const S = P.S;
  $('title').textContent = S.run?.name ?? 'Graph live';
  $('rootPath').textContent = S.root;
  $('counters').innerHTML = counters();
  let pct = 0;
  $('phases').innerHTML = PHASES.map(([id, label]) => {
    const p = S.phases.get(id),
      pr = S.progress.get(id);
    const st = p?.state ?? 'pending';
    pct +=
      WEIGHT[id] *
      (st === 'done' || st === 'skipped'
        ? 1
        : pr?.total
          ? pr.done / pr.total
          : 0);
    return `<span class="phase ${st}" title="${esc(p?.note ?? st)}"><span class="ic"></span>${label}${pr?.total ? ` ${pr.done}/${pr.total}` : ''}${p?.ms != null ? ` · ${secs(p.ms)}` : ''}</span>`;
  }).join('');
  $('overall').style.width = `${S.done ? 100 : Math.min(99, pct)}%`;
  $('bar').classList.toggle('done', Boolean(S.done || S.error));
  legend();
  $('cancel', HTMLButtonElement).disabled = Boolean(S.done || S.error);
}

const EDGE_KEY = [
  ['--e-imp', 'import'],
  ['--e-call', 'call · graph'],
  ['--e-both', 'call · graph + LSP'],
  ['--e-lsp', 'call · LSP only'],
  ['--e-stitch', 'between chunks'],
];
// Legend: the active color mode (seven slots plus Other, or the depth ramp), link kinds by color with weak links
// fainter (WebGL draws no dashes), node states, and the overview gestures. It also keeps the toolbar state current.
function colorLegend(S: LiveState) {
  if (P.graph.colorBy === 'depth')
    return `<span>depth 0 (depends on nothing)</span><i style="display:inline-block;width:120px;height:10px;border-radius:5px;background:linear-gradient(90deg,var(--seq-lo),var(--seq-hi))"></i><span>${S.metrics?.stats.maxDepth ?? '…'}</span>`;
  if (P.graph.colorBy !== 'community') return null;
  const top = (S.metrics?.communities ?? []).slice(0, 7);
  return `${top.map(c => `<span><i class="c" style="background:var(--s${c.id + 1})"></i>${c.size} · ${esc(c.label)}</span>`).join('')}<span><i class="c" style="background:var(--other)"></i>Other</span>`;
}

function legend() {
  const S = P.S,
    g = P.graph;
  const mode = g.overview
    ? g.hasMetrics
      ? 'Community view'
      : 'Folder view'
    : 'File view';
  $('viewSummary').textContent =
    `${mode} · ${num(g.drawnNodes)} nodes · ${num(g.drawnLinks)} links`;
  const folders =
    g.overview && !g.hasMetrics
      ? '<span>Folder labels show parsed / listed files</span>'
      : (colorLegend(S) ??
        (g.colorBy === 'group'
          ? [...S.groupSlot]
              .map(
                ([k, i]) =>
                  `<span><i class="c" style="background:var(--s${i})"></i>${esc(k === '.' ? 'root files' : k)}</span>`
              )
              .join('') +
            (S.groupSlot.size
              ? '<span><i class="c" style="background:var(--other)"></i>Other</span>'
              : '')
          : `<span class="note">color: ${esc(g.colorBy)} (first seven, then Other)</span>`));
  const edges = `${EDGE_KEY.map(([v, l]) => `<span><i class="l" style="border-top:2px solid var(${v})"></i>${l}</span>`).join('')}<span><i class="l" style="border-top:2px solid var(--e-call);opacity:.35"></i>weak / low confidence</span>`;
  const states =
    '<span><i class="c" style="background:var(--warning)"></i>changed since snapshot</span><span><i class="c" style="background:var(--critical)"></i>missing / line outside file</span><span><i class="c" style="background:var(--acc)"></i>being ingested</span><span><i class="c" style="background:var(--mute);opacity:.6"></i>listed, not parsed</span>';
  const group = g.hasMetrics
    ? 'Communities sized by PageRank'
    : 'Folder size grows as files are parsed';
  const overview = g.overview
    ? `<div class="row note">${group} · click to expand · right-click a file to fold</div>`
    : '';
  $('legend').innerHTML =
    `<div class="row">${folders}</div><div class="row">${edges}</div><div class="row">${states}</div>${overview}`;
  const toggles = $('toggles');
  const [overviewButton, collapseButton, dimsButton] = [
    'overview',
    'collapse',
    'dims',
  ].map(k => toggles.querySelector(`[data-${k}]`));
  if (
    !overviewButton ||
    !(collapseButton instanceof HTMLButtonElement) ||
    !dimsButton
  )
    return;
  overviewButton.classList.toggle('on', g.overview);
  overviewButton.textContent = g.hasMetrics ? 'Communities' : 'Folders';
  overviewButton.setAttribute('aria-pressed', String(g.overview));
  collapseButton.disabled = !g.overview || !g.expanded;
  dimsButton.classList.toggle('on', g.dimensions === 3);
}

function chunkRow(c: ChunkState, now: number) {
  const time =
    c.state === 'done' || c.state === 'error'
      ? secs(c.ms)
      : c.startT != null
        ? secs(Math.round(now - c.startT))
        : '';
  const rows = Object.values(c.rows ?? {}).reduce((a, v) => a + v, 0);
  const name = c.path === '.' ? (P.S.run?.name ?? '.') : c.path;
  const counts = c.counts
    ? `<div class="note">${num(c.counts.imports)} imports · ${num(c.counts.calls)} calls · ${num(c.counts.declarations)} declarations</div>`
    : c.state === 'ingesting' && c.build
      ? `<div class="note">${esc(c.build)}</div>`
      : '';
  return `<tr><td><span class="state ${c.state}"></span><span class="mono">${esc(name)}${c.loose ? ' (files outside packages)' : ''}</span>${c.package ? ` <span class="badge">${esc(c.package)}</span>` : ''}${c.error ? `<div class="bad note">${esc(c.error)}</div>` : ''}${counts}</td><td class="n">${num(c.files)}</td><td class="n">${num(rows)}</td><td class="n">${time}</td></tr>`;
}

function progressTab() {
  const S = P.S;
  const chunks = [...S.chunks.values()].filter(c => c.state !== 'split');
  const order: Partial<Record<ChunkState['state'], number>> = {
    ingesting: 0,
    paging: 0,
    queued: 1,
    error: 2,
    done: 3,
  };
  const rows = chunks.toSorted(
    (a, b) => (order[a.state] ?? 4) - (order[b.state] ?? 4) || b.files - a.files
  );
  const split = [...S.chunks.values()].filter(c => c.state === 'split');
  const phases = PHASES.map(([id, label]) => {
    const p = S.phases.get(id);
    return `<tr><td><span class="state ${p?.state === 'running' ? 'start' : (p?.state ?? '')}"></span>${label}</td><td class="note">${esc(p?.note ?? p?.state ?? 'pending')}</td><td class="n">${secs(p?.ms)}</td></tr>`;
  }).join('');
  const st = S.stitched;
  return `<h2>Phases</h2><table>${phases}</table>
    <h2>Chunks <span>${chunks.filter(c => c.state === 'done').length}/${chunks.length} built</span></h2>
    <table><tr><th>chunk</th><th class="n">files</th><th class="n">rows</th><th class="n">time</th></tr>${rows
      .slice(0, limit('chunks'))
      .map(c => chunkRow(c, serverNow()))
      .join('')}</table>${more('chunks', rows.length)}
    ${split.length ? `<h2>Split on scopeTooBroad</h2>${split.map(c => `<div class="note mono">${esc(c.path)}</div>`).join('')}` : ''}
    ${st ? `<h2>Stitch</h2><div class="note">${num(st.imports.length)} imports and ${num(st.calls.length)} calls between chunks · ${num(st.unresolved.length)} out-of-root imports unresolved · ${num(st.uncovered.length)} files in no chunk</div>` : ''}
    ${S.notes.length ? `<h2>Notes</h2>${S.notes.map(n => `<div class="note ${n.level}">${esc(n.text)}</div>`).join('')}` : ''}
    ${S.done ? `<h2>Result</h2><div class="note mono">${esc(S.done.data)}</div><div class="note mono">${esc(S.done.brief)}</div>` : ''}${S.error ? `<h2>Error</h2><div class="bad">${esc(S.error.message)}</div>` : ''}`;
}

function lspBlock(L: CallStats | null) {
  if (!L) return '';
  if (!L.probed && L.lsp !== 'pending')
    return `<h2>Graph vs LSP calls</h2><div class="note">${esc(L.lsp)}</div>`;
  // Every probing state (lsp.ts, pipeline.ts) carries the counts.
  const { probed = 0, resolved = 0, both = 0, graphOnly = 0, lspOnly = 0 } = L;
  const t = Math.max(1, both + graphOnly + lspOnly),
    w = (v: number) => (100 * v) / t;
  return `<h2>Graph vs LSP calls <span>${probed} probed${L.lsp === 'pending' ? ' · confirmation pending' : ''}</span></h2><div class="hbar" style="height:12px"><i style="width:${w(both)}%;background:var(--both)"></i><i style="width:${w(graphOnly)}%;background:var(--call)"></i><i style="width:${w(lspOnly)}%;background:var(--lsp)"></i></div><div class="note">${both} both · ${graphOnly} graph only · ${lspOnly} LSP only · ${resolved}/${probed} fully answered${L.partialWithMatches ? ` · ${L.partialWithMatches} partial with confirmed matches` : ''}</div>`;
}

function verifyBlock(S: LiveState) {
  const v = S.verification;
  if (!v) return '';
  const bad = [...S.nodes.values()].filter(
    (n): n is ViewNode & { check: FileCheck } =>
      Boolean(n.check && n.check.status !== 'ok')
  );
  const t = Math.max(1, v.checked),
    w = (x: number) => (100 * x) / t;
  const head = `<h2>Files on disk <span>${num(v.ok)}/${num(v.checked)} match</span></h2><div class="hbar" style="height:10px"><i style="width:${w(v.ok)}%;background:var(--ok)"></i><i style="width:${w(v.stale + (v.unverified ?? 0))}%;background:var(--warning)"></i><i style="width:${w(v.missing + v.lines)}%;background:var(--critical)"></i></div><div class="note">${v.stale} changed since snapshot${v.unverified ? ` · ${v.unverified} freshness unverified` : ''} · ${v.missing} missing · ${v.lines} with lines outside the file${v.staleCheck === 'ok' ? '' : ` · stale check ${esc(v.staleCheck)}`}</div>`;
  const row = (n: ViewNode & { check: FileCheck }) =>
    `<tr><td><span class="badge ${n.check.status}">${n.check.status}</span>${nodeLink(n.id)}<div class="note">${esc(n.check.problems?.[0] ?? '')}</div></td></tr>`;
  return (
    head +
    (bad.length
      ? `<table>${bad.slice(0, limit('checks')).map(row).join('')}</table>${more('checks', bad.length)}`
      : '')
  );
}

const fmt = (x: number | null | undefined) =>
  x == null
    ? '–'
    : Number.isInteger(x) || Math.abs(x) >= 100
      ? Math.round(x).toLocaleString()
      : x.toPrecision(3);
function statsBlock(M: MetricsPayload) {
  const s = M.stats;
  const kpis: [string, number][] = [
    ['nodes', s.nodes],
    ['weighted edges', s.edges],
    ['total weight', s.weight],
    ['cycles', s.cycles],
    ['communities', s.communities],
    ['modularity', s.modularity],
    ['max depth', s.maxDepth],
    ['max k-core', s.maxCore],
  ];
  return `<h2>Graph <span>algorithms ${Object.values(M.timings).reduce((a, x) => a + x, 0)} ms</span></h2><div class="kpis">${kpis.map(([k, v]) => `<span class="counter"><b>${fmt(v)}</b><span>${k}</span></span>`).join('')}</div><div class="note">Weight: ${esc(M.weightRule)}</div>`;
}
const RANKS: [RankKey, string][] = [
  ['pagerank', 'PageRank'],
  ['betweenness', 'Betweenness'],
  ['inWeight', 'In weight'],
  ['outWeight', 'Out weight'],
  ['core', 'k-core'],
  ['depth', 'Depth'],
];
function rankBlock(S: LiveState) {
  const key = P.rankBy;
  const rows = [...S.nodes.values()]
    .filter((n): n is Measured => Boolean(n.metrics))
    .toSorted((a, b) => b.metrics[key] - a.metrics[key]);
  const max = rows[0]?.metrics[key] || 1;
  const row = (n: Measured) =>
    `<tr><td>${nodeLink(n.id)}</td><td style="width:90px">${bar((100 * n.metrics[key]) / max, '--acc')}</td><td class="n">${fmt(n.metrics[key])}</td></tr>`;
  return `<h2>Files ranked by <select id="rankBy" aria-label="rank files by">${RANKS.map(([k, l]) => `<option value="${k}"${k === key ? ' selected' : ''}>${l}</option>`).join('')}</select></h2><table>${rows.slice(0, limit('ranks')).map(row).join('')}</table>${more('ranks', rows.length)}`;
}
type Measured = ViewNode & { metrics: FileMetrics };
const setBlock = <T>(
  title: string,
  list: T[],
  name: PageName,
  card: (x: T) => string
) =>
  list.length
    ? `<h2>${title} <span>${list.length}</span></h2>${list.slice(0, limit(name)).map(card).join('')}${more(name, list.length)}`
    : '';
const cycleCard = (c: CycleSet) =>
  `<div class="card" data-hl="${attr(c.members)}">${c.size} files · weight ${fmt(c.weight)} · ${esc(c.languages.join(', '))}<br><small class="mono">${esc(c.members.slice(0, 4).join(' → '))}${c.size > 4 ? ` … +${c.size - 4}` : ''}</small><br><small>${esc(c.chunks.join(' + '))}</small></div>`;
const communityCard = (c: Community) =>
  `<div class="card" data-hl="${attr(c.members)}" style="border-left-color:var(${c.id < 7 ? `--s${c.id + 1}` : '--other'})"><b>${esc(c.label)}</b> · ${c.size} files · ${Math.round(c.purity * 100)}% in ${esc(c.chunk)}<br><small>weight inside ${fmt(c.internal)} · to other communities ${fmt(c.external)}</small><br><small class="mono">${esc(c.members.slice(0, 3).join(', '))}</small></div>`;
function packagesBlock(M: MetricsPayload) {
  return `<h2>Packages <span>Martin coupling</span></h2><table><tr><th>chunk</th><th class="n">files</th><th class="n" title="afferent: files outside that depend on it">Ca</th><th class="n" title="efferent: files outside it depends on">Ce</th><th colspan="2" class="n" title="instability Ce/(Ca+Ce)">I</th></tr>${M.packages.slice(0, limit('packages')).map(packageRow).join('')}</table>${more('packages', M.packages.length)}`;
}
const packageRow = (p: PackageCoupling) =>
  `<tr><td class="mono">${esc(p.chunk)}</td><td class="n">${p.files}</td><td class="n">${p.ca}</td><td class="n">${p.ce}</td><td style="width:70px">${p.instability == null ? '' : bar(100 * p.instability, '--acc-2')}</td><td class="n">${p.instability ?? '–'}</td></tr>`;
function metricsTab() {
  const S = P.S,
    M = S.metrics;
  if (!M)
    return (
      [verifyBlock(S), lspBlock(S.lspStats)].join('') ||
      '<p class="note">Metrics appear when the graph is complete: weighted PageRank, betweenness, cycles (Tarjan), communities (Louvain), k-core, dependency depth and package coupling.</p>'
    );
  return [
    statsBlock(M),
    rankBlock(S),
    setBlock('Cycles', M.cycles, 'cycles', cycleCard),
    setBlock('Communities', M.communities, 'communities', communityCard),
    packagesBlock(M),
    verifyBlock(S),
    lspBlock(S.lspStats),
  ].join('');
}

function toolRow(t: ToolCall) {
  const err =
    P.expanded.has(t.id) && t.error
      ? `<div class="bad mono">${esc(t.error)}</div>`
      : '';
  return `<tr data-expand="${t.id}" class="k"><td><span class="state ${t.state}"></span><b>${esc(t.tool)}</b> <span class="note">${esc(t.detail)}</span> <span class="badge">${esc(t.page)}</span>${t.more ? '<span class="badge">continues</span>' : ''}${err}</td><td class="n">${t.rows != null ? num(t.rows) : ''}${t.total != null ? `/${num(t.total)}` : ''}</td><td class="n">${secs(t.ms)}</td><td class="n note">${kb(t.bytes)}</td></tr>`;
}

function toolsTab() {
  const S = P.S;
  const sum = [...S.toolSummary.values()].toSorted((a, b) => b.ms - a.ms);
  const list = S.tools
    .filter(
      t =>
        (!P.toolFilter || t.tool === P.toolFilter) &&
        (!P.errorsOnly || t.state === 'error')
    )
    .toReversed();
  const summary = (s: ToolSummary) =>
    `<tr><td class="k" data-tool="${esc(s.tool)}">${esc(s.tool)}${s.running ? ` <span class="state start"></span>${s.running}` : ''}</td><td class="n">${s.calls}</td><td class="n">${s.pages}</td><td class="n">${num(s.rows)}</td><td class="n">${secs(s.ms)}</td><td class="n">${secs(Math.round(s.ms / Math.max(1, s.calls - s.running)))}</td><td class="n">${secs(s.max)}</td><td class="n ${s.errors ? 'bad' : ''}">${s.errors}</td></tr>`;
  return `<h2>Per tool <span>every octocode call this run made</span></h2><table><tr><th>tool</th><th class="n">calls</th><th class="n">pages</th><th class="n">rows</th><th class="n">total</th><th class="n">avg</th><th class="n">max</th><th class="n">err</th></tr>${sum.map(summary).join('')}</table>
    <h2>Calls <span>${P.toolFilter ? `${esc(P.toolFilter)} · <span class="k" data-tool="">all</span>` : 'newest first'} · <label><input type="checkbox" id="errorsOnly" ${P.errorsOnly ? 'checked' : ''}> errors</label></span></h2>
    <table>${list.slice(0, limit('tools')).map(toolRow).join('')}</table>${more('tools', list.length)}`;
}

function lspAnswer(r: LspResult | undefined) {
  if (!r) return '';
  if (r.state !== 'done')
    return '<div class="note">asking the language server…</div>';
  if (r.error) return `<div class="note bad">${esc(r.error)}</div>`;
  const notes = (r.notes ?? [])
    .map(n => `<div class="note">${esc(n)}</div>`)
    .join('');
  if (!r.rows.length) return notes || '<div class="note">none</div>';
  return `${notes}<div class="note">${r.rows.map(x => `${x.symbolName ? `${esc(x.symbolName)} ` : ''}${at(x.path, x.line ?? 1)} <span class="k" data-select="${esc(x.path)}">→</span>`).join('<br>')}</div>`;
}

function symbolCounts(symbols: Declaration[]) {
  const counts = new Map(symbols.map(s => [s.id, { out: 0, inn: 0 }]));
  if (counts.size)
    for (const c of P.S.calls.values()) {
      const source = counts.get(c.source),
        target = counts.get(c.target);
      if (source) source.out++;
      if (target) target.inn++;
    }
  return counts;
}

function symbolRow(
  file: string,
  s: Declaration,
  { out, inn }: { out: number; inn: number }
) {
  const reqs = (['callers', 'callees', 'references'] as const).map(
    (operation): LspRequest => ({
      file,
      symbolName: s.name,
      line: s.line,
      operation,
    })
  );
  const span = s.endLine && s.endLine !== s.line ? `–${s.endLine}` : '';
  return `<tr><td><b>${esc(s.name)}</b> <span class="badge">${esc(s.kind)}</span>${s.exported ? '<span class="badge">exported</span>' : ''}<div>${at(file, s.line ?? 1)}${span} · ${out} out · ${inn} in</div><div>${(P.S.offline ? [] : reqs).map(r => `<button data-lsp="${attr(r)}">${r.operation}</button>`).join(' ')}</div>${reqs.map(r => lspAnswer(P.S.lspResults.get(JSON.stringify(r)))).join('')}</td></tr>`;
}

function astTree(file: string, list: AstItem[], depth: number): string {
  return list
    .map(s => {
      if (typeof s === 'string') {
        const tab = s.indexOf('\t');
        const line = tab > 0 ? Number(s.slice(0, tab)) : NaN;
        if (!Number.isSafeInteger(line) || line < 1)
          return `<pre>${esc(s)}</pre>`;
        return `<div>${at(file, line)} <code style="white-space:pre-wrap">${esc(s.slice(tab + 1))}</code></div>`;
      }
      return `<div style="padding-left:${depth * 14}px">${at(file, s.line)} <b>${esc(s.symbolName)}</b> <span class="badge">${esc(s.kind)}</span>${s.endLine ? `<span class="note">–${s.endLine}</span>` : ''}</div>${astTree(file, s.members ?? [], depth + 1)}`;
    })
    .join('');
}
function astBlock(id: string) {
  const a = P.S.ast.get(id);
  if (!a && P.S.offline)
    return '<div class="note">Open a live viewer to request an AST outline.</div>';
  if (!a || a.state === 'start')
    return '<div class="note">reading the AST outline…</div>';
  if (a.error) return `<div class="bad note">${esc(a.error)}</div>`;
  return a.symbols?.length
    ? astTree(id, a.symbols, 0)
    : '<div class="note">no declarations</div>';
}

function edgeList(list: ViewEdge[], other: 'source' | 'target') {
  const row = (e: ViewEdge) =>
    `<tr><td>${nodeLink(e[other])}<div class="note">line ${lineLinks(e.source, e.lines)}${e.via !== 'graph' ? ` · <span class="badge">${e.via}</span>` : ''}${e.test ? ' · test' : ''}</div></td><td class="n">${e.count}</td></tr>`;
  return `<table>${list.slice(0, limit('edges')).map(row).join('')}</table>${more('edges', list.length)}`;
}

// One entry per status the verify phase can write (the type makes a missing one a compile error).
const CHECK_TEXT: Record<FileCheckStatus, [string, string]> = {
  ok: ['✓', 'Matches the file on disk'],
  stale: ['!', 'Changed since the snapshot'],
  missing: ['✕', 'File missing'],
  lines: ['✕', 'Lines outside the file'],
  unverified: ['?', 'Freshness not verified'],
};
function checkBlock(n: ViewNode) {
  const c = n.check;
  if (!n.inGraph)
    return '<div class="check"><span class="ic">○</span><div>Listed by structureSearch; the graph parsed no code in this file.</div></div>';
  if (!c)
    return '<div class="check"><span class="ic">…</span><div class="note">Not checked against the file yet (Verify phase).</div></div>';
  const [ic, text] = CHECK_TEXT[c.status];
  const facts =
    c.lines != null ? ` · ${num(c.lines)} lines · ${kb(c.bytes)}` : '';
  return `<div class="check ${c.status}"><span class="ic">${ic}</span><div><b>${text}</b><span class="note">${facts}</span>${(c.problems ?? []).map(p => `<div class="note">${esc(p)}</div>`).join('')}</div></div>`;
}
const edgeSection = (
  title: string,
  list: ViewEdge[],
  other: 'source' | 'target'
) => `<h2>${title} <span>${list.length}</span></h2>${edgeList(list, other)}`;
function metricsLine(n: ViewNode) {
  // A node's metrics and the store's arrive in the same event.
  const m = n.metrics,
    C = P.S.metrics;
  if (!m || !C) return '';
  const position =
    [...P.S.nodes.values()].filter(
      x => x.metrics && x.metrics.pagerank > m.pagerank
    ).length + 1;
  const community = C.communities.find(c => c.id === m.community),
    cycle = m.cycle == null ? null : C.cycles[m.cycle];
  const kpis = [
    [`#${position}`, 'PageRank'],
    [fmt(m.betweenness), 'betweenness'],
    [fmt(m.inWeight), 'in weight'],
    [fmt(m.outWeight), 'out weight'],
    [m.depth, 'depth'],
    [m.core, 'k-core'],
  ];
  const sets = `${community ? `<span class="k" data-hl="${attr(community.members)}">community ${m.community} (${community.size})</span>` : '<span class="note">no community peers</span>'}${cycle ? ` · <span class="k bad" data-hl="${attr(cycle.members)}">in a ${cycle.size}-file cycle</span>` : ''}`;
  return `<div class="kpis">${kpis.map(([v, k]) => `<span class="counter"><b>${v}</b><span>${k}</span></span>`).join('')}</div><div class="note">${sets}</div>`;
}

function nodeTab() {
  const S = P.S,
    id = S.selected,
    n = id ? S.nodes.get(id) : undefined;
  if (!id || !n)
    return '<p class="note">Click a node, search with <b>/</b>, or pick a hub or finding. Every node shows its file location and opens in your editor.</p>';
  const path = n.manifest ?? n.path;
  const edges = [...S.edges.values()];
  const by = (kind: ViewEdge['kind'], side: 'source' | 'target') =>
    edges
      .filter(e => e.kind === kind && e[side] === id)
      .toSorted((a, b) => b.count - a.count);
  const syms = (S.symbols.get(id) ?? []).toSorted(
    (a, b) => (a.line ?? 0) - (b.line ?? 0)
  );
  const visibleSymbols = syms.slice(0, limit('functions'));
  const counts = symbolCounts(visibleSymbols);
  const chips = `${n.language ? `<span class="badge">${esc(n.language)}</span>` : ''}${(n.roles ?? []).map(r => `<span class="badge">${esc(r)}</span>`).join('')}${n.chunk ? `<span class="badge">chunk ${esc(n.chunk)}</span>` : ''}${n.inGraph ? '' : '<span class="badge">not in graph</span>'}`;
  return `<h2>${esc(n.label)} <span>${n.kind}</span></h2>
    <div class="loc"><a href="${esc(href(path, 1))}" class="open">Open</a><span class="mono">${esc(path)}</span><button data-copy="${esc(abs(path))}">copy path</button><button data-copy="${esc(`${path}:1`)}">copy path:line</button></div>
    <div>${chips} <span class="note">${n.fanIn} importers · ${n.fanOut} imports</span></div>
    ${metricsLine(n)}${checkBlock(n)}
    ${edgeSection('Imports', by('imports', 'source'), 'target')}${edgeSection('Imported by', by('imports', 'target'), 'source')}
    ${edgeSection('Calls into other files', by('calls', 'source'), 'target')}${edgeSection('Called from', by('calls', 'target'), 'source')}
    <h2>Declarations <span>${syms.length}</span></h2><table>${visibleSymbols.map(s => symbolRow(id, s, counts.get(s.id) ?? { out: 0, inn: 0 })).join('')}</table>${more('functions', syms.length)}
    <h2>AST outline <span>astSearch symbols</span></h2>${astBlock(id)}`;
}

const TABS: Record<TabName, () => string> = {
  progress: progressTab,
  metrics: metricsTab,
  tools: toolsTab,
  node: nodeTab,
};
const isTab = (name: string | undefined): name is TabName =>
  name != null && Object.hasOwn(TABS, name);
const rankKey = (value: string) => RANKS.find(([k]) => k === value)?.[0];
function render() {
  header();
  document.querySelectorAll('.tabs button').forEach(b => {
    if (b instanceof HTMLElement)
      b.classList.toggle('on', b.dataset.tab === P.tab);
  });
  $('panel').innerHTML = TABS[P.tab]();
  // Controls of the current tab only.
  const errorsOnly = document.getElementById('errorsOnly'),
    rankBy = document.getElementById('rankBy');
  if (errorsOnly instanceof HTMLInputElement)
    errorsOnly.addEventListener('change', () => {
      P.errorsOnly = errorsOnly.checked;
      render();
    });
  if (rankBy instanceof HTMLSelectElement)
    rankBy.addEventListener('change', () => {
      P.rankBy = rankKey(rankBy.value) ?? P.rankBy;
      render();
    });
}
function schedule() {
  if (!P.timer)
    P.timer = setTimeout(() => {
      P.timer = 0;
      render();
    }, 180);
}

// Each action gets its element's data-<name> value; `hl` and `lsp` hold JSON this module wrote with attr().
const ACTIONS: Record<string, (value: string) => void> = {
  select: id => P.select(id, { focus: true }),
  hl: ids => P.highlight(JSON.parse(ids) as string[]),
  lsp: req => P.ask.lsp(JSON.parse(req) as LspRequest),
  more: name => {
    if (isPage(name)) P.shown.set(name, limit(name) + PAGE[name]);
  },
  copy: text => {
    void navigator.clipboard?.writeText(text);
  },
  tool: tool => {
    P.toolFilter = tool;
  },
  expand: k => {
    if (!P.expanded.delete(k)) P.expanded.add(k);
  },
};
function onPanelClick(ev: MouseEvent) {
  const t = closest(
    ev.target,
    '[data-select],[data-hl],[data-lsp],[data-more],[data-copy],[data-tool],[data-expand]'
  );
  if (!t) return;
  const name = Object.keys(ACTIONS).find(k => t.dataset[k] != null);
  const value = name == null ? undefined : t.dataset[name];
  if (name == null || value == null) return;
  ACTIONS[name](value);
  render();
}

// Toolbar: edge kinds, weak links, tests, non-code files, labels, color key, layers, 2D/3D, overview, collapse,
// minimum link weight; legend; editor choice.
function bindToolbar() {
  const TOGGLES: [keyof Toggles, string][] = [
    ['imports', 'Imports'],
    ['calls', 'Calls'],
    ['weak', 'Weak links'],
    ['tests', 'Tests'],
    ['docs', 'Non-code files'],
    ['labels', 'Labels'],
  ];
  const g = P.graph;
  const isToggle = (name: string): name is keyof Toggles =>
    Object.hasOwn(g.toggles, name);
  const views =
    '<button data-layout="layers" title="place files by dependency depth">Layers</button><button data-dims title="switch between a flat 2D and a 3D layout">3D</button><button data-overview title="group files into folders during the build and communities when ready">Overview</button><button data-collapse title="fold every expanded group">Collapse all</button>';
  const weight =
    '<label class="minw" title="hide links lighter than this weight (log scale)">min weight <input id="minW" type="range" min="0" max="1" step="0.01" value="0" aria-label="minimum link weight"><output id="minWOut">all</output></label>';
  $('toggles').innerHTML =
    `${TOGGLES.map(([k, l]) => `<button data-toggle="${k}" class="${g.toggles[k] ? 'on' : ''}">${l}</button>`).join('')}<select id="colorBy" aria-label="color nodes by"><option value="group">color: package / folder</option><option value="chunk">color: chunk</option><option value="language">color: language</option><option value="community">color: community</option><option value="depth">color: depth</option></select>${views}${weight}`;
  $('toggles').addEventListener('click', ev => {
    const b = closest(ev.target, 'button');
    if (!b) return;
    const d = b.dataset;
    if (d.toggle && isToggle(d.toggle))
      b.classList.toggle('on', g.toggle(d.toggle));
    if (d.layout) {
      g.setLayout(g.layout === 'layers' ? 'force' : 'layers');
      b.classList.toggle('on', g.layout === 'layers');
    }
    if (d.dims != null) g.setDimensions(g.dimensions === 3 ? 2 : 3);
    if (d.overview != null) g.setOverview(!g.overview);
    if (d.collapse != null) g.collapseAll();
    legend();
  });
  const minW = $('minW', HTMLInputElement),
    colorBy = $('colorBy', HTMLSelectElement);
  minW.addEventListener('input', () => {
    const w = g.setMinWeight(Number(minW.value));
    $('minWOut', HTMLOutputElement).value =
      w > 0 ? `≥ ${w < 10 ? w.toFixed(1) : Math.round(w)}` : 'all';
  });
  colorBy.addEventListener('change', () => {
    g.colorBy = colorBy.value;
    legend();
  });
  document.addEventListener('gv:graph', legend);
  $('fit').addEventListener('click', () => g.fit());
  try {
    // A stored choice from another version may name an editor this one lacks.
    const stored = localStorage.getItem('gv-editor');
    if (stored && Object.hasOwn(EDITORS, stored)) P.editor = stored;
  } catch {
    /* default editor */
  }
  const editor = $('editor', HTMLSelectElement);
  editor.value = P.editor;
  editor.addEventListener('change', () => {
    P.editor = editor.value;
    try {
      localStorage.setItem('gv-editor', P.editor);
    } catch {
      /* per-viewer only */
    }
    render();
  });
}

// Search across files and functions; Enter or click selects and zooms.
function suggest() {
  const s = $('q', HTMLInputElement).value.trim().toLowerCase(),
    box = $('suggest');
  if (!s) {
    box.hidden = true;
    return;
  }
  const files = [...P.S.nodes.values()]
    .filter(n => n.path.toLowerCase().includes(s))
    .toSorted((a, b) => b.fanIn - a.fanIn)
    .slice(0, 8)
    .map(n => ({ id: n.id, text: n.path, note: `${n.fanIn} in` }));
  const fns: Hit[] = [];
  for (const symbols of P.S.symbols.values()) {
    for (const x of symbols) {
      if (x.name?.toLowerCase().includes(s))
        fns.push({
          id: x.file,
          text: `${x.name}()`,
          note: `${x.file.split('/').pop()}:${x.line}`,
        });
      if (fns.length === 6) break;
    }
    if (fns.length === 6) break;
  }
  P.hits = [...files, ...fns];
  P.cursor = 0;
  box.innerHTML =
    P.hits
      .map(
        (h, i) =>
          `<div data-i="${i}" class="${i === 0 ? 'on' : ''}"><span class="mono">${esc(h.text)}</span><span class="note">${esc(h.note)}</span></div>`
      )
      .join('') || '<div class="note">no match</div>';
  box.hidden = false;
}
function pick(i: number) {
  const h = P.hits[i];
  if (!h) return;
  $('suggest').hidden = true;
  $('q').blur();
  P.select(h.id, { focus: true });
}
function onSearchKey(ev: KeyboardEvent) {
  const box = $('suggest');
  if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
    P.cursor =
      (P.cursor + (ev.key === 'ArrowDown' ? 1 : P.hits.length - 1)) %
      Math.max(1, P.hits.length);
    [...box.children].forEach((c, i) =>
      c.classList.toggle('on', i === P.cursor)
    );
    ev.preventDefault();
  } else if (ev.key === 'Enter') pick(P.cursor);
  else if (ev.key === 'Escape') {
    box.hidden = true;
    $('q').blur();
  }
}
function onKey(ev: KeyboardEvent) {
  if (
    ev.target instanceof Element &&
    ev.target.matches('input,select,textarea')
  )
    return;
  if (ev.key === '/') {
    ev.preventDefault();
    $('q').focus();
  } else if (ev.key === 'Escape') {
    P.select(null);
    P.highlight([]);
  } else if (ev.key === 'f') P.graph.fit();
  else if (/^[1-4]$/.test(ev.key)) {
    const tab = Object.keys(TABS)[Number(ev.key) - 1];
    if (isTab(tab)) {
      P.tab = tab;
      render();
    }
  }
}

function feed(text: string, level: FeedLevel = 'info') {
  const el = document.createElement('div');
  el.className = level;
  el.textContent = text;
  $('feed').prepend(el);
  while ($('feed').children.length > 5) $('feed').lastChild?.remove();
  setTimeout(() => el.remove(), 6000);
}

export function createPanels(
  S: LiveState,
  { select, highlight, ask, graph }: PanelDeps
) {
  Object.assign(P, { S, select, highlight, ask, graph });
  bindToolbar();
  $('panel').addEventListener('click', onPanelClick);
  find(document, '.tabs', HTMLElement).addEventListener('click', ev => {
    const tab =
      ev.target instanceof HTMLElement ? ev.target.dataset.tab : undefined;
    if (isTab(tab)) {
      P.tab = tab;
      render();
    }
  });
  $('q').addEventListener('input', suggest);
  $('q').addEventListener('keydown', onSearchKey);
  $('suggest').addEventListener('mousedown', ev => {
    const d = closest(ev.target, '[data-i]');
    if (d) pick(Number(d.dataset.i));
  });
  document.addEventListener('keydown', onKey);
  render();
  return {
    schedule,
    feed,
    render,
    tab: (t: TabName) => {
      P.tab = t;
      render();
    },
  };
}
