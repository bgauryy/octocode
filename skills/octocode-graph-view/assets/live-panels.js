// Header, phases, toolbar, search and the four side panels. Lists page in place ("Show more"); nothing is dropped.
// One panel set per page: P holds its state, the functions below share it.
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const attr = (v) => esc(JSON.stringify(v));
const num = (n) => (n == null ? '–' : Number(n).toLocaleString());
const secs = (ms) => (ms == null ? '' : ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`);
const kb = (b) => (b == null ? '' : b < 1024 ? `${b} B` : b < 1 << 20 ? `${(b / 1024).toFixed(1)} KB` : `${(b / (1 << 20)).toFixed(1)} MB`);
const endId = (x) => x?.id ?? x;
const nodeLink = (id) => `<span class="k mono" data-select="${esc(id)}">${esc(id)}</span>`;
const bar = (pct, color) => `<div class="hbar"><i style="width:${pct}%;background:var(${color})"></i></div>`;
const PHASES = [['list', 'Files'], ['plan', 'Plan'], ['graph', 'Graph'], ['stitch', 'Stitch'], ['lsp', 'LSP'], ['verify', 'Verify'], ['metrics', 'Metrics']];
const WEIGHT = { list: 5, plan: 2, graph: 55, stitch: 4, lsp: 22, verify: 8, metrics: 4 };
const PAGE = { checks: 25, cycles: 15, communities: 15, ranks: 25, packages: 20, tools: 100, edges: 40, functions: 60, chunks: 60 };
const EDITORS = {
  vscode: (abs, line) => `vscode://file${abs}:${line}`,
  cursor: (abs, line) => `cursor://file${abs}:${line}`,
  idea: (abs, line) => `idea://open?file=${encodeURIComponent(abs)}&line=${line}`,
  file: (abs) => `file://${abs}`,
};

const P = { S: null, select: null, highlight: null, ask: null, graph: null, tab: 'progress', timer: 0, shown: new Map(), expanded: new Set(), editor: 'vscode', toolFilter: '', errorsOnly: false, hits: [], cursor: 0, rankBy: 'pagerank' };

const limit = (name) => P.shown.get(name) ?? PAGE[name];
const more = (name, total) => (total > limit(name) ? `<button class="more" data-more="${name}">Show ${Math.min(PAGE[name], total - limit(name))} more of ${num(total - limit(name))}</button>` : '');
const abs = (path) => `${P.S.root}/${path}`;
const href = (path, line = 1) => EDITORS[P.editor](abs(path), line);
const at = (path, line) => `<a href="${esc(href(path, line))}" class="mono" title="open ${esc(path)}:${line} in the editor">${esc(path)}:${line}</a>`;
const lineLinks = (path, lines) => [...lines].toSorted((a, b) => a - b).map((l) => `<a href="${esc(href(path, l))}">${l}</a>`).join(', ');
function serverNow() {
  const S = P.S;
  if (S.done) return S.done.ms;
  return S.error ? S.t : S.t + (performance.now() - (S.clockAt ?? performance.now()));
}

function counters() {
  const S = P.S;
  const inGraph = [...S.nodes.values()].filter((n) => n.inGraph).length;
  const imports = [...S.edges.values()].filter((e) => e.kind === 'imports').length;
  const fns = [...S.symbols.values()].reduce((a, l) => a + l.length, 0);
  const v = S.verification;
  return [[`${num(inGraph)}/${num(S.nodes.size)}`, 'files'], ...(v ? [[`${num(v.ok)}/${num(v.checked)}`, 'match disk']] : []), [num(imports), 'imports'], [num(S.calls.size), 'calls'], [num(fns), 'functions'], [num(S.tools.length), 'tool calls'], [secs(Math.round(serverNow())), S.done ? 'total' : 'elapsed']]
    .map(([value, k]) => `<span class="counter"><b>${value}</b><span>${k}</span></span>`).join('');
}

