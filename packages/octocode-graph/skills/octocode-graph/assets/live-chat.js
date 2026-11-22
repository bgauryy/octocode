// Chat drawer: talk to the agent session that opened the graph about the selected node. Messages are `chat` events
// from the server (user, agent, system), so a replay restores the conversation; the drawer only sends `chat` ops.
// An agent reply can point at graph nodes (`nodes`): the newest one lights up on the graph, any one on request.
const ROLE = { user: 'You', agent: 'Agent' };
const STATE = { queued: 'Queued', delivered: 'Delivered to the agent', replied: 'Replied', failed: 'Not delivered' };
const STEPS = ['queued', 'delivered', 'replied'];
// Chip metrics, in the order the Metrics tab reads them: [key, label, digits].
const METRICS = [['pagerank', 'PageRank', 4], ['betweenness', 'betweenness', 4], ['inWeight', 'in', 1], ['outWeight', 'out', 1], ['depth', 'depth', 0], ['core', 'core', 0]];
const HTML = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
// A path-like token with an extension, optionally :line or :line:column; only known graph nodes become links.
const MENTION = /[\w@+./-]*[\w@+-]\.[A-Za-z][\w]*(?::\d+){0,2}/g;

const noop = () => {};
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => HTML[c]);
const isTyping = (el) => Boolean(el?.closest?.('input,textarea,select,[contenteditable=""],[contenteditable="true"]'));
const clock = (at) => (at ? new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '');

