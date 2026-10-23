// Page behavior. Inlined into the HTML via Function#toString; reads #report-data. The page views
// (document, widgets, diagrams, search) arrive in `views`, inlined the same way, and share `ctx`.
export function client(kit, MERMAID_URL, PANZOOM_URL, views) {
  const { esc, slugify } = kit;
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const reEsc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const decode = (value) => { try { return decodeURIComponent(value); } catch { return value; } };
  const store = {
    get: (k) => { try { return localStorage.getItem(`octocode-report-${k}`); } catch { return null; } },
    set: (k, v) => { try { localStorage.setItem(`octocode-report-${k}`, v); } catch { /* private mode */ } },
  };

  let data = null;
  try { data = JSON.parse($('#report-data').textContent); } catch { /* opened without data */ }
  if (!data || !Array.isArray(data.files) || !data.files.length) {
    $('#page').innerHTML = '<h1>No report data</h1><p>Build this page with <code>node scripts/render-report.mjs &lt;file-or-folder&gt;</code>.</p>';
    return;
  }

  const plain = (s) => String(s).replace(/(^|\n)\s*>+\s?/g, '$1').replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/[`*~#]/g, '').replace(/(^|[^\w])_+|_+(?![\w])/g, '$1').replace(/\s+/g, ' ').trim();
  const files = data.files.map((f) => {
    const h1 = (f.body.match(/^ {0,3}#\s+(.+?)\s*#*\s*$/m) || [])[1];
    const base = f.path.split('/').pop();
    const title = f.meta.title || (h1 ? plain(h1) : base.replace(/\.md$/i, ''));
    const afterTitle = h1 ? f.body.slice(f.body.search(/^ {0,3}#\s+/m)).replace(/^.*\n/, '') : f.body;
    const para = afterTitle.split(/\n\s*\n/).map((p) => p.trim()).find((p) => p && !/^(#|```|~~~|\||[-*+] |\d+[.)] |<|!\[|---|>\s*\[!)/.test(p));
    const date = f.meta.date || (f.path.match(/(\d{4}-\d{2}-\d{2})/) || [])[1] || '';
    return { ...f, base, title, lead: para ? plain(para) : '', date };
  });
  const byPath = new Map(files.map((f) => [f.path, f]));
  const library = files.length > 1;
  const home = library ? data.primary || null : files[0].path;
  const siteTitle = data.title || (library ? data.rootName : files[0].title);
  $('#site-title').textContent = siteTitle;
  document.body.classList.toggle('single', !library);

  const setTheme = (t) => { document.documentElement.dataset.theme = t; store.set('theme', t); };
  setTheme(store.get('theme') || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'));
  let editor = store.get('editor') || 'vscode';
  $('#editor').value = editor;
  const editorUrl = (abs, line) => {
    const p = abs.replace(/\\/g, '/');
    const lead = p.startsWith('/') ? '' : '/';
    return editor === 'file' ? `file://${lead}${kit.encodePath(p)}` : `${editor}://file${lead}${kit.encodePath(p)}${line ? `:${line}` : ''}`;
  };

  const toast = (msg) => { const t = $('#toast'); t.textContent = msg; t.classList.add('show'); clearTimeout(toast.h); toast.h = setTimeout(() => t.classList.remove('show'), 1600); };
  const copy = async (text, msg) => {
    try {
      if (!navigator.clipboard?.writeText) throw new Error('Clipboard unavailable');
      await navigator.clipboard.writeText(text);
      toast(msg);
      return true;
    } catch {
      toast('Copy unavailable. Select the source text and copy it manually.');
      return false;
    }
  };

  // Routes: #/ library, #/<path> document, #/<path>@<slug> section.
  const parseHash = () => kit.routeParse(location.hash);
  const href = kit.routeHref;
  const dirOf = (p) => p.slice(0, p.lastIndexOf('/') + 1);
  const resolvePath = (from, rel) => {
    const out = [];
    for (const seg of (dirOf(from) + rel).split('/')) { if (seg === '..') out.pop(); else if (seg && seg !== '.') out.push(seg); }
    return out.join('/');
  };

  let pendingTerm = '';
  const ctx = {
    kit, $, $$, esc, slugify, reEsc, decode, plain, files, byPath, library, siteTitle, href, resolvePath, editorUrl, toast, copy, show,
    MERMAID_URL, PANZOOM_URL, setPendingTerm: (term) => { pendingTerm = term; },
  };
  const { diagramLabel, renderDiagrams, openDiagram, closeModal, fitDiagram, modal, expanded } = views.diagramView(ctx);
  const { renderLibrary, decorate, sources } = views.documentView({ ...ctx, diagramLabel });
  const { disposeWidgets, renderWidgets } = views.widgetView(ctx);
  const { q, results } = views.searchView(ctx);

  function highlightTerm(root, term) {
    const rx = new RegExp(reEsc(term), 'gi');
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, { acceptNode: (node) => (node.parentElement.closest('.diagram,.anchor,mark,.codebar') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT) });
    const hits = [];
    while (walker.nextNode()) { rx.lastIndex = 0; if (rx.test(walker.currentNode.nodeValue)) hits.push(walker.currentNode); }
    for (const node of hits) {
      const span = document.createElement('span');
      span.innerHTML = esc(node.nodeValue).replace(new RegExp(reEsc(esc(term)), 'gi'), (m) => `<mark>${m}</mark>`);
      node.replaceWith(span);
    }
    $('mark', root)?.scrollIntoView({ block: 'center' });
  }

  let spyObs;
  function spy() {
    spyObs?.disconnect();
    const links = new Map($$('#toc a[data-id]').map((a) => [a.dataset.id, a]));
    if (!links.size) return;
    spyObs = new IntersectionObserver((entries) => {
      const e = entries.find((x) => x.isIntersecting);
      if (!e) return;
      links.forEach((a) => a.classList.remove('active'));
      links.get(e.target.id)?.classList.add('active');
    }, { rootMargin: '-80px 0px -70% 0px' });
    links.forEach((_, id) => { const el = document.getElementById(id); if (el) spyObs.observe(el); });
  }

  function renderNav(current) {
    if (!library) return;
    const term = $('#navfilter').value.trim().toLowerCase();
    const groups = new Map();
    for (const f of files) {
      if (term && !`${f.title} ${f.path}`.toLowerCase().includes(term)) continue;
      const g = f.kind || f.group || 'Documents';
      if (!groups.has(g)) groups.set(g, []);
      groups.get(g).push(f);
    }
    $('#nav').innerHTML = `<a href="#/" class="nav-home${current ? '' : ' active'}">Overview</a>`
      + [...groups].map(([g, list]) => `<div class="nav-group"><h3>${esc(g)}</h3>${list.map((f) => `<a href="${href(f.path)}" class="${f.path === current ? 'active' : ''}"><span>${esc(f.title)}</span>${f.date ? `<small>${esc(f.date)}</small>` : ''}</a>`).join('')}</div>`).join('')
      + (groups.size ? '' : '<p class="muted">No document matches.</p>');
  }

  async function show() {
    const { path: requested, slug } = parseHash();
    const p = requested || home;
    document.body.classList.remove('menu');
    const art = $('#page');
    if (!p) {
      if (art.dataset.path !== '') { disposeWidgets(); art.dataset.path = ''; renderLibrary(); renderNav(null); spy(); scrollTo(0, 0); }
      document.title = siteTitle;
      return;
    }
    const f = byPath.get(p);
    if (!f) { toast(`Not found: ${p}`); location.hash = '#/'; return; }
    if (art.dataset.path !== p) {
      art.dataset.path = p;
      disposeWidgets();
      art.innerHTML = kit.parse(f.body);
      $$('img', art).forEach(img => {
        const raw = img.getAttribute('src');
        if (f.images?.[raw]) img.src = f.images[raw];
        img.addEventListener('error', () => {
          if (img.nextElementSibling?.classList.contains('image-error')) return;
          const note = document.createElement('p'); note.className = 'image-error';
          note.textContent = 'Image could not load: ' + (img.alt || raw); img.after(note);
        });
      });
      if (!$('h1', art)) art.insertAdjacentHTML('afterbegin', `<h1>${esc(f.title)}</h1>`);
      decorate(art, f);
      renderNav(p);
      spy();
      document.title = library ? `${f.title} – ${siteTitle}` : f.title;
      if (!slug) scrollTo(0, 0);
      renderDiagrams(art);
      renderWidgets(art);
    }
    if (slug) {
      const el = document.getElementById(slug);
      if (el) { el.scrollIntoView(); el.classList.remove('flash'); void el.offsetWidth; el.classList.add('flash'); }
    }
    if (pendingTerm) { highlightTerm(art, pendingTerm); pendingTerm = ''; }
  }

  // ----- Global interactions -----
  document.addEventListener('click', (e) => {
    if (e.target.closest('a.skip')) { e.preventDefault(); $('#page').focus(); return; }
    if (!e.target.closest('.search')) results.classList.remove('open');
    const anchor = e.target.closest('a.anchor');
    if (anchor) { e.preventDefault(); history.replaceState(null, '', anchor.getAttribute('href')); copy(location.href, 'Link copied'); return; }
    const jump = e.target.closest('[data-jump]');
    if (jump) {
      e.preventDefault();
      const el = document.getElementById(jump.dataset.jump);
      if (el) { el.scrollIntoView({ block: 'center' }); el.classList.remove('flash'); void el.offsetWidth; el.classList.add('flash'); }
      return;
    }
    const scroll = e.target.closest('[data-scroll]');
    if (scroll) { e.preventDefault(); document.getElementById(scroll.dataset.scroll)?.scrollIntoView(); return; }
    const tab = e.target.closest('[data-tab]');
    if (tab) {
      $$('[data-tab]').forEach((t) => t.setAttribute('aria-selected', String(t === tab)));
      $$('.rail-panel').forEach((p) => { p.hidden = p.id !== tab.dataset.tab; });
      return;
    }
    const act = e.target.closest('[data-act]');
    if (!act) return;
    const fig = act.closest('.diagram');
    if (act.dataset.act === 'copy-sources') copy(sources().map((s) => `- [${s.label.replace(/[[\]]/g, '')}](${s.kind === 'web' ? s.key : `${data.workspace}/${s.key}`})`).join('\n'), 'Sources copied');
    if (act.dataset.act === 'copy' && fig) copy(fig.dataset.src, 'Diagram source copied');
    if (act.dataset.act === 'src' && fig) {
      const pre = $('pre', fig), canvas = $('.canvas', fig), showSrc = pre.hidden;
      if (!$('svg', canvas)) { pre.hidden = false; canvas.hidden = true; act.setAttribute('aria-pressed', 'true'); toast('Diagram unavailable; source is shown'); return; }
      pre.hidden = !showSrc;
      canvas.hidden = showSrc || !$('svg', canvas);
      act.textContent = showSrc ? 'Show diagram' : 'Show source';
      act.setAttribute('aria-pressed', String(showSrc));
    }
    if (act.dataset.act === 'download' && fig) {
      const svg = $('.canvas svg', fig);
      if (!svg) return;
      const clone = svg.cloneNode(true);
      clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
      const url = URL.createObjectURL(new Blob([new XMLSerializer().serializeToString(clone)], { type: 'image/svg+xml;charset=utf-8' }));
      const link = Object.assign(document.createElement('a'), { href: url, download: `diagram-${slugify(diagramLabel(fig.dataset.src))}.svg` });
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    }
    if (act.dataset.act === 'zoom' && fig) {
      const svg = $('.canvas svg', fig);
      if (!svg) { toast('Nothing to expand'); return; }
      openDiagram(svg, act);
    }
  });
  $('#editor').addEventListener('change', (e) => {
    editor = e.target.value;
    store.set('editor', editor);
    $$('a.coderef').forEach((a) => { a.href = editorUrl(a.dataset.abs, a.dataset.line); });
    toast(`Code links open in ${e.target.selectedOptions[0].textContent}`);
  });
  $('#navfilter').addEventListener('input', () => renderNav(parseHash().path || home));
  $('#theme').addEventListener('click', () => {
    setTheme(document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark');
    const art = $('#page');
    if (art.dataset.path) { art.dataset.path = '-'; show(); }
  });
  let printDetails = [];
  addEventListener('beforeprint', () => { printDetails = $$('.data-widget details').filter(el=>!el.open); printDetails.forEach(el=>{el.open=true;}); });
  addEventListener('afterprint', () => { printDetails.forEach(el=>{el.open=false;}); printDetails=[]; });
  $('#print').addEventListener('click', () => print());
  $('#menu').addEventListener('click', () => document.body.classList.toggle('menu'));
  $('#site-title').addEventListener('click', () => { location.hash = '#/'; });
  document.addEventListener('keydown', (e) => {
    if ($('#render-check').open) return;
    if (modal.open) {
      if (e.key === 'Escape') { e.preventDefault(); closeModal(); }
      const p = expanded()?.panzoom;
      if (p && !e.ctrlKey && !e.metaKey && !e.altKey) {
        const actions = { '+': () => p.zoomIn(), '=': () => p.zoomIn(), '-': () => p.zoomOut(), '0': fitDiagram,
          ArrowLeft: () => p.pan(40, 0, { relative: true }), ArrowRight: () => p.pan(-40, 0, { relative: true }),
          ArrowUp: () => p.pan(0, 40, { relative: true }), ArrowDown: () => p.pan(0, -40, { relative: true }) };
        if (actions[e.key]) { e.preventDefault(); actions[e.key](); }
      }
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey || /input|textarea|select/i.test(e.target.tagName)) return;
    const cur = parseHash().path || home;
    const i = files.findIndex((f) => f.path === cur);
    if (e.key === '/') { e.preventDefault(); q.focus(); q.select(); }
    else if (e.key === 'Escape') { if (modal.open) closeModal(); document.body.classList.remove('menu'); }
    else if (e.key === ']' && files[i + 1]) location.hash = href(files[i + 1].path);
    else if (e.key === '[' && library) location.hash = i > 0 ? href(files[i - 1].path) : '#/';
    else if (e.key === 'g' && library) location.hash = '#/';
    else if (e.key === 't') $('#theme').click();
  });
  addEventListener('scroll', () => {
    const max = document.documentElement.scrollHeight - innerHeight;
    $('#progress').style.transform = `scaleX(${max > 0 ? scrollY / max : 0})`;
  }, { passive: true });
  addEventListener('hashchange', show);
  const renderStatus = () => {
    const items = $$('.diagram,.data-widget', $('#page')).map(el=>({title:$('figcaption',el)?.textContent || 'Diagram',state:el.dataset.render || 'pending'}));
    $$('img', $('#page')).forEach(img=>items.push({title:img.alt || 'Image',state:img.complete ? (img.naturalWidth?'ready':'failed') : 'pending'}));
    const pending = items.filter(x=>x.state==='pending').length, failed = items.filter(x=>x.state==='failed').length;
    return {state:failed?'failed':pending?'pending':'ready',total:items.length,pending,failed,items};
  };
  const checkDialog = $('#render-check'), checkOutput = $('#render-check-output');
  let checkTimer;
  const updateCheck = () => {
    const result = renderStatus();
    checkOutput.textContent = result.state === 'ready' ? `Ready: ${result.total} visuals loaded. Check readability and links before sharing.` : `${result.state === 'failed' ? 'Failed' : 'Pending'}: ${result.failed} failed, ${result.pending} pending of ${result.total}. ${result.items.filter(x=>x.state==='failed').map(x=>x.title).join('; ')}. Review the source/error messages; reload after correcting content or connectivity.`;
  };
  $('#verify-render').onclick = () => { $$('img', $('#page')).forEach(img=>{img.loading='eager';}); updateCheck(); checkDialog.showModal(); checkTimer=setInterval(updateCheck,500); };
  $('#render-check-close').onclick = () => checkDialog.close();
  checkDialog.addEventListener('close',()=>{clearInterval(checkTimer);$('#verify-render').focus();});
  window.__octocodeReport = { files, parse: kit.parse, slugify, renderStatus };
  show();
}
