import { writeFileSync, readFileSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const helper = (name) => import(pathToFileURL(resolve(process.cwd(), '.octocode', name)).href);
const { waitForPageReady } = await helper('dom-actionability.mjs');
const { collectRefs, clip, flat } = await helper('ax-snapshot.mjs');

// Compact, ref-based page view from the accessibility tree: controls, headings, and
// non-semantic clickables in document order (same-process iframes inlined), each with a
// ref backed by a backendDOMNodeId that dom-operations-check.mjs accepts as DOM_REF.
// Every ref of the run is saved to page-snapshot.json; stdout shows one page of them.
//
// Env:
//   SNAPSHOT_MAX       refs per printed page (default 60, max 300)
//   SNAPSHOT_PAGE      page to print (default 1); refs keep global numbers e61…
//   SNAPSHOT_ROOT      only this region: CSS selector, eN ref, or rN region from the outline
//   SNAPSHOT_VIEWPORT  1 keeps only elements intersecting the current viewport
//   SNAPSHOT_OUTLINE   1 prints landmarks/headings with ref spans instead of refs
//   SNAPSHOT_CONTEXT   0 drops row/card context lines and inferred names (default on)
//   SNAPSHOT_URLS      1 appends link targets (→ /path)
//   SNAPSHOT_CLICKABLE 0 skips cursor:pointer/onclick detection (one DOMSnapshot call)
//   SNAPSHOT_TEXT      print the first N chars (max 4000) of main/body text; full text -> page-text.txt
//   SNAPSHOT_STDOUT    summary keeps refs on disk
//   SNAPSHOT_DEPTH     max AX tree depth (default unlimited)

const int = (name, fallback) => Number.parseInt(process.env[name] ?? String(fallback), 10);
const DEPTH = int('SNAPSHOT_DEPTH', -1);
const MAX_REFS = Math.max(1, Math.min(300, int('SNAPSHOT_MAX', 60) || 60));
const PAGE = Math.max(1, int('SNAPSHOT_PAGE', 1) || 1);
const ROOT = (process.env.SNAPSHOT_ROOT ?? '').trim();
const VIEWPORT = process.env.SNAPSHOT_VIEWPORT === '1';
const OUTLINE = process.env.SNAPSHOT_OUTLINE === '1';
const CONTEXT = process.env.SNAPSHOT_CONTEXT !== '0';
const URLS = process.env.SNAPSHOT_URLS === '1';
const CLICKABLE = process.env.SNAPSHOT_CLICKABLE !== '0';
const SUMMARY = process.env.SNAPSHOT_STDOUT === 'summary';
const MAX_CONTEXT = 110;
const TEXT_CHARS = Math.max(0, Math.min(4000, int('SNAPSHOT_TEXT', 0) || 0));

function previousSnapshot(cdp) {
  try {
    const map = JSON.parse(readFileSync(cdp.resourcesFile, 'utf8'));
    return JSON.parse(readFileSync(map.resources['page-snapshot'].artifactPath, 'utf8'));
  } catch {
    return null;
  }
}

async function rootBackendId(cdp) {
  if (/^[er]\d+$/.test(ROOT)) {
    const prev = previousSnapshot(cdp);
    const entry = ROOT.startsWith('e') ? prev?.refs?.[ROOT] : prev?.regions?.[ROOT];
    if (!entry) throw new Error(`SNAPSHOT_ROOT=${ROOT} not in the last snapshot; run page-snapshot (SNAPSHOT_OUTLINE=1 for regions) first`);
    return entry.backendDOMNodeId;
  }
  const { root } = await cdp.send('DOM.getDocument', { depth: 0 });
  const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: root.nodeId, selector: ROOT });
  if (!nodeId) throw new Error(`SNAPSHOT_ROOT selector ${JSON.stringify(ROOT)} matched nothing`);
  return (await cdp.send('DOM.describeNode', { nodeId })).node.backendNodeId;
}

