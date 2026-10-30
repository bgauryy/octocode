// Findings that need more than one layer to state: computed from graph data plus whatever evidence ran.
export function synthesize(D) {
  const out = [];
  const add = (level, text, layers) => out.push({ level, text, layers });
  const V = D.verify;
  if (D.mode === 'overview') {
    const built = D.chunks.filter((c) => c.built), todo = D.chunks.filter((c) => !c.built);
    const total = D.chunks.reduce((a, c) => a + c.files, 0);
    add('info', `${D.chunks.length} chunks cover ${total} files${D.area ? ` under ${D.area}` : ''}; ${built.length} built (${built.map((c) => c.path.split('/').pop()).join(', ') || 'none'}), ${todo.length} not built.${todo.length ? ` Build one with chunk.mjs --only ${todo[0].id}.` : ''}`, ['plan']);
    if (D.uncovered > 0) add('info', `${D.uncovered} of ${total + D.uncovered} files sit in groups under 10 files and belong to no chunk.`, ['plan']);
    const via = (k) => D.imports.filter((e) => e.via.includes(k)).length;
    add('info', `${D.imports.length} links between chunks (${via('declared')} declared in manifests, ${via('observed')} observed in imports).`, ['manifest', 'graph']);
    const cyc = built.filter((c) => c.filesInCycles != null).sort((a, b) => b.filesInCycles - a.filesInCycles)[0];
    if (cyc) add('warn', `${cyc.path} has the most files in import cycles among built chunks (${cyc.filesInCycles} of ${cyc.files}).`, ['graph']);
    const lonely = D.files.filter((n) => !n.fanIn && !n.fanOut).length;
    if (lonely) add('info', `${lonely} of ${D.files.length} chunks have no declared or observed link to another chunk.`, ['manifest']);
    return out;
  }
  const short = (id) => id;

  // Islands: connected components of the imports shown.
  const parent = new Map(D.files.map((f) => [f.id, f.id]));
  const find = (x) => (parent.get(x) === x ? x : (parent.set(x, find(parent.get(x))), parent.get(x)));
  for (const e of D.imports) if (parent.has(e.source) && parent.has(e.target)) parent.set(find(e.source), find(e.target));
  const comps = new Map();
  for (const f of D.files) comps.set(find(f.id), [...(comps.get(find(f.id)) ?? []), f]);
  const islands = [...comps.values()].sort((a, b) => b.length - a.length);
  if (islands.length > 1) {
    const describe = (c) => `${c.length} files in ${[...new Set(c.map((f) => f.group))].join('/')}`;
    add('info', `The ${D.files.length} busiest files form ${islands.length} disconnected islands (${islands.slice(0, 3).map(describe).join('; ')}): no import path joins them inside this view.`, ['graph']);
  }

  // Concentration: how much of the shown in-degree the top hubs hold.
  const total = D.files.reduce((a, f) => a + f.fanIn, 0);
  const top = [...D.files].sort((a, b) => b.fanIn - a.fanIn).slice(0, 3);
  if (total && D.files.length >= 6) add('info', `${top.map((f) => short(f.id)).join(', ')} account for ${Math.round((100 * top.reduce((a, f) => a + f.fanIn, 0)) / total)}% of the importers counted across the files in view: many files depend on them, so edits there have wide reach.`, ['graph']);

  // Cycles and how many of their in-view edges are soft.
  const cyc = D.insights.cycles;
  if (cyc.total) {
    const big = cyc.top[0];
    const rustOnly = cyc.top.length > 0 && cyc.top.every((c) => c.nodes.every((n) => n.endsWith('.rs')));
    const rt = rustOnly ? '; all are Rust module cycles, which may be legal inside one crate (the graph scores them at most 0.35 confidence; its runtime flag means module-load cycles, which Rust does not have)' : cyc.runtime == null ? '' : cyc.runtime ? `; ${cyc.runtime} flagged as runtime cycles (largest ${cyc.largestRuntime} files)` : '; the graph flags none as a module-load cycle';
    add(cyc.runtime && !rustOnly ? 'warn' : 'info', `${cyc.total} import cycles${rt}${big ? `; the largest overall spans ${big.size} files (${short(big.nodes[0]).split('/').slice(0, -1).join('/')})` : ''}.`, ['graph']);
  }

  const cs = D.callStats;
  if (cs?.resolved) {
    const total = cs.both + cs.graphOnly + cs.lspOnly;
    add(cs.graphOnly + cs.lspOnly ? 'info' : 'info', `Function calls from ${cs.resolved} of ${cs.probed} probed functions were cross-checked with the language server: ${cs.both} of ${total} edges found by both, ${cs.graphOnly} only by the graph (for example calls inside closures, which LSP call hierarchy skips), ${cs.lspOnly} only by LSP (calls the graph could not type).`, ['graph', 'lsp']);
  } else if (cs?.mode === 'both') add('info', `Function calls were not cross-checked with the language server: ${cs.lsp}.`, ['lsp']);
  if (V) {
    const edges = V.leads.filter((l) => l.kind === 'edge');
    const tally = edges.reduce((a, l) => ({ ...a, [l.verdict]: (a[l.verdict] ?? 0) + 1 }), {});
    if (edges.length) add('info', `${edges.length} of the ${D.imports.length} imports in view were tested in depth (the rest were only confirmed to exist): ${Object.entries(tally).map(([k, v]) => `${v} ${k}`).join(', ')}.`, ['ast', 'usage', 'clasify']);
    for (const l of edges.filter((x) => x.verdict === 'unused')) add('warn', `${short(l.file)} imports ${short(l.target)} but never uses ${l.signals.source.names.join(', ')}: a stale import or an edge the graph over-counts.`, ['graph', 'ast', 'usage']);
    for (const l of edges.filter((x) => x.verdict === 'conflict')) add('warn', `Layers disagree on ${short(l.file)} → ${short(l.target)}: ${l.why}`, ['usage', 'clasify']);
    const c = V.connections;
    if (c) add(c.unconfirmed.length ? 'warn' : 'info', `The parsed AST confirms ${c.confirmed} of ${c.checked} graph import edges in view (${c.viaImport} by import statement, ${c.viaPath} by inline path)${c.na ? ` (${c.na} in files the AST layer cannot parse)` : ''}${c.unconfirmed.length ? `; unconfirmed: ${c.unconfirmed.slice(0, 3).map((e) => `${short(e.source)} → ${short(e.target)}`).join(', ')}` : ''}${c.stale?.length ? `; ${c.stale.length} edge(s) from ${[...new Set(c.stale.map((e) => short(e.source)))].slice(0, 3).join(', ')} are stale: the file changed after the graph was built, rebuild to retest` : ''}.${c.mutual.length ? ` ${c.mutual.length} mutual import pair(s): ${c.mutual.slice(0, 2).map((p) => p.map(short).join(' ↔ ')).join('; ')}.` : ''}`, ['graph', 'ast']);
    const files = V.leads.filter((l) => l.kind === 'file' && l.verdict === 'corroborated');
    if (files.length) add('warn', `Mixed responsibilities supported for ${files.map((l) => short(l.file)).join(', ')} (graph structure and clasify agree); read them before splitting.`, ['graph', 'clasify']);
    const notSupported = V.leads.filter((l) => l.kind === 'file' && l.verdict === 'not-supported');
    if (notSupported.length) add('info', `The graph's mixed-duties finding is not supported for ${notSupported.map((l) => short(l.file)).join(', ')} (clasify disagrees or the file is small with one owner); these files are still hubs, only the mixed-duties claim is dropped.`, ['graph', 'clasify']);
    const H = V.history;
    if (H) {
      const q = H.quality;
      if (!q.small) add('info', `History is too coarse for co-change analysis: none of ${q.commits} commits touch ${q.maxCommitFiles} files or fewer (median ${q.medianFiles} files per commit); churn counts below only say how many snapshots changed a file.`, ['history']);
      const top = H.hotspots.slice(0, 3);
      if (top.length) add('info', `Hotspots (fan-in x commits): ${top.map((h) => `${short(h.id)} (${h.fanIn} importers, ${h.commits} commits${h.recent90 ? `, ${h.recent90} of them in the last 90 days` : ''})`).join('; ')}.`, ['graph', 'history']);
      if (H.top?.length) add('info', `Most changed files in the section (all commits): ${H.top.slice(0, 3).map((t) => `${t.id} (${t.commits})`).join('; ')}.`, ['history']);
      const hidden = H.coChange.filter((c) => !c.imported);
      if (hidden.length) add('warn', `Hidden coupling candidates (change together in small commits, no import between them): ${hidden.slice(0, 3).map((c) => `${short(c.a)} + ${short(c.b)} (${c.count}x)`).join('; ')}.`, ['history', 'graph']);
    }
    const L = V.render?.layout?.functions;
    if (L && (L.overlaps || L.hidden)) add(L.overlaps ? 'warn' : 'info', `Functions view: ${L.overlaps} overlapping labels, ${L.hidden ?? 0} labels hidden to avoid collisions (shown on hover); Files view: ${V.render.layout.files.overlaps} overlaps (measured in the browser).`, ['dom']);
  }
  if (D.model?.upward?.length) add('risk', `${D.model.upward.length} upward import(s) against the declared layers: ${D.model.upward.slice(0, 2).map((e) => `${short(e.source)} → ${short(e.target)}`).join('; ')}.`, ['graph', 'model']);
  return out;
}
