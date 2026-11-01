// Query bar: the questions the graph exists for (impact, deps, path, cycle, community, callers, top, find), answered
// by the server over the whole stitched weighted graph (scripts/query.mjs) and highlighted on the graph. Answers
// are `query` events, so the agent's queries (query_graph, reply.mjs query) show here too and a replay keeps them.
const PAGE = 60;
const HINTS = ['impact pipeline.mjs', 'path a b', 'cycle x', 'top pagerank', 'callers name', 'community file', 'deps file depth 2', 'find text'];
const HTML = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => HTML[c]);
const num = (x) => (typeof x === 'number' && !Number.isInteger(x) ? x.toPrecision(4) : String(x));
const HISTORY_KEY = 'gv-query-history';

const TEMPLATE = `
  <form class="qb-form" role="search">
    <span class="qb-ic" aria-hidden="true"></span>
    <input class="qb-input" autocomplete="off" spellcheck="false" aria-label="ask the graph" placeholder="Ask the graph: ${HINTS.slice(0, 4).join(' · ')}">
  </form>
  <div class="qb-card" hidden role="region" aria-label="graph answer" aria-live="polite">
    <div class="qb-head">
      <div class="qb-title"><b class="qb-op"></b><span class="qb-q mono"></span><small class="qb-from"></small></div>
      <div class="qb-actions"><button type="button" class="qb-show" title="highlight the answer again">Show</button><button type="button" class="qb-hide" title="hide this card (Esc); the highlight stays">Hide</button><button type="button" class="qb-clear" title="clear the highlight and close">Clear</button></div>
    </div>
    <div class="qb-summary"></div>
    <div class="qb-list"></div>
  </div>`;

function tags(r) {
  const t = [];
  if (r.depth != null) t.push(`depth ${r.depth}`);
  if (r.order != null) t.push(`ring ${r.order + 1}`);
  if (r.rank != null) t.push(`#${r.rank}`);
  if (r.value != null) t.push(num(r.value));
  if (r.weight != null) t.push(`w ${num(r.weight)}`);
  if (r.pagerank != null && r.rank == null) t.push(`pr ${num(r.pagerank)}`);
  if (r.symbol) t.push(`${r.via ?? ''}${r.confidence ? `/${r.confidence}` : ''}${r.count > 1 ? ` ×${r.count}` : ''}`);
  return t.map((x) => `<span class="qb-tag">${esc(x)}</span>`).join('');
}

function rowHtml(r, i) {
  const name = r.symbol ? `${r.name ?? r.symbol.split('#').pop()}` : r.label ?? r.id;
  const sub = r.through ? `via ${r.through.split('/').pop()}` : r.next ? `→ ${r.next.split('/').pop()}` : r.symbol ? r.label : '';
  const evidence = r.evidence ? `<span class="qb-ev mono">${esc([r.evidence.imports.length ? `imports line ${r.evidence.imports.join(', ')}` : '', ...r.evidence.calls.map((c) => `${c.source.split('#').pop()} → ${c.target.split('#').pop()} (${c.via}${c.lines.length ? ` :${c.lines.join(',')}` : ''})`)].filter(Boolean).join(' · '))}</span>` : '';
  return `<button type="button" class="qb-row" data-i="${i}" title="select ${esc(r.id)}"><span class="qb-name">${esc(name)}${sub ? `<small>${esc(sub)}</small>` : ''}</span><span class="qb-tags">${tags(r)}</span><span class="qb-loc mono">${esc(r.loc?.path ?? r.id)}:${r.loc?.line ?? 1}</span>${evidence}</button>`;
}

function extrasHtml(e) {
  const out = [];
  if (e.candidates?.length) out.push(`<div class="qb-sub">Did you mean</div>${e.candidates.map((c) => `<button type="button" class="qb-cand mono" data-q="${esc(c.query)}">${esc(c.id)}</button>`).join('')}`);
  if (e.links?.length) out.push(`<div class="qb-sub">Linked communities</div>${e.links.map((l) => `<button type="button" class="qb-cand" data-q="community ${l.community}"><b>${l.community}</b> ${esc(l.label ?? '')} <span class="qb-tag">w ${num(l.weight)}</span></button>`).join('')}`);
  return out.join('');
}

function renderList(K) {
  const e = K.Q.current;
  const rows = e.rows ?? [];
  const shown = rows.slice(0, K.Q.shown);
  const more = rows.length - shown.length;
  K.D.list.innerHTML = shown.map(rowHtml).join('')
    + (more > 0 ? `<button type="button" class="qb-more">Show ${Math.min(PAGE, more)} more (${more} left)</button>` : '')
    + extrasHtml(e);
}