// One page-side pass over the printed refs: inferred names for unnamed controls, link
// targets, and the row/card text around each ref (list items, table rows, articles, cards).
const ENRICH_FN = `function(...els) {
  const ROW = 'tr, li, article, [role=row], [role=listitem], [role=article], [class*=card i], [class*=item i]';
  const groups = new Map();
  const text = (el) => (el?.innerText || '').replace(/\\s+/g, ' ').trim();
  const isItemStart = (el) => /^\\s*\\d+[.)]/.test(text(el));
  const pathOf = (href) => { try { const u = new URL(href, location.href); return (u.origin === location.origin ? '' : u.host) + u.pathname + u.search; } catch { return ''; } };
  function hint(el) {
    const own = text(el);
    if (own) return own.slice(0, 80);
    const t = el.getAttribute('title') || el.querySelector('[title]')?.getAttribute('title');
    if (t) return t;
    const desc = el.getAttribute('aria-describedby');
    if (desc) { const d = text(document.getElementById(desc)); if (d) return d; }
    const img = el.querySelector('img, svg');
    if (img) {
      const alt = img.getAttribute('alt') || img.querySelector?.('title')?.textContent;
      if (alt) return alt;
      const src = img.getAttribute('src');
      if (src) return 'img ' + src.split(/[?#]/)[0].split('/').pop();
    }
    const cls = [el, ...el.querySelectorAll('*')].map((n) => (typeof n.className === 'string' ? n.className : '')).join(' ').split(/\\s+/).find((c) => c.length > 2);
    const href = el.getAttribute('href');
    return [cls, href && '\\u2192 ' + pathOf(href).slice(0, 50)].filter(Boolean).join(' ');
  }
  return els.map((el) => {
    if (!el || el.nodeType !== 1) return null;
    const row = el.closest(ROW);
    let key = null;
    let context = '';
    if (row && row !== document.body) {
      // Item row + meta row tables (Hacker News style): fold the following row in.
      const owner = row.localName === 'tr' && !isItemStart(row) && row.previousElementSibling && isItemStart(row.previousElementSibling) ? row.previousElementSibling : row;
      if (!groups.has(owner)) {
        let t = text(owner);
        const next = owner.nextElementSibling;
        if (owner.localName === 'tr' && isItemStart(owner) && next && !isItemStart(next)) t += ' \\u00b7 ' + text(next);
        groups.set(owner, { id: groups.size + 1, t });
      }
      const g = groups.get(owner);
      key = g.id; context = g.t;
    }
    const href = el.closest('a[href]')?.getAttribute('href');
    return { key, context, hint: hint(el), href: href ? pathOf(href).slice(0, 80) : '' };
  });
}`;

async function enrich(cdp, nodes) {
  if (!nodes.length) return [];
  const objectIds = [];
  for (const n of nodes) {
    if (n.frame) { objectIds.push(null); continue; }
    const r = await cdp.send('DOM.resolveNode', { backendNodeId: n.backendDOMNodeId, objectGroup: 'snapshot' }).catch(() => null);
    objectIds.push(r?.object?.objectId ?? null);
  }
  const anchor = objectIds.find(Boolean);
  if (!anchor) return [];
  try {
    // Iframe elements live in another execution context, so they are passed as null and keep AX names only.
    const res = await cdp.send('Runtime.callFunctionOn', {
      objectId: anchor,
      functionDeclaration: ENRICH_FN,
      arguments: objectIds.map((objectId) => (objectId ? { objectId } : { value: null })),
      returnByValue: true,
    });
    return res.result?.value ?? [];
  } catch {
    return [];
  } finally {
    await cdp.send('Runtime.releaseObjectGroup', { objectGroup: 'snapshot' }).catch(() => {});
  }
}