function header() {
  const S = P.S;
  $('title').textContent = S.run?.name ?? 'Graph live';
  $('rootPath').textContent = S.root;
  $('counters').innerHTML = counters();
  let pct = 0;
  $('phases').innerHTML = PHASES.map(([id, label]) => {
    const p = S.phases.get(id), pr = S.progress.get(id);
    const st = p?.state ?? 'pending';
    pct += WEIGHT[id] * (st === 'done' || st === 'skipped' ? 1 : pr?.total ? pr.done / pr.total : 0);
    return `<span class="phase ${st}" title="${esc(p?.note ?? st)}"><span class="ic"></span>${label}${pr?.total ? ` ${pr.done}/${pr.total}` : ''}${p?.ms != null ? ` · ${secs(p.ms)}` : ''}</span>`;
  }).join('');
  $('overall').style.width = `${S.done ? 100 : Math.min(99, pct)}%`;
  $('bar').classList.toggle('done', Boolean(S.done || S.error));
  legend();
  $('cancel').disabled = Boolean(S.done || S.error);
}

const EDGE_KEY = [['--e-imp', 'import'], ['--e-call', 'call · graph'], ['--e-both', 'call · graph + LSP'], ['--e-lsp', 'call · LSP only'], ['--e-stitch', 'between chunks']];
// Legend: the active color mode (seven slots plus Other, or the depth ramp), link kinds by color with weak links
// fainter (WebGL draws no dashes), node states, and the overview gestures. It also keeps the toolbar state current.
function colorLegend(S) {
  if (P.graph.colorBy === 'depth') return `<span>depth 0 (depends on nothing)</span><i style="display:inline-block;width:120px;height:10px;border-radius:5px;background:linear-gradient(90deg,var(--seq-lo),var(--seq-hi))"></i><span>${S.metrics?.stats.maxDepth ?? '…'}</span>`;
  if (P.graph.colorBy !== 'community') return null;
  const top = (S.metrics?.communities ?? []).slice(0, 7);
  return `${top.map((c) => `<span><i class="c" style="background:var(--s${c.id + 1})"></i>${c.size} · ${esc(c.label)}</span>`).join('')}<span><i class="c" style="background:var(--other)"></i>Other</span>`;
}

function legend() {
  const S = P.S, g = P.graph;
  const folders = colorLegend(S) ?? (g.colorBy === 'group' ? [...S.groupSlot].map(([k, i]) => `<span><i class="c" style="background:var(--s${i})"></i>${esc(k === '.' ? 'root files' : k)}</span>`).join('') + (S.groupSlot.size ? '<span><i class="c" style="background:var(--other)"></i>Other</span>' : '') : `<span class="note">color: ${esc(g.colorBy)} (first seven, then Other)</span>`);
  const edges = EDGE_KEY.map(([v, l]) => `<span><i class="l" style="border-top:2px solid var(${v})"></i>${l}</span>`).join('') + '<span><i class="l" style="border-top:2px solid var(--e-call);opacity:.35"></i>weak / low confidence</span>';
  const states = '<span><i class="c" style="background:var(--warning)"></i>changed since snapshot</span><span><i class="c" style="background:var(--critical)"></i>missing / line outside file</span><span><i class="c" style="background:var(--acc)"></i>being ingested</span><span><i class="c" style="background:var(--mute);opacity:.6"></i>listed, not parsed</span>';
  const overview = g.overview ? '<div class="row note">overview: one node per community, sized by summed PageRank · click a community to expand it · right-click a file to fold it back</div>' : '';
  $('legend').innerHTML = `<div class="row">${folders}</div><div class="row">${edges}</div><div class="row">${states}</div>${overview}`;
  const [overviewButton, collapseButton, dimsButton] = ['overview', 'collapse', 'dims'].map((k) => $('toggles').querySelector(`[data-${k}]`));
  if (!overviewButton) return;
  overviewButton.classList.toggle('on', g.overview);
  overviewButton.disabled = !g.hasMetrics;
  collapseButton.disabled = !g.overview || !g.expanded;
  dimsButton.classList.toggle('on', g.dimensions === 3);
}

function chunkRow(c, now) {
  const time = c.state === 'done' || c.state === 'error' ? secs(c.ms) : c.startT != null ? secs(Math.round(now - c.startT)) : '';
  const rows = Object.values(c.rows ?? {}).reduce((a, v) => a + v, 0);
  const name = c.path === '.' ? P.S.run?.name ?? '.' : c.path;
  const counts = c.counts ? `<div class="note">${num(c.counts.imports)} imports · ${num(c.counts.calls)} calls · ${num(c.counts.declarations)} declarations</div>` : c.state === 'ingesting' && c.build ? `<div class="note">${esc(c.build)}</div>` : '';
  return `<tr><td><span class="state ${c.state}"></span><span class="mono">${esc(name)}${c.loose ? ' (files outside packages)' : ''}</span>${c.package ? ` <span class="badge">${esc(c.package)}</span>` : ''}${c.error ? `<div class="bad note">${esc(c.error)}</div>` : ''}${counts}</td><td class="n">${num(c.files)}</td><td class="n">${num(rows)}</td><td class="n">${time}</td></tr>`;
}

