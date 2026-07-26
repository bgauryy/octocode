import { bytes } from './text.mjs';

const EXTERNAL_BADGES = /github\.com|linkedin\.com|twitter\.com|x\.com|facebook\.com|instagram\.com|youtube\.com|discord\.|slack\.|medium\.com/i;
const ASSET_EXT = /\.(png|jpe?g|gif|svg|webp|ico|css|js|pdf|zip|gz)(?:[?#]|$)/i;
const NOISY_LINK_TEXT = /^(skip to (main )?content|menu|more|open menu|close|toggle|products|resources)$/i;

export function classifyWorkflow(value) {
  const s = String(value || '').toLowerCase();
  const rules = [
    ['pricing', /price|pricing|plans?|billing|checkout|buy|subscribe/],
    ['signup', /sign\s*up|signup|register|start\s+(free|trial)|try\s+it|get\s+started|create\s+account/],
    ['login', /log\s*in|login|sign\s*in|dashboard|account/],
    ['api-reference', /api|endpoint|curl|token|request|response|sdk|reference|extract|markdown|json/],
    ['docs', /docs?|guide|learn|tutorial|handbook|quickstart|getting\s+started/],
    ['examples', /examples?|samples?|demo|github/],
    ['support', /support|help|contact|faq|community/],
    ['changelog', /changelog|release|updates?|version/],
    ['blog', /blog|article|news|web-scraping-101/],
    ['legal', /terms|privacy|legal|cookie/],
    ['download', /download|install/]
  ];
  for (const [type, rx] of rules) if (rx.test(s)) return type;
  return null;
}

export function classifyLink(link, rootUrl) {
  let url;
  try { url = new URL(link.href); } catch { return { ...link, kind: 'invalid', score: 0, reason: 'invalid-url', workflowType: link.workflowHint || classifyWorkflow(link.text) }; }
  const root = new URL(rootUrl);
  const sameHost = url.hostname === root.hostname;
  const sameDomain = url.hostname === root.hostname || url.hostname.endsWith(`.${root.hostname}`);
  const text = (link.text || '').trim();
  const pathDepth = url.pathname.split('/').filter(Boolean).length;
  let kind = sameHost ? 'internal' : sameDomain ? 'subdomain' : 'external';
  if (ASSET_EXT.test(url.pathname)) kind = 'asset';
  if (/mailto:|tel:/i.test(link.href)) kind = 'contact';
  const samePageAnchor = sameHost && url.pathname === root.pathname && Boolean(url.hash);
  const hashOnly = sameHost && url.pathname === root.pathname && (url.hash || url.href.endsWith('#'));
  const noisyLabel = NOISY_LINK_TEXT.test(text);
  const workflowType = link.workflowHint || classifyWorkflow(`${url.pathname} ${text}`);
  let score = 0;
  const reasons = [];
  if (sameHost) { score += 5; reasons.push('same-host'); }
  else if (sameDomain) { score += 3; reasons.push('same-domain'); }
  if (text.length >= 3) { score += 1; reasons.push('labeled'); }
  if (workflowType) { score += 3; reasons.push(`workflow:${workflowType}`); }
  if (pathDepth <= 3) { score += 1; reasons.push('shallow'); }
  if (samePageAnchor) { score -= 4; reasons.push('same-page-anchor'); }
  if (hashOnly) { score -= 3; reasons.push('hash-only'); }
  if (noisyLabel) { score -= 8; reasons.push('boilerplate-label'); }
  if (EXTERNAL_BADGES.test(url.hostname)) { score -= 2; reasons.push('social-or-code-host'); }
  if (kind === 'asset') { score -= 5; reasons.push('asset'); }
  return { ...link, href: url.href, host: url.hostname, path: url.pathname, kind, score, reasons, workflowType, actionability: score >= 4 ? 'candidate' : 'noise' };
}

export function buildSiteGraph({ rootUrl, pageMaps, sources, linksAll, headingsAll, elementRows = [] }) {
  const sourceByPage = new Map(sources.map((s) => [s.pageId, s]));
  const pages = pageMaps.map((page) => {
    const source = sourceByPage.get(page.pageId) || {};
    const headings = headingsAll.filter((h) => h.pageId === page.pageId);
    const links = linksAll.filter((l) => l.pageId === page.pageId).map((l) => classifyLink(l, rootUrl));
    const elements = elementRows.filter((r) => r.pageId === page.pageId);
    const topLinks = links.filter((l) => l.score >= 4).sort((a, b) => b.score - a.score).slice(0, 25);
    return {
      pageId: page.pageId,
      url: page.url,
      status: page.status,
      ok: source.ok,
      title: page.title,
      warning: source.targetLikelyError || null,
      textBytes: source.cleanTextBytes ?? source.textBytes ?? 0,
      headingCount: headings.length,
      linkCount: links.length,
      elementCount: elements.length,
      topLinks,
      headingOutline: headings.slice(0, 40)
    };
  });
  return {
    rootUrl,
    totals: {
      pages: pages.length,
      okPages: pages.filter((p) => p.ok).length,
      warnings: pages.filter((p) => p.warning).length,
      links: linksAll.length,
      elements: elementRows.length,
      bytes: sources.reduce((n, s) => n + (s.cleanTextBytes || 0), 0)
    },
    pages,
    edges: linksAll.map((l) => classifyLink(l, rootUrl)).filter((l) => l.kind === 'internal' || l.kind === 'subdomain').map((l) => ({ from: l.pageId, toUrl: l.href, text: l.text, kind: l.kind, score: l.score, workflowType: l.workflowType || null }))
  };
}

export function buildWorkflowIndex({ rootUrl, pageMaps, linksAll, headingsAll, elementRows = [] }) {
  const candidates = [];
  for (const link of linksAll.map((l) => classifyLink(l, rootUrl))) {
    if (!link.workflowType || link.score < 4) continue;
    candidates.push({ workflowType: link.workflowType, confidence: link.score >= 8 ? 'high' : 'medium', entryPageId: link.pageId, entryUrl: link.href, label: link.text, source: 'link', score: link.score, evidence: [{ file: 'indexes/top-links.jsonl', reason: link.reasons.join(', ') }] });
  }
  for (const h of headingsAll) {
    // h.workflowHint was already computed from h.text at extraction time (extractHeadings) —
    // no fallback re-classification here; a heading with no clean signal has no signal.
    const workflowType = h.workflowHint;
    if (workflowType) candidates.push({ workflowType, confidence: h.level <= 2 ? 'medium' : 'low', entryPageId: h.pageId, entryUrl: pageMaps.find((p) => p.pageId === h.pageId)?.url || null, label: h.text, source: 'heading', score: 4, evidence: [{ file: 'extracts/headings.jsonl', reason: `heading level ${h.level}` }] });
  }
  for (const e of elementRows) {
    // Never fall back to classifyWorkflow(JSON.stringify(e)) — that stringifies the whole row,
    // including bookkeeping fields like pageId ("page-001"), which any sufficiently generic rule
    // (e.g. a hypothetical "page N" pattern) can false-positive-match. Every extractor already
    // computes workflowHint from clean, scoped fields (text/action/labels/headers, never the row
    // itself); the one row type that doesn't (extractCanonicalFromHtml) correctly gets no
    // workflowType here — a canonical link isn't a workflow signal anyway.
    //
    // meta/jsonld rows have no href/action of their own — their entryUrl always falls back to
    // the current page, so a match here (e.g. og:url containing "api", description mentioning
    // "docs") never points anywhere distinct; verified live on docs.scrapingant.com, where this
    // produced 15 duplicate same-URL candidates. Forms/buttons/tables stay: they represent real
    // on-page content/actions even without a distinct href.
    if (e._file === 'meta' || e._file === 'jsonld') continue;
    const workflowType = e.workflowHint;
    if (workflowType) candidates.push({ workflowType, confidence: ['form', 'button', 'cta-link', 'table'].includes(e.kind) ? 'high' : 'medium', entryPageId: e.pageId, entryUrl: e.href || e.action || pageMaps.find((p) => p.pageId === e.pageId)?.url || null, label: e.text || e.key || e.kind || workflowType, source: e.kind || 'element', score: 8, evidence: [{ file: `extracts/${e._file || 'elements'}.jsonl`, reason: `${e.kind || 'element'} workflow hint` }] });
  }
  const seen = new Set();
  const deduped = [];
  for (const c of candidates.sort((a, b) => b.score - a.score)) {
    const key = `${c.workflowType}:${c.entryUrl}:${c.label}`;
    if (seen.has(key)) continue;
    seen.add(key);
    deduped.push(c);
  }
  return { rootUrl, totals: { candidates: deduped.length }, workflows: deduped };
}

export function buildUnifiedGraph({ rootUrl, pageMaps, siteGraph, workflowIndex, elementRows = [], resourcesAll = [] }) {
  const pageIdByUrl = new Map(pageMaps.map((p) => [p.url, p.pageId]));
  const pageUrlById = new Map(pageMaps.map((p) => [p.pageId, p.url]));
  const workflowTypesByPageId = new Map();
  for (const w of workflowIndex.workflows || []) {
    if (!w.entryPageId) continue;
    const set = workflowTypesByPageId.get(w.entryPageId) || new Set();
    set.add(w.workflowType);
    workflowTypesByPageId.set(w.entryPageId, set);
  }
  const source = (file, row) => ({ file, pageId: row.pageId || null, url: pageUrlById.get(row.pageId) || row.href || row.src || null });
  const nodes = pageMaps.map((p) => ({ id: p.pageId, kind: 'page', url: p.url, pageId: p.pageId, title: p.title || null, text: null, selector: null, status: p.status ?? null, workflowTypes: [...(workflowTypesByPageId.get(p.pageId) || [])], risk: null, confidence: 1, source: { file: `pages/${p.pageId}.json`, pageId: p.pageId, url: p.url } }));
  const nodeIds = new Set(nodes.map((n) => n.id));
  const edges = [];
  const addNode = (node) => { if (!nodeIds.has(node.id)) { nodes.push(node); nodeIds.add(node.id); } };
  const addEdge = (edge) => edges.push(edge);

  for (const edge of siteGraph.edges || []) {
    if ((edge.score || 0) < 4) continue;
    const targetPageId = pageIdByUrl.get(edge.toUrl);
    const toId = targetPageId || `link:${edge.toUrl}`;
    if (!targetPageId) addNode({ id: toId, kind: edge.workflowType === 'pagination' ? 'pagination' : 'link', url: edge.toUrl, pageId: null, title: null, text: edge.text || null, selector: null, status: null, workflowTypes: edge.workflowType ? [edge.workflowType] : [], risk: null, confidence: Math.min(1, (edge.score || 0) / 10), source: { file: 'graph/site-graph.json', pageId: edge.from, url: edge.toUrl } });
    addEdge({ from: edge.from, to: toId, kind: edge.workflowType === 'pagination' ? 'paginates_to' : 'navigates_to', label: edge.text || null, workflowType: edge.workflowType || null, score: edge.score || 0, risk: null, confidence: Math.min(1, (edge.score || 0) / 10), source: { file: 'graph/site-graph.json', pageId: edge.from, url: edge.toUrl } });
  }

  for (const row of elementRows) {
    if (!['forms', 'buttons', 'tables'].includes(row._file)) continue;
    const kind = row._file === 'forms' ? 'form' : row._file === 'buttons' ? 'button' : 'table';
    const id = `${kind}:${row.pageId}:${nodes.length}`;
    const url = row.href || row.action || pageUrlById.get(row.pageId) || rootUrl;
    const risk = kind === 'form' ? 'user-data-required' : null;
    addNode({ id, kind, url, pageId: row.pageId, title: null, text: row.text || row.labels?.join(' ') || row.preview || null, selector: row.selector || null, status: null, workflowTypes: row.workflowHint ? [row.workflowHint] : [], risk, confidence: row.workflowHint ? 0.8 : 0.55, source: source(`extracts/${row._file}.jsonl`, row) });
    addEdge({ from: row.pageId, to: id, kind: kind === 'form' ? 'submits_to' : kind === 'button' ? 'clicks_to' : 'reveals', label: row.text || row.labels?.join(' ') || kind, workflowType: row.workflowHint || null, score: row.workflowHint ? 8 : 4, risk, confidence: row.workflowHint ? 0.8 : 0.55, source: source(`extracts/${row._file}.jsonl`, row) });
    if (kind === 'form') {
      for (const input of row.inputs || []) {
        const inputId = `input:${row.pageId}:${input.name || input.type || nodes.length}`;
        addNode({ id: inputId, kind: 'input', url, pageId: row.pageId, title: null, text: input.name || input.placeholder || input.type || null, selector: input.selector || null, status: null, workflowTypes: row.workflowHint ? [row.workflowHint] : [], risk: 'user-data-required', confidence: 0.75, source: source(`extracts/${row._file}.jsonl`, row) });
        addEdge({ from: id, to: inputId, kind: 'reveals', label: input.name || input.type || 'input', workflowType: row.workflowHint || null, score: 5, risk: 'user-data-required', confidence: 0.75, source: source(`extracts/${row._file}.jsonl`, row) });
      }
    }
  }

  for (const row of resourcesAll) {
    const resourceKind = /\.pdf(?:[?#]|$)/i.test(row.src || '') ? 'download' : row.kind === 'script' && /api|graphql|sdk/i.test(row.src || '') ? 'api' : 'resource';
    const id = `${resourceKind}:${row.pageId}:${row.src}`;
    addNode({ id, kind: resourceKind, url: row.src, pageId: row.pageId, title: null, text: row.kind || null, selector: null, status: null, workflowTypes: [], risk: null, confidence: 0.7, source: source('extracts/resources.jsonl', row) });
    addEdge({ from: row.pageId, to: id, kind: resourceKind === 'download' ? 'downloads' : resourceKind === 'api' ? 'calls_api' : 'loads_resource', label: row.kind || null, workflowType: null, score: 4, risk: null, confidence: 0.7, source: source('extracts/resources.jsonl', row) });
  }

  return { schemaVersion: 2, rootUrl, totals: { nodes: nodes.length, edges: edges.length, pages: pageMaps.length, actions: nodes.filter((n) => ['form', 'button', 'table', 'pagination'].includes(n.kind)).length, resources: nodes.filter((n) => ['resource', 'api', 'download'].includes(n.kind)).length }, nodes, edges };
}

export function pageSlice({ pageMaps, sources, linksAll, headingsAll, page = 1, pageSize = 20 }) {
  const sourceByPage = new Map(sources.map((s) => [s.pageId, s]));
  const totalPages = Math.max(1, Math.ceil(pageMaps.length / pageSize));
  const safePage = Math.min(Math.max(1, page), totalPages);
  const start = (safePage - 1) * pageSize;
  const rows = pageMaps.slice(start, start + pageSize).map((p) => {
    const source = sourceByPage.get(p.pageId) || {};
    return {
      pageId: p.pageId,
      url: p.url,
      status: p.status,
      ok: source.ok,
      title: p.title,
      warning: source.targetLikelyError || null,
      textBytes: source.cleanTextBytes ?? 0,
      headingCount: headingsAll.filter((h) => h.pageId === p.pageId).length,
      linkCount: linksAll.filter((l) => l.pageId === p.pageId).length,
      searchFirst: p.searchFirst
    };
  });
  return { page: safePage, pageSize, totalPages, totalItems: pageMaps.length, prev: safePage > 1 ? `indexes/pages-${String(safePage - 1).padStart(3, '0')}.json` : null, next: safePage < totalPages ? `indexes/pages-${String(safePage + 1).padStart(3, '0')}.json` : null, rows };
}

export function pageSlices(args) {
  const totalPages = Math.max(1, Math.ceil(args.pageMaps.length / (args.pageSize || 20)));
  return Array.from({ length: totalPages }, (_, i) => pageSlice({ ...args, page: i + 1 }));
}

export function compactStats(text) {
  const lines = (text || '').split(/\r?\n/).length;
  return { bytes: bytes(text), lines };
}
