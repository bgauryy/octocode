
const EXTERNAL_BADGES = /github\.com|linkedin\.com|twitter\.com|x\.com|facebook\.com|instagram\.com|youtube\.com|discord\.|slack\.|medium\.com/i;
const ASSET_EXT = /\.(png|jpe?g|gif|svg|webp|ico|css|js|pdf|zip|gz)(?:[?#]|$)/i;
const NOISY_LINK_TEXT = /^(skip to (main )?content|menu|more|open menu|close|toggle|products|resources)$/i;

export function classifyWorkflow(value) {
  const s = String(value || '').toLowerCase();
  // Whole words only: "explanation" is not a plan, "capital" is not an api, "dockerfile" is not docs.
  const rules = [
    ['pricing', /\b(?:prices?|pricing|plans?|billing|checkout|buy|subscribe)\b/],
    ['signup', /\b(?:sign[\s_-]*up|register|start[\s_-]+(?:free|trial)|try[\s_-]+it|get[\s_-]+started|create[\s_-]+account)\b/],
    ['login', /\b(?:log[\s_-]*in|sign[\s_-]*in|dashboard|accounts?)\b/],
    ['api-reference', /\b(?:apis?|endpoints?|curl|tokens?|requests?|responses?|sdks?|reference|extract|markdown|json)\b/],
    ['docs', /\b(?:docs?|documentation|guides?|learn|tutorials?|handbook|quickstart|getting[\s_-]+started)\b/],
    ['examples', /\b(?:examples?|samples?|demos?|github)\b/],
    ['support', /\b(?:support|help|contact|faq|community)\b/],
    ['changelog', /\b(?:changelog|releases?|updates?|versions?)\b/],
    ['blog', /\b(?:blog|articles?|news|web-scraping-101)\b/],
    ['legal', /\b(?:terms|privacy|legal|cookies?)\b/],
    ['download', /\b(?:downloads?|install|installation)\b/]
  ];
  for (const [type, rx] of rules) if (rx.test(s)) return type;
  return null;
}

// A partial session may lack a root URL; then no link counts as same-host.
const parseRoot = (rootUrl) => { try { return new URL(rootUrl); } catch { return null; } };

function linkKind(url, href, sameHost, sameDomain) {
  let kind = sameHost ? 'internal' : sameDomain ? 'subdomain' : 'external';
  if (ASSET_EXT.test(url.pathname)) kind = 'asset';
  if (/mailto:|tel:/i.test(href)) kind = 'contact';
  return kind;
}

// Links back to the root page itself: an in-page anchor, or a bare hash.
function anchorFacts(url, root, sameHost) {
  const samePage = sameHost && url.pathname === root?.pathname;
  return { samePageAnchor: samePage && Boolean(url.hash), hashOnly: samePage && (url.hash || url.href.endsWith('#')) };
}

// Score signals in order: [applies, points, reason].
function linkScore(f) {
  const signals = [
    [f.sameHost, 5, 'same-host'],
    [!f.sameHost && f.sameDomain, 3, 'same-domain'],
    [f.text.length >= 3, 1, 'labeled'],
    [f.workflowType, 3, `workflow:${f.workflowType}`],
    [f.pathDepth <= 3, 1, 'shallow'],
    [f.samePageAnchor, -4, 'same-page-anchor'],
    [f.hashOnly, -3, 'hash-only'],
    [f.noisyLabel, -8, 'boilerplate-label'],
    [EXTERNAL_BADGES.test(f.host), -2, 'social-or-code-host'],
    [f.kind === 'asset', -5, 'asset'],
  ];
  let score = 0;
  const reasons = [];
  for (const [applies, points, reason] of signals) if (applies) { score += points; reasons.push(reason); }
  return { score, reasons };
}

export function classifyLink(link, rootUrl) {
  let url;
  try { url = new URL(link.href); } catch { return { ...link, kind: 'invalid', score: 0, reason: 'invalid-url', workflowType: link.workflowHint || classifyWorkflow(link.text) }; }
  const root = parseRoot(rootUrl);
  const sameHost = Boolean(root) && url.hostname === root.hostname;
  const sameDomain = Boolean(root) && (sameHost || url.hostname.endsWith(`.${root.hostname}`));
  const text = (link.text || '').trim();
  const kind = linkKind(url, link.href, sameHost, sameDomain);
  const workflowType = link.workflowHint || classifyWorkflow(`${url.pathname} ${text}`);
  const { score, reasons } = linkScore({
    sameHost, sameDomain, text, workflowType, kind, host: url.hostname,
    pathDepth: url.pathname.split('/').filter(Boolean).length,
    noisyLabel: NOISY_LINK_TEXT.test(text),
    ...anchorFacts(url, root, sameHost),
  });
  return { ...link, href: url.href, host: url.hostname, path: url.pathname, kind, score, reasons, workflowType, actionability: score >= 4 ? 'candidate' : 'noise' };
}

// Group rows by pageId once; filtering every array per page grows with pages × rows.
export function byPage(rows = []) {
  const groups = new Map();
  for (const row of rows) {
    if (!groups.has(row.pageId)) groups.set(row.pageId, []);
    groups.get(row.pageId).push(row);
  }
  return groups;
}

export function buildSiteGraph({ rootUrl, pageMaps, sources, linksAll, headingsAll, elementRows = [] }) {
  const sourceByPage = new Map(sources.map((s) => [s.pageId, s]));
  const headingsByPage = byPage(headingsAll), linksByPage = byPage(linksAll), elementsByPage = byPage(elementRows);
  const pages = pageMaps.map((page) => {
    const source = sourceByPage.get(page.pageId) || {};
    const headings = headingsByPage.get(page.pageId) || [];
    const links = (linksByPage.get(page.pageId) || []).map((l) => classifyLink(l, rootUrl));
    const elements = elementsByPage.get(page.pageId) || [];
    const topLinks = links.filter((l) => l.score >= 4).toSorted((a, b) => b.score - a.score);
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
      headingOutline: headings
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

const pageUrlFor = (pageMaps, pageId) => pageMaps.find((p) => p.pageId === pageId)?.url || null;

function linkCandidates(linksAll, rootUrl) {
  return linksAll.map((l) => classifyLink(l, rootUrl)).filter((link) => link.workflowType && link.score >= 4)
    .map((link) => ({ workflowType: link.workflowType, confidence: link.score >= 8 ? 'high' : 'medium', entryPageId: link.pageId, entryUrl: link.href, label: link.text, source: 'link', score: link.score, evidence: [{ file: 'indexes/top-links.jsonl', reason: link.reasons.join(', ') }] }));
}

// h.workflowHint was already computed from h.text at extraction time (extractHeadings) —
// no fallback re-classification here; a heading with no clean signal has no signal.
function headingCandidates(headingsAll, pageMaps) {
  return headingsAll.filter((h) => h.workflowHint)
    .map((h) => ({ workflowType: h.workflowHint, confidence: h.level <= 2 ? 'medium' : 'low', entryPageId: h.pageId, entryUrl: pageUrlFor(pageMaps, h.pageId), label: h.text, source: 'heading', score: 4, evidence: [{ file: 'extracts/headings.jsonl', reason: `heading level ${h.level}` }] }));
}

function elementCandidate(e, pageMaps) {
  const workflowType = e.workflowHint;
  return { workflowType, confidence: ['form', 'button', 'cta-link', 'table'].includes(e.kind) ? 'high' : 'medium', entryPageId: e.pageId, entryUrl: e.href || e.action || pageUrlFor(pageMaps, e.pageId), label: e.text || e.key || e.kind || workflowType, source: e.kind || 'element', score: 8, evidence: [{ file: `extracts/${e._file || 'elements'}.jsonl`, reason: `${e.kind || 'element'} workflow hint` }] };
}

function elementCandidates(elementRows, pageMaps) {
  const candidates = [];
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
    if (e.workflowHint) candidates.push(elementCandidate(e, pageMaps));
  }
  return candidates;
}

export function buildWorkflowIndex({ rootUrl, pageMaps, linksAll, headingsAll, elementRows = [] }) {
  const candidates = [...linkCandidates(linksAll, rootUrl), ...headingCandidates(headingsAll, pageMaps), ...elementCandidates(elementRows, pageMaps)];
  const workflows = candidates.toSorted((a, b) => b.score - a.score);
  return { rootUrl, totals: { candidates: workflows.length }, workflows };
}

function workflowTypesByPage(workflowIndex) {
  const byPageId = new Map();
  for (const w of workflowIndex.workflows || []) {
    if (!w.entryPageId) continue;
    const set = byPageId.get(w.entryPageId) || new Set();
    set.add(w.workflowType);
    byPageId.set(w.entryPageId, set);
  }
  return byPageId;
}

// The graph under construction: nodes keep insertion order and unique ids.
function graphBuilder(nodes) {
  const nodeIds = new Set(nodes.map((n) => n.id));
  const edges = [];
  return {
    nodes,
    edges,
    addNode(node) { if (!nodeIds.has(node.id)) { nodes.push(node); nodeIds.add(node.id); } },
    addEdge(edge) { edges.push(edge); },
  };
}

const SITE_GRAPH = 'graph/site-graph.json';

// Strong site-graph links: page-to-page edges, plus a node for each unvisited target.
function addLinkEdges(g, siteGraph, pageIdByUrl) {
  for (const edge of siteGraph.edges || []) {
    if ((edge.score || 0) < 4) continue;
    const targetPageId = pageIdByUrl.get(edge.toUrl);
    const toId = targetPageId || `link:${edge.toUrl}`;
    const paginates = edge.workflowType === 'pagination';
    const confidence = Math.min(1, (edge.score || 0) / 10);
    const source = { file: SITE_GRAPH, pageId: edge.from, url: edge.toUrl };
    if (!targetPageId) g.addNode({ id: toId, kind: paginates ? 'pagination' : 'link', url: edge.toUrl, pageId: null, title: null, text: edge.text || null, selector: null, status: null, workflowTypes: edge.workflowType ? [edge.workflowType] : [], risk: null, confidence, source });
    g.addEdge({ from: edge.from, to: toId, kind: paginates ? 'paginates_to' : 'navigates_to', label: edge.text || null, workflowType: edge.workflowType || null, score: edge.score || 0, risk: null, confidence, source: { ...source } });
  }
}

const ELEMENT_KIND = { forms: 'form', buttons: 'button', tables: 'table' };
const ELEMENT_EDGE = { form: 'submits_to', button: 'clicks_to', table: 'reveals' };

// A form's inputs: each is a node the form reveals, and each needs user data.
function addFormInputs(g, row, form) {
  for (const input of row.inputs || []) {
    const inputId = `input:${row.pageId}:${input.name || input.type || g.nodes.length}`;
    g.addNode({ id: inputId, kind: 'input', url: form.url, pageId: row.pageId, title: null, text: input.name || input.placeholder || input.type || null, selector: input.selector || null, status: null, workflowTypes: row.workflowHint ? [row.workflowHint] : [], risk: 'user-data-required', confidence: 0.75, source: form.source() });
    g.addEdge({ from: form.id, to: inputId, kind: 'reveals', label: input.name || input.type || 'input', workflowType: row.workflowHint || null, score: 5, risk: 'user-data-required', confidence: 0.75, source: form.source() });
  }
}

const elementText = (row) => row.text || row.labels?.join(' ');

function addElementNode(g, row, { rowSource, pageUrlById, rootUrl }) {
  const kind = ELEMENT_KIND[row._file];
  const id = `${kind}:${row.pageId}:${g.nodes.length}`;
  const url = row.href || row.action || pageUrlById.get(row.pageId) || rootUrl;
  const risk = kind === 'form' ? 'user-data-required' : null;
  const hinted = Boolean(row.workflowHint);
  const source = () => rowSource(`extracts/${row._file}.jsonl`, row);
  g.addNode({ id, kind, url, pageId: row.pageId, title: null, text: elementText(row) || row.preview || null, selector: row.selector || null, status: null, workflowTypes: hinted ? [row.workflowHint] : [], risk, confidence: hinted ? 0.8 : 0.55, source: source() });
  g.addEdge({ from: row.pageId, to: id, kind: ELEMENT_EDGE[kind], label: elementText(row) || kind, workflowType: row.workflowHint || null, score: hinted ? 8 : 4, risk, confidence: hinted ? 0.8 : 0.55, source: source() });
  if (kind === 'form') addFormInputs(g, row, { id, url, source });
}

const RESOURCE_EDGE = { download: 'downloads', api: 'calls_api', resource: 'loads_resource' };
const resourceKind = (row) => (/\.pdf(?:[?#]|$)/i.test(row.src || '') ? 'download' : row.kind === 'script' && /api|graphql|sdk/i.test(row.src || '') ? 'api' : 'resource');

function addResourceNodes(g, resourcesAll, rowSource) {
  for (const row of resourcesAll) {
    const kind = resourceKind(row);
    const id = `${kind}:${row.pageId}:${row.src}`;
    g.addNode({ id, kind, url: row.src, pageId: row.pageId, title: null, text: row.kind || null, selector: null, status: null, workflowTypes: [], risk: null, confidence: 0.7, source: rowSource('extracts/resources.jsonl', row) });
    g.addEdge({ from: row.pageId, to: id, kind: RESOURCE_EDGE[kind], label: row.kind || null, workflowType: null, score: 4, risk: null, confidence: 0.7, source: rowSource('extracts/resources.jsonl', row) });
  }
}

export function buildUnifiedGraph({ rootUrl, pageMaps, siteGraph, workflowIndex, elementRows = [], resourcesAll = [] }) {
  const pageIdByUrl = new Map(pageMaps.map((p) => [p.url, p.pageId]));
  const pageUrlById = new Map(pageMaps.map((p) => [p.pageId, p.url]));
  const workflowTypesByPageId = workflowTypesByPage(workflowIndex);
  const rowSource = (file, row) => ({ file, pageId: row.pageId || null, url: pageUrlById.get(row.pageId) || row.href || row.src || null });
  const g = graphBuilder(pageMaps.map((p) => ({ id: p.pageId, kind: 'page', url: p.url, pageId: p.pageId, title: p.title || null, text: null, selector: null, status: p.status ?? null, workflowTypes: [...(workflowTypesByPageId.get(p.pageId) || [])], risk: null, confidence: 1, source: { file: `pages/${p.pageId}.json`, pageId: p.pageId, url: p.url } })));
  addLinkEdges(g, siteGraph, pageIdByUrl);
  for (const row of elementRows) if (Object.hasOwn(ELEMENT_KIND, row._file ?? '')) addElementNode(g, row, { rowSource, pageUrlById, rootUrl });
  addResourceNodes(g, resourcesAll, rowSource);
  const { nodes, edges } = g;
  return { schemaVersion: 2, rootUrl, totals: { nodes: nodes.length, edges: edges.length, pages: pageMaps.length, actions: nodes.filter((n) => ['form', 'button', 'table', 'pagination'].includes(n.kind)).length, resources: nodes.filter((n) => ['resource', 'api', 'download'].includes(n.kind)).length }, nodes, edges };
}

export function pageSlice({ pageMaps, sources, linksAll, headingsAll, page = 1, pageSize = 20, headingsByPage = byPage(headingsAll), linksByPage = byPage(linksAll) }) {
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
      headingCount: (headingsByPage.get(p.pageId) || []).length,
      linkCount: (linksByPage.get(p.pageId) || []).length,
      searchFirst: p.searchFirst
    };
  });
  return { page: safePage, pageSize, totalPages, totalItems: pageMaps.length, prev: safePage > 1 ? `indexes/pages-${String(safePage - 1).padStart(3, '0')}.json` : null, next: safePage < totalPages ? `indexes/pages-${String(safePage + 1).padStart(3, '0')}.json` : null, rows };
}

export function pageSlices(args) {
  const totalPages = Math.max(1, Math.ceil(args.pageMaps.length / (args.pageSize || 20)));
  const grouped = { headingsByPage: byPage(args.headingsAll), linksByPage: byPage(args.linksAll) };
  return Array.from({ length: totalPages }, (_, i) => pageSlice({ ...args, ...grouped, page: i + 1 }));
}