function progressTab() {
  const S = P.S;
  const chunks = [...S.chunks.values()].filter((c) => c.state !== 'split');
  const order = { ingesting: 0, paging: 0, queued: 1, error: 2, done: 3 };
  const rows = chunks.toSorted((a, b) => (order[a.state] ?? 4) - (order[b.state] ?? 4) || b.files - a.files);
  const split = [...S.chunks.values()].filter((c) => c.state === 'split');
  const phases = PHASES.map(([id, label]) => { const p = S.phases.get(id); return `<tr><td><span class="state ${p?.state === 'running' ? 'start' : p?.state ?? ''}"></span>${label}</td><td class="note">${esc(p?.note ?? p?.state ?? 'pending')}</td><td class="n">${secs(p?.ms)}</td></tr>`; }).join('');
  const st = S.stitched;
  return `<h2>Phases</h2><table>${phases}</table>
    <h2>Chunks <span>${chunks.filter((c) => c.state === 'done').length}/${chunks.length} built</span></h2>
    <table><tr><th>chunk</th><th class="n">files</th><th class="n">rows</th><th class="n">time</th></tr>${rows.slice(0, limit('chunks')).map((c) => chunkRow(c, serverNow())).join('')}</table>${more('chunks', rows.length)}
    ${split.length ? `<h2>Split on scopeTooBroad</h2>${split.map((c) => `<div class="note mono">${esc(c.path)}</div>`).join('')}` : ''}
    ${st ? `<h2>Stitch</h2><div class="note">${num(st.imports.length)} imports and ${num(st.calls.length)} calls between chunks · ${num(st.unresolved.length)} out-of-root imports unresolved · ${num(st.uncovered.length)} files in no chunk</div>` : ''}
    ${S.notes.length ? `<h2>Notes</h2>${S.notes.map((n) => `<div class="note ${n.level}">${esc(n.text)}</div>`).join('')}` : ''}
    ${S.done ? `<h2>Result</h2><div class="note mono">${esc(S.done.data)}</div><div class="note mono">${esc(S.done.brief)}</div>` : ''}${S.error ? `<h2>Error</h2><div class="bad">${esc(S.error.message)}</div>` : ''}`;
}

function lspBlock(L) {
  if (!L) return '';
  const t = Math.max(1, L.both + L.graphOnly + L.lspOnly), w = (v) => (100 * v) / t;
  return `<h2>Graph vs LSP calls <span>${L.probed} probed</span></h2><div class="hbar" style="height:12px"><i style="width:${w(L.both)}%;background:var(--both)"></i><i style="width:${w(L.graphOnly)}%;background:var(--call)"></i><i style="width:${w(L.lspOnly)}%;background:var(--lsp)"></i></div><div class="note">${L.both} both · ${L.graphOnly} graph only · ${L.lspOnly} LSP only · ${L.resolved}/${L.probed} answered</div>`;
}

function verifyBlock(S) {
  const v = S.verification;
  if (!v) return '';
  const bad = [...S.nodes.values()].filter((n) => n.check && n.check.status !== 'ok');
  const t = Math.max(1, v.checked), w = (x) => (100 * x) / t;
  const head = `<h2>Files on disk <span>${num(v.ok)}/${num(v.checked)} match</span></h2><div class="hbar" style="height:10px"><i style="width:${w(v.ok)}%;background:var(--ok)"></i><i style="width:${w(v.stale)}%;background:var(--warning)"></i><i style="width:${w(v.missing + v.lines)}%;background:var(--critical)"></i></div><div class="note">${v.stale} changed since snapshot · ${v.missing} missing · ${v.lines} with lines outside the file${v.staleCheck === 'ok' ? '' : ` · stale check ${esc(v.staleCheck)}`}</div>`;
  const row = (n) => `<tr><td><span class="badge ${n.check.status}">${n.check.status}</span>${nodeLink(n.id)}<div class="note">${esc(n.check.problems?.[0] ?? '')}</div></td></tr>`;
  return head + (bad.length ? `<table>${bad.slice(0, limit('checks')).map(row).join('')}</table>${more('checks', bad.length)}` : '');
}

