// Page view: search across every document, with keyboard navigation of the results.
// Inlined into the page via Function#toString: it may use only ctx and browser globals.
export function searchView(ctx) {
  const { $, $$, esc, slugify, reEsc, plain, files, library, href, show, setPendingTerm } = ctx;
  const q = $('#q'), results = $('#results');
  let hits = [], active = 0;
  function search(term) {
    term = term.trim();
    if (term.length < 2) { results.classList.remove('open'); return; }
    const rx = new RegExp(reEsc(term), 'i');
    hits = [];
    for (const f of files) {
      let heading = '', fence = false;
      for (const line of f.body.split('\n')) {
        if (/^\s*(```|~~~)/.test(line)) fence = !fence;
        const h = !fence && line.match(/^ {0,3}#{1,4}\s+(.+)/);
        if (h) heading = plain(h[1]);
        const at = line.search(rx);
        if (at < 0) continue;
        const from = Math.max(0, at - 48);
        hits.push({ f, heading, snip: (from ? '…' : '') + plain(line.slice(from, at + term.length + 80)), score: h ? 0 : f.title.match(rx) ? 1 : 2 });
      }
    }
    hits.sort((a, b) => a.score - b.score);
    active = 0;
    const mk = (s) => esc(s).replace(new RegExp(reEsc(esc(term)), 'gi'), (m) => `<mark>${m}</mark>`);
    results.innerHTML = hits.length
      ? `<p class="res-count">${hits.length} ${hits.length === 1 ? 'match' : 'matches'}</p>${hits.map((h, i) => `<a class="result${i ? '' : ' active'}" role="option" data-i="${i}"><small>${library ? `${esc(h.f.title)}${h.heading ? ' › ' : ''}` : ''}${esc(h.heading)}</small><span>${mk(h.snip)}</span></a>`).join('')}`
      : `<p class="empty">No match for “${esc(term)}”.</p>`;
    results.classList.add('open');
  }
  function openHit(i) {
    const h = hits[i];
    if (!h) return;
    setPendingTerm(q.value.trim());
    results.classList.remove('open');
    q.blur();
    const next = href(h.f.path, h.heading ? slugify(h.heading) : '');
    if (location.hash === next) { $('#page').dataset.path = '-'; show(); } else location.hash = next;
  }
  q.addEventListener('input', () => search(q.value));
  q.addEventListener('focus', () => q.value && search(q.value));
  q.addEventListener('keydown', (e) => {
    const items = $$('.result', results);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      active = (active + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % Math.max(items.length, 1);
      items.forEach((el, k) => el.classList.toggle('active', k === active));
      items[active]?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter') { e.preventDefault(); openHit(active); }
    else if (e.key === 'Escape') { results.classList.remove('open'); q.blur(); }
  });
  results.addEventListener('mousedown', (e) => { const r = e.target.closest('.result'); if (r) { e.preventDefault(); openHit(Number(r.dataset.i)); } });

  return { q, results };
}
