// Page view: the library listing, document decoration (headings, chips, links, code blocks),
// and the cited-sources rail. Inlined into the page via Function#toString: it may use only ctx
// and browser globals.
export function documentView(ctx) {
  const { kit, $, $$, esc, slugify, decode, href, resolvePath, editorUrl, copy, files, byPath, siteTitle, diagramLabel } = ctx;
  const { toneOf } = kit;
  const chip = (text, tone) => `<span class="chip${tone ? ` t-${tone}` : ''}">${esc(text)}</span>`;

  // ----- Library view -----
  let libFilter = '';
  const kindKey = (f) => f.kind || f.group || 'Documents';
  function renderLibrary() {
    $('#mobile-toc').hidden = true;
    const kinds = new Map();
    for (const f of files) kinds.set(kindKey(f), (kinds.get(kindKey(f)) || 0) + 1);
    if (libFilter && !kinds.has(libFilter)) libFilter = '';
    const list = files.filter((f) => !libFilter || kindKey(f) === libFilter)
      .sort((a, b) => (b.date || '').localeCompare(a.date || '') || kindKey(a).localeCompare(kindKey(b)) || a.title.localeCompare(b.title));
    const filters = [['', `All <small>${files.length}</small>`], ...[...kinds].sort((a, b) => a[0].localeCompare(b[0])).map(([k, n]) => [k, `${esc(k)} <small>${n}</small>`])];
    const filterHtml = (cls) => filters.map(([k, label]) => `<button class="${cls}${k === libFilter ? ' on' : ''}" data-filter="${esc(k)}" aria-pressed="${k === libFilter}">${label}</button>`).join('');
    let html = `<h1>${esc(siteTitle)}</h1><p class="lead">${files.length} document${files.length === 1 ? '' : 's'} from ${kinds.size} skill${kinds.size === 1 ? '' : 's'}, newest first. Press <kbd>/</kbd> to search every document.</p>`
      + `<div class="lib-filters" role="toolbar" aria-label="Filter by kind">${filterHtml('fchip')}</div><ul class="lib-list">`;
    for (const f of list) {
      const status = f.meta.status ? chip(f.meta.status, toneOf(f.meta.status) || 'blue') : '';
      html += `<li><a href="${href(f.path)}"><span class="lib-row"><span class="lib-kind">${esc(kindKey(f))}</span><span class="lib-title">${esc(f.title)}</span>${status}<span class="lib-date">${esc(f.date)}</span></span>`
        + `${f.lead ? `<span class="lib-lead">${esc(f.lead)}</span>` : ''}<span class="lib-path">${esc(f.path)}</span></a></li>`;
    }
    html += '</ul>';
    $('#page').innerHTML = html;
    $('#library-summary').hidden = false;
    $('#library-summary').innerHTML = `<span class="kind">Library</span><span class="stats"><span>${list.length === files.length ? `${files.length} documents` : `${list.length} of ${files.length} documents`}</span></span>`;
    $('#toc').innerHTML = `<div class="toc-filters">${filterHtml('l2 tfilter')}</div>`;
    $$('[data-filter]').forEach((b) => b.addEventListener('click', (e) => { e.preventDefault(); libFilter = b.dataset.filter; renderLibrary(); }));
    $('#sources').innerHTML = '<p class="muted">Open a document to list its sources.</p>';
    $('#tab-sources').textContent = 'Sources';
  }

  // ----- Document view -----
  let cites = [];
  function decorate(art, f) {
    $$('.tablewrap', art).forEach(table => {
      table.tabIndex = 0;
      table.setAttribute('role', 'region');
      table.setAttribute('aria-label', 'Scrollable table');
    });
    const refs = f.refs || {};
    const seen = {}, toc = [];
    $$('h1,h2,h3,h4', art).forEach((h) => {
      let id = slugify(h.textContent);
      seen[id] = seen[id] == null ? 0 : seen[id] + 1;
      if (seen[id]) id += `-${seen[id]}`;
      h.id = id;
      if (/H[23]/.test(h.tagName)) toc.push(`<a href="${href(f.path, id)}" data-id="${esc(id)}" class="l${h.tagName[1]}">${esc(h.textContent)}</a>`);
      const a = Object.assign(document.createElement('a'), { className: 'anchor', href: href(f.path, id), textContent: '#' });
      a.setAttribute('aria-label', `Copy link to “${h.textContent}”`);
      h.append(a);
    });
    $('#toc').innerHTML = toc.join('') || '<p class="muted">No sections</p>';
    $('#mobile-toc').innerHTML = toc.length ? '<summary>On this page</summary><nav aria-label="Page sections">' + toc.join('') + '</nav>' : '';
    $('#mobile-toc').hidden = !toc.length;
    const first = $('h1', art)?.nextElementSibling;
    if (first && first.tagName === 'P') first.classList.add('lead');

    $$('table', art).forEach((t) => {
      const w = document.createElement('div');
      w.className = 'tablewrap';
      t.replaceWith(w); w.append(t);
      const heads = $$('thead th', t).map((th) => th.textContent);
      $$('td', t).forEach((td) => {
        if (td.children.length > 1 || (td.children.length === 1 && !/^(STRONG|EM|CODE)$/.test(td.children[0].tagName))) return;
        const tone = toneOf(td.textContent, heads[td.cellIndex] || '');
        if (tone) td.innerHTML = chip(td.textContent.trim(), tone);
      });
    });
    // "**Status:** Draft" or "Verdict: **Confirmed**" lines get a chip for the value.
    $$('p > strong, li > strong', art).forEach((s) => {
      const tone = toneOf(s.textContent, s.previousSibling?.textContent || '');
      if (tone && s.previousSibling && /:\s*$/.test(s.previousSibling.textContent || '')) s.outerHTML = chip(s.textContent, tone);
      else if (/:$/.test(s.textContent) && s.nextSibling?.nodeType === 3) {
        const v = s.nextSibling.textContent.trim(), tone2 = toneOf(v, s.textContent);
        if (tone2) s.nextSibling.replaceWith(Object.assign(document.createElement('span'), { innerHTML: ` ${chip(v, tone2)}` }));
      }
    });

    cites = [];
    const cite = (a, kind, key, label) => { a.id = a.id || `cite-${cites.length}`; cites.push({ id: a.id, kind, key, label: label || a.textContent.trim() || key }); };
    const refKind = (rel) => /\.(md|markdown)$/i.test(rel) ? 'doc' : 'code';
    const codeLink = (a, ref) => {
      const loc = ref.rel + (ref.line ? `:${ref.line}` : '');
      a.href = editorUrl(ref.abs, ref.line);
      a.dataset.abs = ref.abs;
      a.dataset.line = ref.line || '';
      a.classList.add('coderef');
      a.title = `Open ${loc} in your editor`;
      cite(a, refKind(ref.rel), loc, loc);
    };
    $$('a[href]', art).forEach((a) => {
      const raw = a.getAttribute('href');
      if (a.classList.contains('anchor') || raw.startsWith('#/')) return;
      if (raw.startsWith('#')) { const s = decode(raw.slice(1)); a.href = href(f.path, /^fn(ref)?-/.test(s) ? s : slugify(s)); return; }
      if (/^https?:/i.test(raw)) { a.target = '_blank'; a.rel = 'noopener noreferrer'; a.classList.add('ext'); cite(a, 'web', raw); return; }
      if (/^[a-z][\w+.-]*:/i.test(raw)) return;
      const [file, frag] = raw.split('#');
      const target = resolvePath(f.path, decode(file));
      if (byPath.has(target)) { a.href = href(target, frag && slugify(decode(frag))); a.classList.add('doclink'); return; }
      if (refs[raw]) codeLink(a, refs[raw]);
      else { a.classList.add('missing'); a.title = 'Not found in this workspace'; a.setAttribute('aria-disabled', 'true'); a.removeAttribute('href'); }
    });
    $$('code', art).forEach((c) => {
      if (c.closest('pre,a')) return;
      const ref = refs[c.textContent];
      if (!ref) return;
      const a = document.createElement('a');
      c.replaceWith(a); a.append(c);
      codeLink(a, ref);
    });

    let n = 0;
    $$('pre > code', art).forEach((code) => {
      const pre = code.parentElement;
      if (pre.classList.contains('widget-src')) {
        n++;
        try { const d = kit.widget(pre.dataset.widget,code.textContent); if (d.nodes) for (const node of d.nodes) if (node.url) cite(pre,'web',node.url,node.label); } catch { /* Invalid input is explained by the widget renderer. */ }
        return;
      }
      if (pre.classList.contains('mermaid-src')) {
        const src = code.textContent;
        const fig = document.createElement('figure');
        fig.className = 'diagram'; fig.dataset.render = 'pending';
        fig.innerHTML = `<figcaption><b>Diagram ${++n}</b><span>${esc(diagramLabel(src))}</span>`
          + '<button data-act="src" aria-pressed="false">Show source</button><button data-act="copy">Copy source</button><button data-act="download" disabled title="Download vector image">Export SVG</button><button data-act="zoom" disabled>Expand</button></figcaption>'
          + '<div class="canvas" tabindex="0" role="region" aria-label="Scrollable diagram">Rendering diagram…</div><pre hidden><code></code></pre>';
        $('pre code', fig).textContent = src;
        fig.dataset.src = src;
        pre.replaceWith(fig);
        return;
      }
      const bar = document.createElement('div');
      bar.className = 'codebar';
      bar.innerHTML = `<span>${esc(pre.dataset.lang || '')}</span><button type="button">Copy</button>`;
      $('button', bar).addEventListener('click', async (e) => {
        const button = e.currentTarget;
        if (await copy(code.textContent, 'Code copied')) {
          button.textContent = 'Copied';
          setTimeout(() => { button.textContent = 'Copy'; }, 1200);
        }
      });
      pre.prepend(bar);
    });

    renderSources();
    $('#library-summary').replaceChildren();
    $('#library-summary').hidden = true;
  }


  let sourceList = [];
  function renderSources() {
    const groups = new Map();
    for (const c of cites) {
      let g = 'Workspace code';
      if (c.kind === 'doc') g = 'Local docs';
      else if (c.kind === 'web') { try { g = new URL(c.key).hostname.replace(/^www\./, ''); } catch { g = 'Links'; } }
      if (!groups.has(g)) groups.set(g, new Map());
      const m = groups.get(g);
      if (!m.has(c.key)) m.set(c.key, { ...c, ids: [] });
      m.get(c.key).ids.push(c.id);
    }
    sourceList = [...groups.values()].flatMap((m) => [...m.values()]);
    $('#tab-sources').textContent = sourceList.length ? `Sources (${sourceList.length})` : 'Sources';
    if (!sourceList.length) { $('#sources').innerHTML = '<p class="muted">This document cites no links or workspace files.</p>'; return; }
    const rank = (name) => name === 'Local docs' ? 0 : name === 'Workspace code' ? 1 : 2;
    const order = [...groups.keys()].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
    $('#sources').innerHTML = '<button type="button" class="linkbtn" data-act="copy-sources">Copy list as Markdown</button>'
      + order.map((g) => `<div class="src-group"><h3>${esc(g)} <small>${groups.get(g).size}</small></h3>${[...groups.get(g).values()].map((s) => `<a href="#" class="src" data-jump="${esc(s.ids[0])}" title="${esc(s.key)}"><span class="src-label">${esc(s.label)}</span>`
        + `${s.kind === 'web' && s.label !== s.key ? `<span class="src-url">${esc(s.key.replace(/^https?:\/\/(www\.)?/, ''))}</span>` : ''}${s.ids.length > 1 ? `<span class="src-count">cited ${s.ids.length} times</span>` : ''}</a>`).join('')}</div>`).join('');
  }

  return { renderLibrary, decorate, sources: () => sourceList };
}