const fmt = (x) => (x == null ? '–' : Number.isInteger(x) || Math.abs(x) >= 100 ? Math.round(x).toLocaleString() : Number(x).toPrecision(3));
function statsBlock(M) {
  const s = M.stats;
  const kpis = [['nodes', s.nodes], ['weighted edges', s.edges], ['total weight', s.weight], ['cycles', s.cycles], ['communities', s.communities], ['modularity', s.modularity], ['max depth', s.maxDepth], ['max k-core', s.maxCore]];
  return `<h2>Graph <span>algorithms ${Object.values(M.timings).reduce((a, x) => a + x, 0)} ms</span></h2><div class="kpis">${kpis.map(([k, v]) => `<span class="counter"><b>${fmt(v)}</b><span>${k}</span></span>`).join('')}</div><div class="note">Weight: ${esc(M.weightRule)}</div>`;
}
const RANKS = [['pagerank', 'PageRank'], ['betweenness', 'Betweenness'], ['inWeight', 'In weight'], ['outWeight', 'Out weight'], ['core', 'k-core'], ['depth', 'Depth']];
function rankBlock(S) {
  const key = P.rankBy;
  const rows = [...S.nodes.values()].filter((n) => n.metrics).toSorted((a, b) => b.metrics[key] - a.metrics[key]);
  const max = rows[0]?.metrics[key] || 1;
  const row = (n) => `<tr><td>${nodeLink(n.id)}</td><td style="width:90px">${bar((100 * n.metrics[key]) / max, '--acc')}</td><td class="n">${fmt(n.metrics[key])}</td></tr>`;
  return `<h2>Files ranked by <select id="rankBy" aria-label="rank files by">${RANKS.map(([k, l]) => `<option value="${k}"${k === key ? ' selected' : ''}>${l}</option>`).join('')}</select></h2><table>${rows.slice(0, limit('ranks')).map(row).join('')}</table>${more('ranks', rows.length)}`;
}
const setBlock = (title, list, name, card) => (list.length ? `<h2>${title} <span>${list.length}</span></h2>${list.slice(0, limit(name)).map(card).join('')}${more(name, list.length)}` : '');
const cycleCard = (c) => `<div class="card" data-hl="${attr(c.members)}">${c.size} files · weight ${fmt(c.weight)} · ${esc(c.languages.join(', '))}<br><small class="mono">${esc(c.members.slice(0, 4).join(' → '))}${c.size > 4 ? ` … +${c.size - 4}` : ''}</small><br><small>${esc(c.chunks.join(' + '))}</small></div>`;
const communityCard = (c) => `<div class="card" data-hl="${attr(c.members)}" style="border-left-color:var(${c.id < 7 ? `--s${c.id + 1}` : '--other'})"><b>${esc(c.label)}</b> · ${c.size} files · ${Math.round(c.purity * 100)}% in ${esc(c.chunk)}<br><small>weight inside ${fmt(c.internal)} · to other communities ${fmt(c.external)}</small><br><small class="mono">${esc(c.members.slice(0, 3).join(', '))}</small></div>`;
function packagesBlock(M) {
  return `<h2>Packages <span>Martin coupling</span></h2><table><tr><th>chunk</th><th class="n">files</th><th class="n" title="afferent: files outside that depend on it">Ca</th><th class="n" title="efferent: files outside it depends on">Ce</th><th colspan="2" class="n" title="instability Ce/(Ca+Ce)">I</th></tr>${M.packages.slice(0, limit('packages')).map(packageRow).join('')}</table>${more('packages', M.packages.length)}`;
}
const packageRow = (p) => `<tr><td class="mono">${esc(p.chunk)}</td><td class="n">${p.files}</td><td class="n">${p.ca}</td><td class="n">${p.ce}</td><td style="width:70px">${p.instability == null ? '' : bar(100 * p.instability, '--acc-2')}</td><td class="n">${p.instability ?? '–'}</td></tr>`;
function metricsTab() {
  const S = P.S, M = S.metrics;
  if (!M) return [verifyBlock(S), lspBlock(S.lspStats)].join('') || '<p class="note">Metrics appear when the graph is complete: weighted PageRank, betweenness, cycles (Tarjan), communities (Louvain), k-core, dependency depth and package coupling.</p>';
  return [statsBlock(M), rankBlock(S), setBlock('Cycles', M.cycles, 'cycles', cycleCard), setBlock('Communities', M.communities, 'communities', communityCard), packagesBlock(M), verifyBlock(S), lspBlock(S.lspStats)].join('');
}

