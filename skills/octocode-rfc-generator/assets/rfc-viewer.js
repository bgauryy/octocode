// Octocode RFC viewer. Inlined into assets/rfc-viewer.html by scripts/render-rfc.mjs.
(() => {
'use strict';
const MARKED = 'https://cdn.jsdelivr.net/npm/marked@18.0.14/lib/marked.umd.js';
const MERMAID = 'https://cdn.jsdelivr.net/npm/mermaid@12.1.0/dist/mermaid.esm.min.mjs';
const META = {
  'RFC.md': ['⚖️', 'Decision'], 'PLAN.md': ['🧭', 'Plan'], 'IMPLEMENTATION.md': ['🛠️', 'Implementation'],
  'PREREQUISITES.md': ['🧱', 'Prerequisites'], 'KPI.md': ['🎯', 'Acceptance & KPIs'],
  'RESOURCES.md': ['📚', 'Sources'], 'AUDIT.md': ['🔎', 'Audit'], 'README.md': ['📄', 'Readme'],
};
const $ = (s, r = document) => r.querySelector(s);
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const reEsc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const cls = (s) => esc(String(s).toLowerCase().split(/\s+/)[0]);

let data = null;
try { data = JSON.parse($('#rfc-data').textContent); } catch { /* template opened without data */ }
if (!data || !Array.isArray(data.files) || !data.files.length) {
  $('#page').innerHTML = '<h1>No RFC data</h1><p>Build this page with <code>node scripts/render-rfc.mjs &lt;rfc-folder&gt;</code>.</p>';
  return;
}

const files = data.files.map((f) => {
  const base = f.path.split('/').pop();
  const h1 = (f.content.match(/^#\s+(.+)$/m) || [])[1];
  const done = (f.content.match(/^\s*[-*+]\s+\[[xX]\]/gm) || []).length;
  const open = (f.content.match(/^\s*[-*+]\s+\[ \]/gm) || []).length;
  const diagrams = (f.content.match(/^\s*(```|~~~)\s*mermaid/gm) || []).length;
  const status = (f.content.match(/^\s*(?:[-*]\s*)?\**Status\**\s*:\**\s*`?([A-Za-z][\w -]{0,30})/mi) || [])[1];
  const para = f.content.replace(/```[\s\S]*?```/g, '').split(/\n\s*\n/).map((p) => p.trim())
    .find((p) => p && !/^(#|\||>|[-*]\s|\d+\.\s|<!--|\**[\w ]{1,24}\**\s*:)/.test(p)) || '';
  const [icon, kind] = META[base] || ['📄', base.replace(/\.md$/i, '')];
  return {
    ...f, base, icon, kind, done, open, diagrams,
    h1: h1 ? h1.replace(/[`*_]/g, '') : base,
    status: status ? status.trim() : '',
    words: f.content.split(/\s+/).length,
    summary: para.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*_#>]/g, '').replace(/\s+/g, ' ').slice(0, 280),
  };
});
const byPath = new Map(files.map((f) => [f.path, f]));
const primary = files.find((f) => /^(RFC|PLAN)\.md$/i.test(f.base)) || files[0];
const title = data.title || primary.h1;
const statusText = primary.status;
document.title = `${title} · RFC`;
$('#title').textContent = title;
if (statusText) Object.assign($('#status'), { hidden: false, textContent: statusText, className: `badge ${cls(statusText)}` });

// Theme: saved choice, else the OS preference.
const setTheme = (t) => { document.documentElement.dataset.theme = t; try { localStorage.setItem('rfc-theme', t); } catch { /* private mode */ } };
let savedTheme = null;
try { savedTheme = localStorage.getItem('rfc-theme'); } catch { /* private mode */ }
setTheme(savedTheme || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'));

const toast = (msg) => { const t = $('#toast'); t.textContent = msg; t.classList.add('show'); clearTimeout(toast.h); toast.h = setTimeout(() => t.classList.remove('show'), 1400); };
const copy = (text, msg) => { navigator.clipboard?.writeText(text).then(() => toast(msg), () => toast('Copy failed')); };

// Markdown: marked (pinned CDN). Offline: a small built-in renderer keeps every page readable.
let md = null, offline = false;
const loadScript = (src) => new Promise((ok, fail) => { const s = document.createElement('script'); s.src = src; s.onload = ok; s.onerror = fail; document.head.append(s); });
function inline(t) {
  return esc(t).replace(/`([^`]+)`/g, '<code>$1</code>').replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*\w])\*([^*\s][^*]*)\*/g, '$1<em>$2</em>')
    .replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, '<img alt="$1" src="$2">')
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a href="$2">$1</a>');
}
const LIST = /^\s*([-*+]|\d+[.)])\s+/;
function fallback(src) {
  const out = [], lines = src.replace(/\r/g, '').split('\n');
  for (let i = 0; i < lines.length;) {
    const l = lines[i];
    let m;
    if ((m = l.match(/^\s*(```|~~~)\s*([\w-]*)/))) {
      const buf = []; i++;
      while (i < lines.length && !lines[i].trim().startsWith(m[1])) buf.push(lines[i++]);
      i++; out.push(`<pre><code class="language-${esc(m[2])}">${esc(buf.join('\n'))}</code></pre>`); continue;
    }
    if ((m = l.match(/^(#{1,6})\s+(.*)$/))) { out.push(`<h${m[1].length}>${inline(m[2])}</h${m[1].length}>`); i++; continue; }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(l)) { out.push('<hr>'); i++; continue; }
    if (/^\s*\|/.test(l) && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1] || '')) {
      const cells = (r) => r.trim().replace(/^\||\|$/g, '').split('|').map((c) => inline(c.trim()));
      let h = `<table><thead><tr>${cells(l).map((c) => `<th>${c}</th>`).join('')}</tr></thead><tbody>`; i += 2;
      while (i < lines.length && /^\s*\|/.test(lines[i])) h += `<tr>${cells(lines[i++]).map((c) => `<td>${c}</td>`).join('')}</tr>`;
      out.push(`${h}</tbody></table>`); continue;
    }
    if (/^\s*>/.test(l)) {
      const buf = []; while (i < lines.length && /^\s*>/.test(lines[i])) buf.push(lines[i++].replace(/^\s*>\s?/, ''));
      out.push(`<blockquote>${fallback(buf.join('\n'))}</blockquote>`); continue;
    }
    if ((m = l.match(LIST))) {
      const tag = /\d/.test(m[1]) ? 'ol' : 'ul', items = [];
      while (i < lines.length && LIST.test(lines[i])) {
        let item = lines[i++].replace(LIST, '');
        while (i < lines.length && /^\s{2,}\S/.test(lines[i]) && !LIST.test(lines[i])) item += ` ${lines[i++].trim()}`;
        const task = item.match(/^\[([ xX])\]\s+/);
        items.push(task ? `<li><input type="checkbox" disabled${task[1] === ' ' ? '' : ' checked'}> ${inline(item.slice(task[0].length))}</li>` : `<li>${inline(item)}</li>`);
      }
      out.push(`<${tag}>${items.join('')}</${tag}>`); continue;
    }
    if (!l.trim()) { i++; continue; }
    const buf = [lines[i++]];
    while (i < lines.length && lines[i].trim() && !/^(#{1,6}\s|\s*(```|~~~)|\s*>|\s*\|)/.test(lines[i]) && !LIST.test(lines[i])) buf.push(lines[i++]);
    out.push(`<p>${inline(buf.join(' '))}</p>`);
  }
  return out.join('\n');
}
async function initMarkdown() {
  try {
    await loadScript(MARKED);
    // Raw HTML renders as text: RFCs quote untrusted snippets.
    window.marked.use({ gfm: true, renderer: { html: (token) => esc(token.text || token.raw || '') } });
    md = (s) => window.marked.parse(s);
  } catch { offline = true; md = fallback; }
}
let mermaidReady = null;
const getMermaid = () => (mermaidReady ||= import(MERMAID).then((m) => m.default).catch(() => null));

// Routes: #/ overview · #/<path> document · #/<path>@<slug> heading.
function parseHash() {
  const h = decodeURIComponent(location.hash.replace(/^#\/?/, ''));
  const at = h.lastIndexOf('@');
  return at > 0 ? { path: h.slice(0, at), slug: h.slice(at + 1) } : { path: h || null };
}
const href = (path, slug) => `#/${path ? encodeURI(path) + (slug ? `@${slug}` : '') : ''}`;
const dirOf = (p) => p.slice(0, p.lastIndexOf('/') + 1);
function resolvePath(from, rel) {
  const out = [];
  for (const p of (dirOf(from) + rel).split('/')) { if (p === '..') out.pop(); else if (p && p !== '.') out.push(p); }
  return out.join('/');
}
const slugify = (t) => t.toLowerCase().trim().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s+/g, '-');

function renderNav(current) {
  const items = files.map((f) => {
    const total = f.done + f.open;
    const meter = total ? `<div class="meter" title="${f.done}/${total} checklist items done"><i style="width:${Math.round((100 * f.done) / total)}%"></i></div>` : '';
    return `<a href="${href(f.path)}" class="${current === f.path ? 'active' : ''}"${current === f.path ? ' aria-current="page"' : ''}><span class="ic">${f.icon}</span><span class="lbl">${esc(f.kind)}<span>${esc(f.path)}</span>${meter}</span></a>`;
  });
  $('#nav').innerHTML = `<a href="#/" class="${current ? '' : 'active'}"><span class="ic">🏠</span><span class="lbl">Overview<span>${files.length} documents</span></span></a>${items.join('')}`;
  const when = new Date(data.generated || Date.now()).toLocaleString();
  $('#sidefoot').innerHTML = `Built ${esc(when)}${data.root ? `<br><code>${esc(data.root)}</code>` : ''}<br>Edited a file? Run <code>render-rfc.mjs</code> again and reload.`;
}

function overview() {
  const sum = (k) => files.reduce((a, f) => a + f[k], 0);
  const done = sum('done'), total = done + sum('open'), diagrams = sum('diagrams');
  const stats = (f) => [
    f.status && `<span>${esc(f.status)}</span>`,
    f.diagrams && `<span>◇ ${f.diagrams} diagram${f.diagrams > 1 ? 's' : ''}</span>`,
    f.done + f.open && `<span>☑ ${f.done}/${f.done + f.open}</span>`,
    `<span>${Math.max(1, Math.round(f.words / 230))} min read</span>`,
  ].filter(Boolean).join('');
  $('#page').innerHTML = `
    <div class="hero"><div class="crumbs">RFC set · ${files.length} documents</div><h1 style="margin:0">${esc(title)}</h1>
      <p>${esc(primary.summary || 'Open a document to start reading.')}</p>
      <div class="metaline">${statusText ? `<span class="badge ${cls(statusText)}">${esc(statusText)}</span>` : ''}
        <span class="badge">${diagrams} diagram${diagrams === 1 ? '' : 's'}</span>
        ${total ? `<span class="badge">${done}/${total} checklist done</span>` : ''}</div></div>
    <div class="cards">${files.map((f) => `
      <a class="card" href="${href(f.path)}"><div class="top"><span class="ic">${f.icon}</span><div><h3>${esc(f.kind)}</h3><small style="color:var(--muted)">${esc(f.path)}</small></div></div>
        <p>${esc(f.summary || f.h1)}</p><div class="stats">${stats(f)}</div></a>`).join('')}</div>
    <p class="hint">Keys: <span class="k">/</span> search · <span class="k">[</span> <span class="k">]</span> previous / next document · <span class="k">g</span> overview · <span class="k">t</span> theme · <span class="k">Esc</span> close</p>`;
  $('#toc').innerHTML = '';
  renderNav(null);
  scrollTo(0, 0);
}

function decorate(art, path) {
  // Headings: GitHub-style ids, copy-link anchors, TOC entries (h2/h3).
  const seen = {}, toc = [];
  art.querySelectorAll('h1,h2,h3,h4').forEach((h) => {
    let id = slugify(h.textContent) || 'section';
    seen[id] = seen[id] == null ? 0 : seen[id] + 1;
    if (seen[id]) id += `-${seen[id]}`;
    h.id = id;
    if (/H[23]/.test(h.tagName)) toc.push(`<a href="${href(path, id)}" data-id="${esc(id)}" class="l${h.tagName[1]}">${esc(h.textContent)}</a>`);
    const a = Object.assign(document.createElement('a'), { className: 'anchor', href: href(path, id), textContent: '#', title: 'Copy link to this section' });
    a.addEventListener('click', (e) => { e.preventDefault(); history.replaceState(null, '', a.href); copy(location.href, 'Link copied'); });
    h.append(a);
  });
  $('#toc').innerHTML = toc.join('') || '<span style="color:var(--muted);font-size:13px;padding:0 10px">No sections</span>';
  art.querySelectorAll('table').forEach((t) => { const w = document.createElement('div'); w.className = 'tablewrap'; t.replaceWith(w); w.append(t); });
  art.querySelectorAll('li').forEach((li) => { if (li.querySelector(':scope > input[type=checkbox], :scope > p > input[type=checkbox]')) li.classList.add('task'); });
  // Links: other RFC documents stay in the app; external links open a tab; other relative paths open the source file.
  art.querySelectorAll('a[href]').forEach((a) => {
    const raw = a.getAttribute('href');
    if (a.classList.contains('anchor') || raw.startsWith('#/')) return;
    if (/^[a-z][\w+.-]*:/i.test(raw)) { if (!/^mailto:/i.test(raw)) Object.assign(a, { target: '_blank', rel: 'noopener' }); return; }
    if (raw.startsWith('#')) { a.href = href(path, slugify(decodeURIComponent(raw.slice(1)))); return; }
    const [file, frag] = raw.split('#');
    const target = resolvePath(path, decodeURIComponent(file));
    if (byPath.has(target)) a.href = href(target, frag && slugify(frag));
    else if (data.root) { a.href = `file://${encodeURI(`${data.root.replace(/\/$/, '')}/${target}`)}`; a.title = 'Open source file'; }
  });
  // Code: copy buttons. Mermaid: diagram cards (render later).
  let n = 0;
  art.querySelectorAll('pre > code').forEach((code) => {
    const pre = code.parentElement;
    if (/language-mermaid/.test(code.className)) {
      const src = code.textContent;
      const card = document.createElement('figure');
      card.className = 'diagram';
      card.style.margin = '20px 0';
      card.innerHTML = `<div class="bar"><b>Diagram ${++n}</b><span>${esc(src.trim().split(/\s/)[0] || '')}</span><button data-act="src" aria-pressed="false">Source</button><button data-act="copy">Copy</button><button data-act="zoom">Expand</button></div><div class="canvas">Rendering…</div><pre hidden><code></code></pre>`;
      card.querySelector('pre code').textContent = src;
      card.dataset.src = src;
      pre.replaceWith(card);
      return;
    }
    const b = Object.assign(document.createElement('button'), { className: 'copy', textContent: 'Copy' });
    b.addEventListener('click', () => { copy(code.textContent, 'Code copied'); b.textContent = 'Copied'; setTimeout(() => { b.textContent = 'Copy'; }, 1200); });
    pre.append(b);
  });
}

async function renderDiagrams(root) {
  const cards = [...root.querySelectorAll('.diagram')];
  if (!cards.length) return;
  const mermaid = await getMermaid();
  if (mermaid) mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: document.documentElement.dataset.theme === 'dark' ? 'dark' : 'neutral', fontFamily: 'system-ui, sans-serif' });
  for (const [k, card] of cards.entries()) {
    if (!card.isConnected) return; // the reader navigated away
    const canvas = card.querySelector('.canvas'), showSource = (note) => {
      canvas.hidden = true; card.querySelector('pre').hidden = false;
      card.insertAdjacentHTML('beforeend', `<div class="note">${esc(note)}</div>`);
    };
    if (!mermaid) { showSource('Mermaid did not load (offline?). Showing the diagram source.'); continue; }
    try { canvas.innerHTML = (await mermaid.render(`mmd-${Date.now()}-${k}`, card.dataset.src)).svg; } catch (e) { showSource(`Diagram error: ${String(e?.message || e).split('\n')[0]}`); }
  }
}

function highlight(root, term) {
  const rx = new RegExp(reEsc(term), 'gi');
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, { acceptNode: (n) => (n.parentElement.closest('.diagram,.anchor,mark') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT) });
  const hits = [];
  while (walker.nextNode()) { rx.lastIndex = 0; if (rx.test(walker.currentNode.nodeValue)) hits.push(walker.currentNode); }
  for (const node of hits) {
    const span = document.createElement('span');
    span.innerHTML = esc(node.nodeValue).replace(new RegExp(reEsc(esc(term)), 'gi'), (m) => `<mark>${m}</mark>`);
    node.replaceWith(span);
  }
  root.querySelector('mark')?.scrollIntoView({ block: 'center' });
}

let spyObs;
function spy() {
  spyObs?.disconnect();
  const links = new Map([...document.querySelectorAll('#toc a')].map((a) => [a.dataset.id, a]));
  if (!links.size) return;
  spyObs = new IntersectionObserver((entries) => {
    const e = entries.find((x) => x.isIntersecting);
    if (!e) return;
    links.forEach((a) => a.classList.remove('active'));
    links.get(e.target.id)?.classList.add('active');
  }, { rootMargin: '-70px 0px -70% 0px' });
  links.forEach((_, id) => { const el = document.getElementById(id); if (el) spyObs.observe(el); });
}

let pendingTerm = '';
async function show() {
  const { path, slug } = parseHash();
  document.body.classList.remove('menu');
  if (!path) { overview(); return; }
  const f = byPath.get(path);
  if (!f) { overview(); toast(`Not found: ${path}`); return; }
  const art = $('#page');
  if (art.dataset.path !== path) {
    art.dataset.path = path;
    art.innerHTML = `${offline ? '<div class="offline">Offline: basic rendering. Diagrams show their source.</div>' : ''}<div class="crumbs"><a href="#/">Overview</a> › <span>${esc(f.kind)}</span> · <code>${esc(f.path)}</code>${f.status ? ` <span class="badge ${cls(f.status)}">${esc(f.status)}</span>` : ''}</div>${md(f.content)}`;
    decorate(art, path);
    const i = files.indexOf(f), prev = files[i - 1], next = files[i + 1];
    art.insertAdjacentHTML('beforeend', `<nav class="pager">${prev ? `<a href="${href(prev.path)}"><small>← Previous</small>${esc(prev.kind)}</a>` : '<span></span>'}${next ? `<a class="next" href="${href(next.path)}"><small>Next →</small>${esc(next.kind)}</a>` : ''}</nav>`);
    renderNav(path);
    spy();
    if (!slug) scrollTo(0, 0);
    await renderDiagrams(art);
  }
  if (slug) { const el = document.getElementById(slug); if (el) { el.scrollIntoView(); el.classList.remove('flash'); void el.offsetWidth; el.classList.add('flash'); } }
  if (pendingTerm) { highlight(art, pendingTerm); pendingTerm = ''; }
}
window.__rfcViewer = { files, render: (s) => md(s), slugify, resolvePath };

// Search: every document, ranked by title/heading hits, then body hits.
const q = $('#q'), results = $('#results');
let hits = [], active = 0;
function search(term) {
  term = term.trim();
  if (term.length < 2) { results.classList.remove('open'); return; }
  const rx = new RegExp(reEsc(term), 'gi');
  hits = [];
  for (const f of files) {
    let heading = '';
    const lines = f.content.split('\n');
    for (const line of lines) {
      const h = line.match(/^#{1,4}\s+(.+)/);
      if (h) heading = h[1].replace(/[`*_]/g, '');
      rx.lastIndex = 0;
      if (!rx.test(line)) continue;
      const at = line.search(rx), from = Math.max(0, at - 50);
      const snip = (from ? '…' : '') + line.slice(from, at + term.length + 70).replace(/[`*#|>]/g, ' ');
      hits.push({ f, heading, snip, score: h ? 0 : 1 });
      if (hits.length > 60) break;
    }
  }
  hits.sort((a, b) => a.score - b.score);
  active = 0;
  const mk = (s) => esc(s).replace(new RegExp(reEsc(esc(term)), 'gi'), (m) => `<mark>${m}</mark>`);
  results.innerHTML = hits.length
    ? hits.slice(0, 40).map((h, i) => `<a class="result${i ? '' : ' active'}" role="option" data-i="${i}"><small>${h.f.icon} ${esc(h.f.kind)}${h.heading ? ` › ${esc(h.heading)}` : ''}</small><span class="snip">${mk(h.snip)}</span></a>`).join('')
    : `<div class="empty">No match for “${esc(term)}”.</div>`;
  results.classList.add('open');
}
function openHit(i) {
  const h = hits[i];
  if (!h) return;
  pendingTerm = q.value.trim();
  results.classList.remove('open');
  q.blur();
  const slug = h.heading ? slugify(h.heading) : '';
  const next = href(h.f.path, slug);
  if (location.hash === next) { $('#page').dataset.path = ''; show(); } else location.hash = next;
}
q.addEventListener('input', () => search(q.value));
q.addEventListener('focus', () => q.value && search(q.value));
q.addEventListener('keydown', (e) => {
  const items = [...results.querySelectorAll('.result')];
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    active = (active + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % Math.max(items.length, 1);
    items.forEach((el, i) => el.classList.toggle('active', i === active));
    items[active]?.scrollIntoView({ block: 'nearest' });
  } else if (e.key === 'Enter') { e.preventDefault(); openHit(active); }
  else if (e.key === 'Escape') { results.classList.remove('open'); q.blur(); }
});
results.addEventListener('mousedown', (e) => { const r = e.target.closest('.result'); if (r) { e.preventDefault(); openHit(Number(r.dataset.i)); } });
document.addEventListener('click', (e) => { if (!e.target.closest('.search')) results.classList.remove('open'); });

// Diagram toolbar and zoom modal.
const modal = $('#modal');
document.addEventListener('click', (e) => {
  const btn = e.target.closest('.diagram .bar button');
  if (!btn) return;
  const card = btn.closest('.diagram'), act = btn.dataset.act;
  if (act === 'copy') copy(card.dataset.src, 'Diagram source copied');
  if (act === 'src') {
    const pre = card.querySelector('pre'), canvas = card.querySelector('.canvas');
    const showSrc = pre.hidden;
    pre.hidden = !showSrc; canvas.hidden = showSrc || !canvas.querySelector('svg') && !canvas.textContent.trim();
    btn.textContent = showSrc ? 'Diagram' : 'Source'; btn.setAttribute('aria-pressed', String(showSrc));
  }
  if (act === 'zoom') {
    const svg = card.querySelector('.canvas svg');
    if (!svg) { toast('Nothing to expand'); return; }
    $('#modalbox').innerHTML = svg.outerHTML;
    modal.classList.add('open');
  }
});
modal.addEventListener('click', () => modal.classList.remove('open'));

// Header controls, keyboard, reading progress.
$('#home').addEventListener('click', () => { location.hash = '#/'; });
$('#home').addEventListener('keydown', (e) => { if (e.key === 'Enter') location.hash = '#/'; });
$('#theme').addEventListener('click', () => {
  setTheme(document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark');
  $('#page').dataset.path = ''; show();
});
$('#print').addEventListener('click', () => print());
$('#menu').addEventListener('click', () => document.body.classList.toggle('menu'));
document.addEventListener('keydown', (e) => {
  if (e.metaKey || e.ctrlKey || e.altKey || /input|textarea|select/i.test(e.target.tagName)) return;
  const { path } = parseHash(), i = path ? files.findIndex((f) => f.path === path) : -1;
  if (e.key === '/') { e.preventDefault(); q.focus(); q.select(); }
  else if (e.key === 'Escape') { modal.classList.remove('open'); document.body.classList.remove('menu'); }
  else if (e.key === ']' && files[i + 1]) location.hash = href(files[i + 1].path);
  else if (e.key === '[') location.hash = i > 0 ? href(files[i - 1].path) : '#/';
  else if (e.key === 'g') location.hash = '#/';
  else if (e.key === 't') $('#theme').click();
});
addEventListener('scroll', () => {
  const max = document.documentElement.scrollHeight - innerHeight;
  $('#progress').style.width = `${max > 0 ? (100 * scrollY) / max : 0}%`;
}, { passive: true });
addEventListener('hashchange', show);

initMarkdown().then(show);
})();