function nodeFor(S, token) {
  let path = token.replace(/(?::\d+){1,2}$/, '').replace(/^\.\//, '');
  if (S.root && path.startsWith(`${S.root}/`)) path = path.slice(S.root.length + 1);
  return S.nodes.has(path) ? path : null;
}

function linkify(S, raw) {
  let html = '', at = 0;
  for (const m of raw.matchAll(MENTION)) {
    const id = nodeFor(S, m[0]);
    if (!id) continue;
    html += `${esc(raw.slice(at, m.index))}<a href="#" class="chat-link" data-node="${esc(id)}" title="show in graph">${esc(m[0])}</a>`;
    at = m.index + m[0].length;
  }
  return html + esc(raw.slice(at));
}

const inline = (S, part) => part.split(/(`[^`\n]+`)/g)
  .map((p) => (p.length > 2 && p.startsWith('`') && p.endsWith('`') ? `<code>${linkify(S, p.slice(1, -1))}</code>` : linkify(S, p)))
  .join('');

// Escaped text with ``` fences as code blocks, `code` spans, and links for path:line mentions of known nodes.
function format(S, text) {
  return String(text ?? '').split(/(```[\s\S]*?```)/g)
    .map((part) => (part.length >= 6 && part.startsWith('```') && part.endsWith('```')
      ? `<pre><code>${linkify(S, part.slice(3, -3).replace(/^[\w+-]*\n/, ''))}</code></pre>`
      : inline(S, part)))
    .join('');
}

function metricsText(metrics) {
  if (!metrics) return '';
  const parts = METRICS.filter(([k]) => typeof metrics[k] === 'number').map(([k, label, d]) => `${label} ${Number.isInteger(metrics[k]) ? metrics[k] : metrics[k].toFixed(d)}`);
  if (metrics.cycle != null) parts.push(`cycle ${metrics.cycle}`);
  return parts.join(' · ');
}

// The selected node as chat context: its file (a package's manifest), line 1, and its metrics when computed.
function contextOf(S, id) {
  const n = S.nodes.get(id);
  if (!n) return null;
  return { file: n.kind === 'package' && n.manifest ? n.manifest : n.path ?? id, line: 1, ...(n.metrics ? { metrics: n.metrics } : {}) };
}

function stateHtml(d) {
  const step = STEPS.indexOf(d.state);
  const dots = STEPS.map((_, i) => `<i class="${i <= step ? 'on' : ''}"></i>`).join('');
  const label = d.state === 'failed' ? `${STATE.failed}${d.error ? `: ${esc(d.error)}` : ''}` : STATE[d.state] ?? '';
  return `<div class="chat-state ${d.state ?? ''}" title="${esc(label)}"><span class="chat-dots">${dots}</span>${label}</div>`;
}

function showHtml(d) {
  const n = Array.isArray(d.nodes) ? d.nodes.length : 0;
  if (d.role !== 'agent' || !n) return '';
  return `<button type="button" class="chat-show" data-show="${esc(d.id)}" title="${esc(d.nodes.join('\n'))}"><span class="chat-show-ic" aria-hidden="true"></span>Show on graph (${n})</button>`;
}

function messageHtml(S, d) {
  if (d.role === 'system') return `<div class="chat-text">${format(S, d.text)}</div>`;
  const ref = d.context?.file
    ? `<a href="#" class="chat-ref mono" data-node="${esc(nodeFor(S, d.context.file) ?? '')}">${esc(d.context.file)}:${d.context.line ?? 1}</a>`
    : '';
  return `<div class="chat-meta"><b>${ROLE[d.role] ?? esc(d.role)}</b><time>${clock(d.at)}</time></div>${ref}<div class="chat-text">${format(S, d.text)}</div>${d.role === 'user' ? stateHtml(d) : ''}${showHtml(d)}`;
}

const TEMPLATE = `
  <div class="chat-head">
    <div class="chat-title"><span class="chat-logo" aria-hidden="true"></span><div><b>Ask the agent</b><small>Goes to the agent session that opened this graph</small></div></div>
    <button type="button" class="chat-close" aria-label="close chat" title="close (Esc)">×</button>
  </div>
  <div class="chat-list" role="log" aria-live="polite">
    <div class="chat-empty">Select a node, then ask about it. The agent answers here; mentions like <code>src/file.ts:12</code> link to the graph.</div>
  </div>
  <form class="chat-compose">
    <div class="chat-ctx" hidden><span class="chat-chip"><span class="chat-chip-path mono"></span><small class="chat-chip-metrics"></small><button type="button" class="chat-chip-x" aria-label="remove node context" title="do not attach this node">×</button></span></div>
    <div class="chat-row">
      <textarea rows="1" placeholder="Ask about the selected node…" aria-label="message to the agent"></textarea>
      <button type="submit" class="chat-send" title="send (Enter); Shift+Enter adds a line">Send</button>
    </div>
  </form>`;

function buildDom(root) {
  root.innerHTML = TEMPLATE;
  const toggle = document.createElement('button');
  toggle.type = 'button';
  toggle.className = 'chat-toggle';
  toggle.title = 'Ask the agent (c)';
  toggle.setAttribute('aria-label', 'Ask the agent');
  toggle.setAttribute('aria-controls', 'chat');
  toggle.setAttribute('aria-expanded', 'false');
  toggle.innerHTML = '<span class="chat-ic" aria-hidden="true"></span><span class="chat-label">Ask the agent</span><span class="chat-unread" hidden></span>';
  const settings = document.querySelector('header .settings');
  if (settings) settings.prepend(toggle);
  else document.querySelector('header')?.append(toggle);
  const $ = (sel) => root.querySelector(sel);
  return {
    root, toggle, unread: toggle.querySelector('.chat-unread'), list: $('.chat-list'), empty: $('.chat-empty'), form: $('.chat-compose'),
    input: $('textarea'), ctx: $('.chat-ctx'), ctxPath: $('.chat-chip-path'), ctxMetrics: $('.chat-chip-metrics'), ctxX: $('.chat-chip-x'), close: $('.chat-close'),
  };
}

function autosize(input) {
  input.style.height = 'auto';
  input.style.height = `${input.scrollHeight}px`;
}

const nearBottom = (list) => list.scrollHeight - list.scrollTop - list.clientHeight < 80;

// K is one drawer: the store S, its DOM D, its state C and the viewer's send/select.
function renderCtx(K) {
  const { D, C } = K;
  const ctx = C.ctxId ? contextOf(K.S, C.ctxId) : null;
  D.ctx.hidden = !ctx;
  if (!ctx) return;
  D.ctxPath.textContent = `${ctx.file}:${ctx.line}`;
  D.ctxMetrics.textContent = metricsText(ctx.metrics);
  D.ctxMetrics.hidden = !D.ctxMetrics.textContent;
}

function place(K, item, stick) {
  K.D.empty.hidden = true;
  item.el.className = `chat-msg ${item.data.role ?? 'system'}`;
  item.el.innerHTML = messageHtml(K.S, item.data);
  if (stick) K.D.list.scrollTop = K.D.list.scrollHeight;
}

function setOpen(K, open) {
  const { D, C } = K;
  C.open = open;
  D.root.hidden = !open;
  D.toggle.classList.toggle('on', open);
  D.toggle.setAttribute('aria-expanded', String(open));
  if (!open) return;
  C.unread = 0;
  D.unread.hidden = true;
  renderCtx(K);
  // Mentions link to nodes known now (a message can predate the graph rows it names).
  for (const item of C.items.values()) place(K, item, false);
  D.list.scrollTop = D.list.scrollHeight;
  D.input.focus();
}

function bump(K, e) {
  if (K.S.replaying || K.C.open || e.role !== 'agent') return;
  K.C.unread++;
  K.D.unread.textContent = String(K.C.unread);
  K.D.unread.hidden = false;
}

// One bubble per message id; later events with the same id (delivery state) update it in place.
function add(K, e) {
  const stick = nearBottom(K.D.list) || e.role === 'user';
  const cur = K.C.items.get(e.id);
  if (cur) {
    Object.assign(cur.data, e);
    place(K, cur, stick);
    return;
  }
  if (!e.role) return;
  const item = { data: { ...e }, el: document.createElement('div') };
  K.C.items.set(e.id, item);
  K.D.list.append(item.el);
  place(K, item, stick);
  bump(K, e);
  if (e.role === 'agent' && e.nodes?.length && !K.S.replaying) K.highlight(e.nodes);
}

function submit(K) {
  const { D, C } = K;
  const text = D.input.value.trim();
  if (!text) return;
  if (!document.getElementById('conn')?.classList.contains('live')) {
    add(K, { id: `local-${++C.local}`, role: 'system', text: 'Not connected to the viewer server: your message is still in the box.' });
    return;
  }
  const context = C.ctxId ? contextOf(K.S, C.ctxId) : null;
  K.send({ op: 'chat', text, ...(context ? { context } : {}) });
  D.input.value = '';
  autosize(D.input);
}

function closeDrawer(K) {
  setOpen(K, false);
  K.D.toggle.focus();
}

function onSelect(K, id) {
  const { C } = K;
  if (id !== C.dismissed) C.dismissed = null;
  C.ctxId = id && id !== C.dismissed ? id : null;
  renderCtx(K);
}

function onLinkClick(K, ev) {
  const show = ev.target.closest('[data-show]');
  if (show) {
    K.highlight(K.C.items.get(show.dataset.show)?.data.nodes ?? []);
    return;
  }
  const a = ev.target.closest('[data-node]');
  if (!a) return;
  ev.preventDefault();
  if (a.dataset.node) K.select(a.dataset.node, { focus: true });
}

function bind(K) {
  const { D, C } = K;
  D.toggle.addEventListener('click', () => setOpen(K, !C.open));
  D.close.addEventListener('click', () => closeDrawer(K));
  D.form.addEventListener('submit', (ev) => { ev.preventDefault(); submit(K); });
  D.input.addEventListener('input', () => autosize(D.input));
  D.input.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter' && !ev.shiftKey && !ev.isComposing) { ev.preventDefault(); submit(K); }
    else if (ev.key === 'Escape') closeDrawer(K);
  });
  D.ctxX.addEventListener('click', () => { C.dismissed = C.ctxId; C.ctxId = null; renderCtx(K); D.input.focus(); });
  D.list.addEventListener('click', (ev) => onLinkClick(K, ev));
  document.addEventListener('gv:select', (ev) => onSelect(K, ev.detail ?? null));
  document.addEventListener('keydown', (ev) => {
    if (ev.key !== 'c' || ev.metaKey || ev.ctrlKey || ev.altKey || isTyping(ev.target)) return;
    ev.preventDefault();
    setOpen(K, !C.open);
  });
}

export function createChat(S, { send, select, highlight = noop }) {
  const root = document.getElementById('chat');
  if (!root) return { add() {} };
  const K = { S, send, select, highlight, D: buildDom(root), C: { open: false, ctxId: null, dismissed: null, items: new Map(), unread: 0, local: 0 } };
  bind(K);
  return { add: (e) => add(K, e) };
}