function toolRow(t) {
  const err = P.expanded.has(t.id) && t.error ? `<div class="bad mono">${esc(t.error)}</div>` : '';
  return `<tr data-expand="${t.id}" class="k"><td><span class="state ${t.state}"></span><b>${esc(t.tool)}</b> <span class="note">${esc(t.detail)}</span> <span class="badge">${esc(t.page)}</span>${t.more ? '<span class="badge">continues</span>' : ''}${err}</td><td class="n">${t.rows != null ? num(t.rows) : ''}${t.total != null ? `/${num(t.total)}` : ''}</td><td class="n">${secs(t.ms)}</td><td class="n note">${kb(t.bytes)}</td></tr>`;
}

function toolsTab() {
  const S = P.S;
  const sum = [...S.toolSummary.values()].toSorted((a, b) => b.ms - a.ms);
  const list = S.tools.filter((t) => (!P.toolFilter || t.tool === P.toolFilter) && (!P.errorsOnly || t.state === 'error')).toReversed();
  const summary = (s) => `<tr><td class="k" data-tool="${esc(s.tool)}">${esc(s.tool)}${s.running ? ` <span class="state start"></span>${s.running}` : ''}</td><td class="n">${s.calls}</td><td class="n">${s.pages}</td><td class="n">${num(s.rows)}</td><td class="n">${secs(s.ms)}</td><td class="n">${secs(Math.round(s.ms / Math.max(1, s.calls - s.running)))}</td><td class="n">${secs(s.max)}</td><td class="n ${s.errors ? 'bad' : ''}">${s.errors}</td></tr>`;
  return `<h2>Per tool <span>every octocode call this run made</span></h2><table><tr><th>tool</th><th class="n">calls</th><th class="n">pages</th><th class="n">rows</th><th class="n">total</th><th class="n">avg</th><th class="n">max</th><th class="n">err</th></tr>${sum.map(summary).join('')}</table>
    <h2>Calls <span>${P.toolFilter ? `${esc(P.toolFilter)} · <span class="k" data-tool="">all</span>` : 'newest first'} · <label><input type="checkbox" id="errorsOnly" ${P.errorsOnly ? 'checked' : ''}> errors</label></span></h2>
    <table>${list.slice(0, limit('tools')).map(toolRow).join('')}</table>${more('tools', list.length)}`;
}

function lspAnswer(r) {
  if (!r) return '';
  if (r.state !== 'done') return '<div class="note">asking the language server…</div>';
  if (r.error) return `<div class="note bad">${esc(r.error)}</div>`;
  if (!r.rows.length) return '<div class="note">none</div>';
  return `<div class="note">${r.rows.map((x) => `${x.symbolName ? `${esc(x.symbolName)} ` : ''}${at(x.path, x.line ?? 1)} <span class="k" data-select="${esc(x.path)}">→</span>`).join('<br>')}</div>`;
}

function symbolRow(file, s) {
  const calls = [...P.S.calls.values()];
  const out = calls.filter((c) => c.source === s.id).length, inn = calls.filter((c) => c.target === s.id).length;
  const reqs = ['callers', 'callees', 'references'].map((operation) => ({ file, symbolName: s.name, line: s.line, operation }));
  const span = s.endLine && s.endLine !== s.line ? `–${s.endLine}` : '';
  return `<tr><td><b>${esc(s.name)}</b> <span class="badge">${esc(s.kind)}</span>${s.exported ? '<span class="badge">exported</span>' : ''}<div>${at(file, s.line ?? 1)}${span} · ${out} out · ${inn} in</div><div>${reqs.map((r) => `<button data-lsp="${attr(r)}">${r.operation}</button>`).join(' ')}</div>${reqs.map((r) => lspAnswer(P.S.lspResults.get(JSON.stringify(r)))).join('')}</td></tr>`;
}