function render(K) {
  const e = K.Q.current;
  K.D.card.hidden = !e || !K.Q.open;
  if (!e || !K.Q.open) return;
  K.D.op.textContent = e.pending ? 'Asking…' : e.op || 'query';
  K.D.q.textContent = e.text;
  K.D.from.textContent = e.from === 'agent' ? 'asked by the agent' : '';
  K.D.summary.textContent = e.pending ? '' : e.summary ?? e.error ?? '';
  K.D.summary.classList.toggle('bad', e.ok === false);
  K.D.show.hidden = !e.nodes?.length;
  renderList(K);
}

function remember(K, text) {
  const h = K.Q.history.filter((x) => x !== text);
  h.push(text);
  K.Q.history = h;
  K.Q.at = h.length;
  try { localStorage.setItem(HISTORY_KEY, JSON.stringify(h)); } catch { /* per-viewer convenience only */ }
}

function ask(K, text) {
  const q = text.trim();
  if (!q) return;
  remember(K, q);
  K.D.input.value = q;
  K.Q.open = true;
  K.Q.shown = PAGE;
  if (!document.getElementById('conn')?.classList.contains('live')) {
    K.Q.current = { text: q, ok: false, error: 'Not connected to the viewer server.', summary: 'Not connected to the viewer server.', rows: [] };
  } else {
    K.Q.current = { text: q, pending: true, rows: [] };
    K.send({ op: 'query', text: q });
  }
  render(K);
}

// Every query event: the agent's and this page's. A replay restores history without moving the camera.
function add(K, e) {
  if (e.type !== 'query') return;
  if (e.text && !K.Q.history.includes(e.text)) K.Q.history.push(e.text);
  K.Q.at = K.Q.history.length;
  K.Q.current = e;
  K.Q.shown = PAGE;
  if (K.S.replaying) return;
  K.Q.open = true;
  K.highlight(e.ok ? e.nodes ?? [] : []);
  render(K);
}

function browse(K, step) {
  const h = K.Q.history;
  if (!h.length) return;
  K.Q.at = Math.max(0, Math.min(h.length, K.Q.at + step));
  K.D.input.value = h[K.Q.at] ?? '';
  K.D.input.setSelectionRange(K.D.input.value.length, K.D.input.value.length);
}

function onKey(K, ev) {
  if (ev.key === 'ArrowUp') { ev.preventDefault(); browse(K, -1); }
  else if (ev.key === 'ArrowDown') { ev.preventDefault(); browse(K, 1); }
  else if (ev.key === 'Escape') { K.Q.open = false; render(K); }
}

function onListClick(K, ev) {
  const t = ev.target.closest('button');
  if (!t) return;
  if (t.classList.contains('qb-more')) { K.Q.shown += PAGE; renderList(K); return; }
  if (t.dataset.q) { ask(K, t.dataset.q); return; }
  const r = K.Q.current?.rows?.[Number(t.dataset.i)];
  if (r) K.select(r.id, { focus: true });
}

function clear(K) {
  K.highlight([]);
  K.Q.open = false;
  render(K);
}

function bind(K) {
  const { D } = K;
  D.form.addEventListener('submit', (ev) => { ev.preventDefault(); ask(K, D.input.value); });
  D.input.addEventListener('keydown', (ev) => onKey(K, ev));
  D.input.addEventListener('focus', () => { if (K.Q.current && !K.Q.open) { K.Q.open = true; render(K); } });
  D.list.addEventListener('click', (ev) => onListClick(K, ev));
  D.clear.addEventListener('click', () => clear(K));
  D.hide.addEventListener('click', () => { K.Q.open = false; render(K); });
  D.show.addEventListener('click', () => K.highlight(K.Q.current?.nodes ?? []));
}

function buildDom(root) {
  root.innerHTML = TEMPLATE;
  const $ = (sel) => root.querySelector(sel);
  return { form: $('.qb-form'), input: $('.qb-input'), card: $('.qb-card'), op: $('.qb-op'), q: $('.qb-q'), from: $('.qb-from'), summary: $('.qb-summary'), list: $('.qb-list'), show: $('.qb-show'), hide: $('.qb-hide'), clear: $('.qb-clear') };
}

function savedHistory() {
  try {
    const h = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? '[]');
    return Array.isArray(h) ? h.filter((x) => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

export function createQuery(S, { send, select, highlight }) {
  const root = document.getElementById('querybar');
  if (!root) return { add() {} };
  const history = savedHistory();
  const K = { S, send, select, highlight, D: buildDom(root), Q: { history, at: history.length, current: null, open: false, shown: PAGE } };
  bind(K);
  return { add: (e) => add(K, e), ask: (text) => ask(K, text) };
}