export async function run(cdp) {
  const ready = await waitForPageReady(cdp, Number(process.env.SNAPSHOT_WAIT_MS || 8000), { selector: process.env.SNAPSHOT_WAIT_SELECTOR || '', text: process.env.SNAPSHOT_WAIT_TEXT || '' });
  if (!ready && (process.env.SNAPSHOT_WAIT_SELECTOR || process.env.SNAPSHOT_WAIT_TEXT)) process.exitCode = 1;
  if (!ready) console.log('[FINDING] PAGE_NOT_FULLY_LOADED document or requested content did not become ready within timeout — snapshot may be incomplete');

  const viewport = VIEWPORT ? (await cdp.send('Page.getLayoutMetrics')).cssVisualViewport : null;
  const { useful, regions, totalNodes, findings } = await collectRefs(cdp, {
    rootBackendId: ROOT ? await rootBackendId(cdp) : null, viewport, clickable: CLICKABLE, depth: DEPTH,
  });
  for (const f of findings) console.log(`[FINDING] ${f}`);

  const refs = {};
  useful.forEach((n, i) => { refs[`e${i + 1}`] = { backendDOMNodeId: n.backendDOMNodeId, role: n.role, name: n.fullName, frame: n.frame }; });
  const regionRefs = {};
  regions.forEach((r, i) => { regionRefs[`r${i + 1}`] = { backendDOMNodeId: r.backendDOMNodeId, role: r.role, name: r.name }; });

  const pages = Math.max(1, Math.ceil(useful.length / MAX_REFS));
  const first = (PAGE - 1) * MAX_REFS;
  const shown = useful.slice(first, first + MAX_REFS);

  const artifactPath = join(cdp.outputDir, 'page-snapshot.json');
  const snapshot = { targetId: cdp.targetInfo.id, url: cdp.targetInfo.url, root: ROOT || null, viewport: VIEWPORT, refs, regions: regionRefs, coverage: { totalAxNodes: totalNodes, findings } };
  const continuation = (view) => JSON.stringify({ command: process.execPath, args: [join(dirname(fileURLToPath(import.meta.url)), 'snapshot-query.mjs'), '--file', artifactPath, '--page', String(PAGE + 1), '--limit', String(MAX_REFS), '--view', view] });
  cdp.upsertResourceMap?.('page-snapshot', {
    type: 'page-snapshot',
    targetUrl: cdp.targetInfo.url,
    refCount: useful.length,
    totalAxNodes: totalNodes,
    artifactPath,
  });

  const title = (await cdp.send('Runtime.evaluate', { expression: 'document.title', returnByValue: true }).catch(() => null))?.result?.value ?? '';
  snapshot.title = String(title);
  console.log(`[PAGE] "${clip(String(title), 120)}" ${cdp.targetInfo.url}${ROOT ? ` root=${ROOT}` : ''}${VIEWPORT ? ' viewport' : ''}`);
  const span = shown.length && !OUTLINE ? ` showing=e${first + 1}-e${first + shown.length}` : '';
  console.log(`[METRIC] SNAPSHOT refs=${useful.length}${span}${OUTLINE ? '' : ` page=${PAGE}/${pages}`} totalAxNodes=${totalNodes}`);

  if (OUTLINE) {
    const items = [];
    const regionNames = new Set(regions.map((r) => r.name).filter(Boolean));
    regions.forEach((r, i) => items.push({ at: r.start, order: i, line: `r${i + 1} ${r.role}${r.name ? ` "${r.name}"` : ''} — ${r.end - r.start} refs${r.end > r.start ? ` e${r.start + 1}-e${r.end}` : ''}` }));
    useful.forEach((n, i) => {
      if (n.role === 'heading' && (n.level ?? 9) <= 3 && !regionNames.has(n.name)) items.push({ at: i, order: Infinity, line: `  h${n.level ?? ''} "${n.name}" e${i + 1}` });
    });
    items.sort((a, b) => a.at - b.at || a.order - b.order);
    snapshot.outline = items;
    for (const it of items.slice(first, first + MAX_REFS)) console.log(`[OUTLINE] ${it.line}`);
    if (first + MAX_REFS < items.length) console.log(`[NEXT] ${continuation('outline')}`);
    console.log('[REASON] Narrow with SNAPSHOT_ROOT=rN (or a CSS selector), or page with SNAPSHOT_PAGE.');
  } else if (!SUMMARY) {
    const extra = CONTEXT || URLS ? await enrich(cdp, shown) : [];
    const namesByGroup = new Map();
    shown.forEach((n, i) => { const k = extra[i]?.key; if (k) namesByGroup.set(k, [...(namesByGroup.get(k) ?? []), n.fullName || flat(extra[i]?.hint)]); });
    let lastKey = null;
    let lastFrame;
    shown.forEach((n, i) => {
      const e = extra[i];
      if (n.frame !== lastFrame && n.frame) console.log(`[SNAPSHOT] — iframe ${clip(n.frame, 90)}`);
      lastFrame = n.frame;
      if (CONTEXT && e?.key && e.key !== lastKey) {
        const ctx = flat(e.context);
        // Print context only when it says more than the group's own control names.
        let residual = ctx;
        for (const name of namesByGroup.get(e.key) ?? []) if (name) residual = residual.split(name).join(' ');
        if (residual.replace(/[^\p{L}\p{N}]/gu, '').length >= 8) console.log(`[SNAPSHOT] — ${clip(ctx, MAX_CONTEXT)}`);
      }
      lastKey = e?.key ?? null;
      const role = n.role === 'heading' ? (n.level ? `h${n.level}` : 'heading') : n.role;
      const label = n.name ? ` "${n.name}"` : CONTEXT && e?.hint ? ` ~"${clip(flat(e.hint), 60)}"` : '';
      const url = URLS && e?.href && n.role === 'link' ? ` → ${e.href}` : '';
      console.log(`[SNAPSHOT] [e${first + i + 1}] ${role}${label}${url}`);
    });
    if (PAGE < pages) console.log(`[NEXT] ${continuation('refs')}`);
  }
  writeFileSync(artifactPath, `${JSON.stringify(snapshot, null, 2)}\n`, { mode: 0o600 });
  console.log(`[ARTIFACT] PAGE_SNAPSHOT ${artifactPath}`);

  if (TEXT_CHARS > 0) {
    const text = (await cdp.send('Runtime.evaluate', {
      returnByValue: true,
      expression: `((document.querySelector('main,[role=main],article') || document.body)?.innerText || '').replace(/\\s+/g, ' ').trim()`,
    }).catch(() => null))?.result?.value ?? '';
    const textPath = join(cdp.outputDir, 'page-text.txt');
    writeFileSync(textPath, `${text}\n`, { mode: 0o600 });
    console.log(`[TEXT] ${text.slice(0, TEXT_CHARS)}${text.length > TEXT_CHARS ? `… (+${text.length - TEXT_CHARS} chars)` : ''}`);
    console.log(`[ARTIFACT] PAGE_TEXT ${textPath}`);
  }
}