function astTree(file, list, depth) {
  return list.map((s) => `<div style="padding-left:${depth * 14}px">${at(file, s.line)} <b>${esc(s.symbolName)}</b> <span class="badge">${esc(s.kind)}</span>${s.endLine ? `<span class="note">–${s.endLine}</span>` : ''}</div>${astTree(file, s.members ?? [], depth + 1)}`).join('');
}
function astBlock(id) {
  const a = P.S.ast.get(id);
  if (!a || a.state === 'start') return '<div class="note">reading the AST outline…</div>';
  if (a.error) return `<div class="bad note">${esc(a.error)}</div>`;
  return a.symbols?.length ? astTree(id, a.symbols, 0) : '<div class="note">no declarations</div>';
}

function edgeList(list, other) {
  const row = (e) => `<tr><td>${nodeLink(endId(e[other]))}<div class="note">line ${lineLinks(endId(e.source), e.lines)}${e.via !== 'graph' ? ` · <span class="badge">${e.via}</span>` : ''}${e.test ? ' · test' : ''}</div></td><td class="n">${e.count}</td></tr>`;
  return `<table>${list.slice(0, limit('edges')).map(row).join('')}</table>${more('edges', list.length)}`;
}

const CHECK_TEXT = { ok: ['✓', 'Matches the file on disk'], stale: ['!', 'Changed since the snapshot'], missing: ['✕', 'File missing'], lines: ['✕', 'Lines outside the file'] };
function checkBlock(n) {
  const c = n.check;
  if (!n.inGraph) return '<div class="check"><span class="ic">○</span><div>Listed by structureSearch; the graph parsed no code in this file.</div></div>';
  if (!c) return '<div class="check"><span class="ic">…</span><div class="note">Not checked against the file yet (Verify phase).</div></div>';
  const [ic, text] = CHECK_TEXT[c.status];
  const facts = c.lines != null ? ` · ${num(c.lines)} lines · ${kb(c.bytes)}` : '';
  return `<div class="check ${c.status}"><span class="ic">${ic}</span><div><b>${text}</b><span class="note">${facts}</span>${(c.problems ?? []).map((p) => `<div class="note">${esc(p)}</div>`).join('')}</div></div>`;
}
const edgeSection = (title, list, other) => `<h2>${title} <span>${list.length}</span></h2>${edgeList(list, other)}`;
function metricsLine(n) {
  const m = n.metrics;
  if (!m) return '';
  const position = [...P.S.nodes.values()].filter((x) => x.metrics && x.metrics.pagerank > m.pagerank).length + 1;
  const C = P.S.metrics, community = C.communities.find((c) => c.id === m.community), cycle = m.cycle == null ? null : C.cycles[m.cycle];
  const kpis = [[`#${position}`, 'PageRank'], [fmt(m.betweenness), 'betweenness'], [fmt(m.inWeight), 'in weight'], [fmt(m.outWeight), 'out weight'], [m.depth, 'depth'], [m.core, 'k-core']];
  const sets = `${community ? `<span class="k" data-hl="${attr(community.members)}">community ${m.community} (${community.size})</span>` : '<span class="note">no community peers</span>'}${cycle ? ` · <span class="k bad" data-hl="${attr(cycle.members)}">in a ${cycle.size}-file cycle</span>` : ''}`;
  return `<div class="kpis">${kpis.map(([v, k]) => `<span class="counter"><b>${v}</b><span>${k}</span></span>`).join('')}</div><div class="note">${sets}</div>`;
}

function nodeTab() {
  const S = P.S, id = S.selected, n = id && S.nodes.get(id);
  if (!n) return '<p class="note">Click a node, search with <b>/</b>, or pick a hub or finding. Every node shows its file location and opens in your editor.</p>';
  const path = n.manifest ?? n.path;
  const edges = [...S.edges.values()];
  const by = (kind, side) => edges.filter((e) => e.kind === kind && endId(e[side]) === id).toSorted((a, b) => b.count - a.count);
  const syms = (S.symbols.get(id) ?? []).toSorted((a, b) => (a.line ?? 0) - (b.line ?? 0));
  const chips = `${n.language ? `<span class="badge">${esc(n.language)}</span>` : ''}${(n.roles ?? []).map((r) => `<span class="badge">${esc(r)}</span>`).join('')}${n.chunk ? `<span class="badge">chunk ${esc(n.chunk)}</span>` : ''}${n.inGraph ? '' : '<span class="badge">not in graph</span>'}`;
  return `<h2>${esc(n.label)} <span>${n.kind}</span></h2>
    <div class="loc"><a href="${esc(href(path, 1))}" class="open">Open</a><span class="mono">${esc(path)}</span><button data-copy="${esc(abs(path))}">copy path</button><button data-copy="${esc(`${path}:1`)}">copy path:line</button></div>
    <div>${chips} <span class="note">${n.fanIn} importers · ${n.fanOut} imports</span></div>
    ${metricsLine(n)}${checkBlock(n)}
    ${edgeSection('Imports', by('imports', 'source'), 'target')}${edgeSection('Imported by', by('imports', 'target'), 'source')}
    ${edgeSection('Calls into other files', by('calls', 'source'), 'target')}${edgeSection('Called from', by('calls', 'target'), 'source')}
    <h2>Functions <span>graph declarations · ${syms.length}</span></h2><table>${syms.slice(0, limit('functions')).map((s) => symbolRow(id, s)).join('')}</table>${more('functions', syms.length)}
    <h2>AST outline <span>astSearch symbols</span></h2>${astBlock(id)}`;
}

const TABS = { progress: progressTab, metrics: metricsTab, tools: toolsTab, node: nodeTab };
function render() {
  header();
  document.querySelectorAll('.tabs button').forEach((b) => b.classList.toggle('on', b.dataset.tab === P.tab));
  $('panel').innerHTML = TABS[P.tab]();
  $('errorsOnly')?.addEventListener('change', (e) => { P.errorsOnly = e.target.checked; render(); });
  $('rankBy')?.addEventListener('change', (e) => { P.rankBy = e.target.value; render(); });
}
function schedule() {
  if (!P.timer) P.timer = setTimeout(() => { P.timer = 0; render(); }, 180);
}

const ACTIONS = {
  select: (d) => P.select(d.select, { focus: true }),
  hl: (d) => P.highlight(JSON.parse(d.hl)),
  lsp: (d) => P.ask.lsp(JSON.parse(d.lsp)),
  more: (d) => P.shown.set(d.more, limit(d.more) + PAGE[d.more]),
  copy: (d) => { void navigator.clipboard?.writeText(d.copy); },
  tool: (d) => { P.toolFilter = d.tool; },
  expand: (d) => { const k = d.expand; if (!P.expanded.delete(k)) P.expanded.add(k); },
};
function onPanelClick(ev) {
  const t = ev.target.closest('[data-select],[data-hl],[data-lsp],[data-more],[data-copy],[data-tool],[data-expand]');
  if (!t) return;
  const name = Object.keys(ACTIONS).find((k) => t.dataset[k] != null);
  ACTIONS[name](t.dataset);
  render();
}

// Toolbar: edge kinds, weak links, tests, non-code files, labels, color key, layers, 2D/3D, overview, collapse,
// minimum link weight; legend; editor choice.
function bindToolbar() {
  const TOGGLES = [['imports', 'Imports'], ['calls', 'Calls'], ['weak', 'Weak links'], ['tests', 'Tests'], ['docs', 'Non-code files'], ['labels', 'Labels']];
  const g = P.graph;
  const views = '<button data-layout="layers" title="place files by dependency depth">Layers</button><button data-dims title="switch between a flat 2D and a 3D layout">3D</button><button data-overview title="one node per community; click a community to expand it">Overview</button><button data-collapse title="fold every expanded community back">Collapse all</button>';
  const weight = '<label class="minw" title="hide links lighter than this weight (log scale)">min weight <input id="minW" type="range" min="0" max="1" step="0.01" value="0" aria-label="minimum link weight"><output id="minWOut">all</output></label>';
  $('toggles').innerHTML = `${TOGGLES.map(([k, l]) => `<button data-toggle="${k}" class="${g.toggles[k] ? 'on' : ''}">${l}</button>`).join('')}<select id="colorBy" aria-label="color nodes by"><option value="group">color: package / folder</option><option value="chunk">color: chunk</option><option value="language">color: language</option><option value="community">color: community</option><option value="depth">color: depth</option></select>${views}${weight}`;
  $('toggles').addEventListener('click', (ev) => {
    const b = ev.target.closest('button');
    if (!b) return;
    const d = b.dataset;
    if (d.toggle) b.classList.toggle('on', g.toggle(d.toggle));
    if (d.layout) { g.setLayout(g.layout === 'layers' ? 'force' : 'layers'); b.classList.toggle('on', g.layout === 'layers'); }
    if (d.dims != null) g.setDimensions(g.dimensions === 3 ? 2 : 3);
    if (d.overview != null) g.setOverview(!g.overview);
    if (d.collapse != null) g.collapseAll();
    legend();
  });
  $('minW').addEventListener('input', (e) => {
    const w = g.setMinWeight(Number(e.target.value));
    $('minWOut').value = w > 0 ? `≥ ${w < 10 ? w.toFixed(1) : Math.round(w)}` : 'all';
  });
  $('colorBy').addEventListener('change', (e) => { g.colorBy = e.target.value; legend(); });
  document.addEventListener('gv:graph', legend);
  $('fit').addEventListener('click', () => g.fit());
  try { P.editor = localStorage.getItem('gv-editor') ?? P.editor; } catch { /* default editor */ }
  $('editor').value = P.editor;
  $('editor').addEventListener('change', (e) => { P.editor = e.target.value; try { localStorage.setItem('gv-editor', P.editor); } catch { /* per-viewer only */ } render(); });
}

// Search across files and functions; Enter or click selects and zooms.
function suggest() {
  const s = $('q').value.trim().toLowerCase(), box = $('suggest');
  if (!s) { box.hidden = true; return; }
  const files = [...P.S.nodes.values()].filter((n) => n.path.toLowerCase().includes(s)).toSorted((a, b) => b.fanIn - a.fanIn).slice(0, 8).map((n) => ({ id: n.id, text: n.path, note: `${n.fanIn} in` }));
  const fns = [...P.S.symbols.values()].flat().filter((x) => x.name?.toLowerCase().includes(s)).slice(0, 6).map((x) => ({ id: x.file, text: `${x.name}()`, note: `${x.file.split('/').pop()}:${x.line}` }));
  P.hits = [...files, ...fns];
  P.cursor = 0;
  box.innerHTML = P.hits.map((h, i) => `<div data-i="${i}" class="${i === 0 ? 'on' : ''}"><span class="mono">${esc(h.text)}</span><span class="note">${esc(h.note)}</span></div>`).join('') || '<div class="note">no match</div>';
  box.hidden = false;
}
function pick(i) {
  const h = P.hits[i];
  if (!h) return;
  $('suggest').hidden = true;
  $('q').blur();
  P.select(h.id, { focus: true });
}
function onSearchKey(ev) {
  const box = $('suggest');
  if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
    P.cursor = (P.cursor + (ev.key === 'ArrowDown' ? 1 : P.hits.length - 1)) % Math.max(1, P.hits.length);
    [...box.children].forEach((c, i) => c.classList.toggle('on', i === P.cursor));
    ev.preventDefault();
  } else if (ev.key === 'Enter') pick(P.cursor);
  else if (ev.key === 'Escape') { box.hidden = true; $('q').blur(); }
}
function onKey(ev) {
  if (ev.target.matches('input,select,textarea')) return;
  if (ev.key === '/') { ev.preventDefault(); $('q').focus(); }
  else if (ev.key === 'Escape') { P.select(null); P.highlight([]); }
  else if (ev.key === 'f') P.graph.fit();
  else if (/^[1-4]$/.test(ev.key)) { P.tab = Object.keys(TABS)[Number(ev.key) - 1]; render(); }
}

function feed(text, level = 'info') {
  const el = document.createElement('div');
  el.className = level;
  el.textContent = text;
  $('feed').prepend(el);
  while ($('feed').children.length > 5) $('feed').lastChild.remove();
  setTimeout(() => el.remove(), 6000);
}

export function createPanels(S, { select, highlight, ask, graph }) {
  Object.assign(P, { S, select, highlight, ask, graph });
  bindToolbar();
  $('panel').addEventListener('click', onPanelClick);
  document.querySelector('.tabs').addEventListener('click', (ev) => { if (ev.target.dataset.tab) { P.tab = ev.target.dataset.tab; render(); } });
  $('q').addEventListener('input', suggest);
  $('q').addEventListener('keydown', onSearchKey);
  $('suggest').addEventListener('mousedown', (ev) => { const d = ev.target.closest('[data-i]'); if (d) pick(Number(d.dataset.i)); });
  document.addEventListener('keydown', onKey);
  render();
  return { schedule, feed, render, tab: (t) => { P.tab = t; render(); } };
}
