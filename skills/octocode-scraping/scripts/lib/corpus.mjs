import { mkdir, readFile, writeFile, rename, rm } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';

const GRAPH_SCHEMA_SOURCE = new URL('../schemas/graph.schema.json', import.meta.url);
import { bytes, cleanForAgent, chunkTextByBytes, detectBrowserNeed, detectTargetError, parsePayload, stripTags, titleFromHtml } from './text.mjs';
import { buildSiteGraph, buildUnifiedGraph, buildWorkflowIndex, pageSlices } from './analyzers.mjs';
import { extractButtonsFromHtml, extractCanonicalFromHtml, extractCodeBlocksFromMarkdown, extractFormsFromHtml, extractHeadings, extractJsonLdFromHtml, extractLinksFromHtml, extractLinksFromMarkdown, extractMetaFromHtml, extractResourcesFromHtml, extractTablesFromHtml, extendedExtractFiles, jsonl } from './extractors.mjs';

async function writeAtomic(file, data, options = {}) {
  const temporary = file + '.' + randomUUID() + '.tmp';
  try {
    await writeFile(temporary, data, { mode: 0o600, ...options });
    await rename(temporary, file);
  } finally { await rm(temporary, { force: true }); }
}

export async function initCorpus(config) {
  const sessionDir = resolve(process.cwd(), config.outBase, config.sessionId);
  await mkdir(sessionDir, { recursive: true });
  for (const dir of ['pages', 'raw', 'text', 'extracts', 'snippets', 'reports', 'graph', 'indexes', 'schemas']) await mkdir(join(sessionDir, dir), { recursive: true });
  await writeAtomic(join(sessionDir, 'schemas/graph.schema.json'), await readFile(GRAPH_SCHEMA_SOURCE, 'utf8'));
  return sessionDir;
}

// The raw file is exactly what the network returned, and text derives from it: both are partial when the body hit the network cap.
async function writeRaw(sessionDir, config, response) {
  const rawExt = config.mode === 'html' && /html/i.test(response.contentType) ? 'html' : 'json';
  const rawRel = `raw/${response.pageId}.${rawExt}`;
  const raw = { text: response.body || '', truncated: Boolean(response.bodyTruncated), bytes: bytes(response.body) };
  if (!config.noRaw && response.body) await writeAtomic(join(sessionDir, rawRel), raw.text);
  return { rawRel, raw };
}

// The provider payload read as text, plus what it says about the target and whether a browser is needed.
function readPayload(config, response) {
  const parsed = parsePayload(config.mode, response.contentType, response.body);
  const providerDetail = parsed.json?.detail ? String(parsed.json.detail) : null;
  const providerStatus = Number(parsed.json?.status ?? parsed.json?.status_code ?? parsed.json?.statusCode);
  const targetLikelyError = detectTargetError({ status: response.status, providerStatus, text: parsed.text, json: parsed.json });
  const cleanSource = cleanForAgent(parsed.text || '');
  const browserReason = config.provider === 'direct' ? detectBrowserNeed({ status: response.status, contentType: response.contentType, body: response.body, cleanText: cleanSource, targetLikelyError }) : null;
  return { parsed, providerDetail, providerStatus, targetLikelyError, cleanSource, browserReason };
}

// Clean text in byte-bounded chunks, and the per-page text index that lists them.
async function writeText(sessionDir, config, response, { parsed, cleanSource }) {
  const { pageId } = response;
  const text = { text: cleanSource, truncated: Boolean(response.bodyTruncated), bytes: bytes(cleanSource) };
  const chunks = chunkTextByBytes(text.text, Math.min(config.chunkBytes, config.maxTextBytes));
  const textParts = [];
  for (let i = 0; i < chunks.length; i += 1) {
    const partRel = `text/${pageId}.clean.part-${String(i + 1).padStart(3, '0')}.md`;
    textParts.push(partRel);
    await writeAtomic(join(sessionDir, partRel), chunks[i]);
  }
  const textRel = `text/${pageId}.md`;
  await writeAtomic(join(sessionDir, textRel), `# Text Index: ${pageId}\n\nSource: ${response.url}\nStatus: ${response.status || 'fetch-error'}\nContent-Type: ${response.contentType || 'unknown'}\nCleaned: true\nOriginal text bytes: ${bytes(parsed.text)}\nClean text bytes: ${bytes(cleanSource)}\nChunks: ${textParts.length}\nTruncated: ${text.truncated}\n\n## Search clean chunks\n${textParts.map((p) => `- \`${p}\``).join('\n')}\n\n## Sample\n\n\`\`\`text\n${text.text.slice(0, 2000)}\n\`\`\`\n`);
  return { text, textParts, textRel };
}

const firstLineTitle = (parsed, providerDetail) => (parsed.text && !providerDetail ? parsed.text.split(/\r?\n/).find((line) => line.trim())?.replace(/^#+\s*/, '').trim().slice(0, 300) || '' : '');

// The first title found: Markdown h1, HTML h1, <title>, a JSON title, then the first text line.
function pageTitle(response, headings, { parsed, providerDetail }) {
  const html = /html/i.test(response.contentType);
  const h1 = headings.find((h) => h.level === 1)?.text;
  const htmlH1 = html ? stripTags((response.body.match(/<h1\b[^>]*>([\s\S]*?)<\/h1>/i) || [])[1] || '').slice(0, 300) : '';
  const jsonTitle = parsed.json?.title ? String(parsed.json.title).slice(0, 300) : '';
  const htmlTitle = html ? titleFromHtml(response.body) : '';
  return h1 || htmlH1 || htmlTitle || jsonTitle || firstLineTitle(parsed, providerDetail);
}

const HTML_ELEMENTS = [
  ['meta', (body) => extractMetaFromHtml(body.html, body.pageId)],
  ['canonical', (body) => extractCanonicalFromHtml(body.html, body.url, body.pageId)],
  ['jsonld', (body) => extractJsonLdFromHtml(body.html, body.pageId)],
  ['forms', (body) => extractFormsFromHtml(body.html, body.url, body.pageId)],
  ['buttons', (body) => extractButtonsFromHtml(body.html, body.url, body.pageId)],
  ['tables', (body) => extractTablesFromHtml(body.html, body.pageId)],
];

// resourceRows never flows into buildWorkflowIndex — a stylesheet/script/image has no
// "workflow type", and folding it into elementRows would feed it through the same
// JSON.stringify(row)-then-classify path that false-positives on this pipeline's own
// pageId bookkeeping field (see the pagination-detection fix for the same bug class).
function pageElements(config, response, { parsed, cleanSource }) {
  const { pageId } = response;
  const elementRows = [];
  const resourceRows = [];
  if (/html/i.test(response.contentType) || config.mode === 'html') {
    const body = { html: response.body, url: response.url, pageId };
    for (const [file, extract] of HTML_ELEMENTS) elementRows.push(...extract(body).map((r) => ({ ...r, _file: file })));
    resourceRows.push(...extractResourcesFromHtml(response.body, response.url, pageId));
  }
  elementRows.push(...extractCodeBlocksFromMarkdown(cleanSource || parsed.text || '', pageId).map((r) => ({ ...r, _file: 'code-blocks' })));
  return { elementRows, resourceRows };
}

async function writeModeExtracts(sessionDir, config, parsed, pageId) {
  if (config.mode === 'extended') {
    for (const [file, rows] of extendedExtractFiles(parsed.json, pageId)) await writeAtomic(join(sessionDir, 'extracts', file), jsonl(rows));
  }
  if (config.mode === 'extract' && parsed.json) await writeAtomic(join(sessionDir, `extracts/${pageId}-ai-extract.json`), JSON.stringify(parsed.json, null, 2));
}

function sourceRowFor(config, response, { payload, rawRel, raw, text, textParts, textRel }) {
  const { parsed, providerDetail, providerStatus, targetLikelyError, cleanSource, browserReason } = payload;
  return {
    pageId: response.pageId,
    url: response.url,
    route: `${config.provider}:${config.mode}${config.browser ? ':browser' : ''}`,
    provider: config.provider,
    status: response.status,
    ok: response.status >= 200 && response.status < 300 && !response.fetchError && !targetLikelyError,
    contentType: response.contentType,
    fetchedAt: response.fetchedAt,
    raw: config.noRaw ? null : rawRel,
    text: textRel,
    textParts,
    rawTruncated: raw.truncated,
    networkTruncated: Boolean(response.bodyTruncated),
    rawBytes: raw.bytes,
    textTruncated: text.truncated,
    textBytes: bytes(parsed.text),
    cleanTextBytes: bytes(cleanSource),
    antCreditsCost: response.creditCost,
    error: response.fetchError,
    providerStatus: Number.isFinite(providerStatus) ? providerStatus : null,
    providerDetail,
    targetLikelyError,
    browserRecommended: Boolean(browserReason),
    browserReason,
    retryAfterMs: response.retryAfterMs ?? null,
  };
}

function pageMetaFor(config, response, { sourceRow, title, rawRel, raw, text, textParts, textRel, links, elementRows }) {
  return {
    pageId: response.pageId,
    url: response.url,
    title,
    status: response.status,
    providerStatus: sourceRow.providerStatus,
    targetLikelyError: sourceRow.targetLikelyError,
    contentType: response.contentType,
    route: sourceRow.route,
    artifacts: { raw: config.noRaw ? null : rawRel, text: textRel, textParts, links: links.length ? 'extracts/links.jsonl' : null, elements: elementRows.length ? 'extracts/elements.jsonl' : null },
    params: { browser: config.browser, waitFor: config.waitFor, mode: config.mode, extractProperties: config.mode === 'extract' ? config.extractProperties : null },
    limits: { maxRawBytes: config.maxRawBytes, maxTextBytes: config.maxTextBytes, chunkBytes: config.chunkBytes },
    truncation: { network: Boolean(response.bodyTruncated), raw: raw.truncated, text: text.truncated },
    browserRecommendation: sourceRow.browserReason,
    retryAfterMs: response.retryAfterMs ?? null,
    antCreditsCost: response.creditCost
  };
}

const metadataFor = (response, { sourceRow, title, raw, textParts }) => ({ pageId: response.pageId, url: response.url, title: title || null, status: response.status, providerStatus: sourceRow.providerStatus, contentType: response.contentType, route: sourceRow.route, textBytes: sourceRow.textBytes, cleanBytes: sourceRow.cleanTextBytes, rawBytes: raw.bytes, textParts: textParts.length, targetLikelyError: sourceRow.targetLikelyError, browserRecommendation: sourceRow.browserReason, networkTruncated: Boolean(response.bodyTruncated), antCreditsCost: response.creditCost });

function pageMapFor(config, response, { title, rawRel, textParts, textRel, links, elementRows }) {
  const { pageId } = response;
  const aiExtract = config.mode === 'extract' ? `extracts/${pageId}-ai-extract.json` : null;
  const linksFile = links.length ? 'extracts/links.jsonl' : null;
  const elementsFile = elementRows.length ? 'extracts/elements.jsonl' : null;
  return {
    pageId,
    url: response.url,
    status: response.status,
    title: title || null,
    files: { summary: 'reports/summary.md', failures: 'reports/failures.md', source: 'sources.jsonl', pageMeta: `pages/${pageId}.json`, metadata: `extracts/${pageId}-metadata.json`, headings: 'extracts/headings.jsonl', links: linksFile, elements: elementsFile, aiExtract, textIndex: textRel, textParts, raw: config.noRaw ? null : rawRel },
    searchFirst: [textRel, ...textParts, aiExtract, 'extracts/headings.jsonl', linksFile, elementsFile].filter(Boolean)
  };
}

export async function writePage({ sessionDir, config, response }) {
  const pageId = response.pageId;
  const { rawRel, raw } = await writeRaw(sessionDir, config, response);
  const payload = readPayload(config, response);
  const { parsed, cleanSource } = payload;
  const { text, textParts, textRel } = await writeText(sessionDir, config, response, payload);

  const links = /html/i.test(response.contentType) || config.mode === 'html'
    ? extractLinksFromHtml(response.body, response.url, pageId)
    : extractLinksFromMarkdown(cleanSource || parsed.text || '', response.url, pageId);
  const headings = extractHeadings(cleanSource, pageId);
  const title = pageTitle(response, headings, payload);
  if (title) await writeAtomic(join(sessionDir, `snippets/${pageId}-title.txt`), `${title}\n`);

  const { elementRows, resourceRows } = pageElements(config, response, payload);
  await writeModeExtracts(sessionDir, config, parsed, pageId);

  const files = { rawRel, raw, text, textParts, textRel, links, elementRows };
  const sourceRow = sourceRowFor(config, response, { payload, ...files });
  await writeAtomic(join(sessionDir, `pages/${pageId}.json`), JSON.stringify(pageMetaFor(config, response, { sourceRow, title, ...files }), null, 2));
  await writeAtomic(join(sessionDir, `extracts/${pageId}-metadata.json`), JSON.stringify(metadataFor(response, { sourceRow, title, ...files }), null, 2));
  const pageMap = pageMapFor(config, response, { title, ...files });
  const committed = { sourceRow, pageMap, links, headings, elements: elementRows, resources: resourceRows, cost: response.creditCost ? { pageId, url: response.url, antCreditsCost: Number(response.creditCost), route: sourceRow.route, fetchedAt: response.fetchedAt } : null };
  // Commit each complete page before fetching another; session indexes can be rebuilt after interruption.
  const pending = join(sessionDir, '.pending');
  await mkdir(pending, { recursive: true });
  const checkpoint = join(pending, pageId + '.json');
  await writeAtomic(checkpoint, JSON.stringify(committed));
  return committed;
}

const PAGE_SIZE = 20;

async function writeExtracts(sessionDir, { sources, linksAll, headingsAll, elementsAll, resourcesAll }) {
  await writeAtomic(join(sessionDir, 'sources.jsonl'), jsonl(sources));
  await writeAtomic(join(sessionDir, 'extracts/links.jsonl'), jsonl(linksAll));
  await writeAtomic(join(sessionDir, 'extracts/headings.jsonl'), jsonl(headingsAll));
  await writeAtomic(join(sessionDir, 'extracts/elements.jsonl'), jsonl(elementsAll));
  await writeAtomic(join(sessionDir, 'extracts/resources.jsonl'), jsonl(resourcesAll));
  for (const name of ['meta', 'canonical', 'jsonld', 'forms', 'buttons', 'tables', 'code-blocks']) await writeAtomic(join(sessionDir, 'extracts', `${name}.jsonl`), jsonl(elementsAll.filter((r) => r._file === name)));
}

// Writes the cost rows and report; returns the known credit total.
async function writeCosts(sessionDir, config, costs) {
  await writeAtomic(join(sessionDir, 'extracts/costs.jsonl'), jsonl(costs));
  const knownTotal = costs.reduce((n, r) => n + (Number.isFinite(r.antCreditsCost) ? r.antCreditsCost : 0), 0);
  await writeAtomic(join(sessionDir, 'reports/costs.md'), `# Fetch Costs (${config.provider})\n\nKnown total credits: ${knownTotal}\n\n${costs.map((r) => `- ${r.pageId} ${r.url}: ${r.antCreditsCost}`).join('\n') || 'No cost header captured (provider may not report per-request cost).'}\n`);
  return knownTotal;
}

function sessionWarnings(sources) {
  const warnings = sources.filter((s) => s.targetLikelyError).map((s) => ({ pageId: s.pageId, url: s.url, warning: s.targetLikelyError }));
  for (const source of sources) if (source.networkTruncated) warnings.push({ pageId: source.pageId, url: source.url, warning: 'response body reached the network byte cap; evidence is partial' });
  return warnings;
}

// The site graph, workflow index, unified graph, and page indexes; returns what the agent index needs.
async function writeGraphs(sessionDir, config, { pageMaps, sources, linksAll, headingsAll, elementsAll, resourcesAll }) {
  const graph = buildSiteGraph({ rootUrl: config.targetUrl, pageMaps, sources, linksAll, headingsAll, elementRows: elementsAll });
  const slices = pageSlices({ pageMaps, sources, linksAll, headingsAll, pageSize: PAGE_SIZE });
  const workflowIndex = buildWorkflowIndex({ rootUrl: config.targetUrl, pageMaps, linksAll, headingsAll, elementRows: elementsAll });
  const unifiedGraph = buildUnifiedGraph({ rootUrl: config.targetUrl, pageMaps, siteGraph: graph, workflowIndex, elementRows: elementsAll, resourcesAll });
  await writeAtomic(join(sessionDir, 'graph/site-graph.json'), JSON.stringify(graph, null, 2));
  await writeAtomic(join(sessionDir, 'graph/workflows.json'), JSON.stringify(workflowIndex, null, 2));
  await writeAtomic(join(sessionDir, 'graph/graph.json'), JSON.stringify(unifiedGraph, null, 2));
  for (const slice of slices) await writeAtomic(join(sessionDir, 'indexes', `pages-${String(slice.page).padStart(3, '0')}.json`), JSON.stringify(slice, null, 2));
  await writeAtomic(join(sessionDir, 'indexes/pages-summary.json'), JSON.stringify({ totalItems: pageMaps.length, pageSize: PAGE_SIZE, totalPages: slices.length, files: slices.map((s) => `indexes/pages-${String(s.page).padStart(3, '0')}.json`) }, null, 2));
  await writeAtomic(join(sessionDir, 'indexes/top-links.jsonl'), jsonl(graph.pages.flatMap((p) => p.topLinks.map((l) => ({ pageId: p.pageId, pageUrl: p.url, ...l })))));
  await writeAtomic(join(sessionDir, 'indexes/workflow-candidates.jsonl'), jsonl(workflowIndex.workflows));
  await writeAtomic(join(sessionDir, 'reports/workflows.md'), `# Workflow Candidates\n\n${workflowIndex.workflows.slice(0, 50).map((w) => `- ${w.workflowType} (${w.confidence}): ${w.label || w.entryUrl} → ${w.entryUrl}`).join('\n') || 'No workflow candidates found.'}\n`);
  return { slices, workflowIndex };
}

function agentIndexFor(config, startedAt, { sources, pageMaps, linksAll, headingsAll, elementsAll, resourcesAll, knownTotal, warnings, slices, workflowIndex }) {
  const browserRecommendations = sources.filter((source) => source.browserRecommended).map((source) => ({ pageId: source.pageId, url: source.url, route: 'octocode-chrome-devtools', reason: source.browserReason }));
  return {
    schemaVersion: 1,
    sessionId: config.sessionId,
    createdAt: startedAt,
    ok: sources.length > 0 && sources.every((s) => s.ok),
    warnings,
    recommendations: browserRecommendations,
    totals: { pages: sources.length, links: linksAll.length, headings: headingsAll.length, elements: elementsAll.length, resources: resourcesAll.length, workflows: workflowIndex.workflows.length, knownAntCredits: knownTotal },
    startHere: ['AGENT_INDEX.json', 'MAP.md', 'page-map.json', 'reports/summary.md', 'sources.jsonl'],
    searchTargets: ['text/*.clean.part-*.md', 'indexes/*.json', 'indexes/*.jsonl', 'graph/graph.json', 'graph/site-graph.json', 'graph/workflows.json', 'extracts/*.jsonl', 'extracts/*-ai-extract.json', 'snippets/*.txt'],
    analysis: { automationGraph: 'graph/graph.json', automationGraphSchema: 'schemas/graph.schema.json', siteGraph: 'graph/site-graph.json', workflows: 'graph/workflows.json', firstPageIndex: 'indexes/pages-001.json', pageSummary: 'indexes/pages-summary.json', topLinks: 'indexes/top-links.jsonl', workflowCandidates: 'indexes/workflow-candidates.jsonl' },
    pagination: { pages: { pattern: 'indexes/pages-*.json', pageSize: PAGE_SIZE, totalPages: slices.length } },
    avoidByDefault: config.noRaw ? [] : ['raw/'],
    pages: pageMaps.slice(0, PAGE_SIZE).map((p) => ({ pageId: p.pageId, url: p.url, status: p.status, title: p.title, searchFirst: p.searchFirst, files: p.files }))
  };
}

const manifestFor = (config, startedAt, sessionDir) => ({ sessionId: config.sessionId, createdAt: startedAt, target: config.targetUrl, route: `${config.provider}:${config.mode}`, provider: config.provider, apiKeyEnv: config.apiKeyEnv, sessionDir, crawl: config.crawl ? { maxPages: config.maxPages, delayMs: config.delayMs, spanHosts: config.spanHosts, sitemap: config.sitemap } : null, outputs: { agentIndex: 'AGENT_INDEX.json', map: 'MAP.md', pageMap: 'page-map.json', automationGraph: 'graph/graph.json', siteGraph: 'graph/site-graph.json', workflows: 'graph/workflows.json', pageIndexes: 'indexes/pages-*.json', topLinks: 'indexes/top-links.jsonl', workflowCandidates: 'indexes/workflow-candidates.jsonl', raw: config.noRaw ? null : 'raw/', text: 'text/', extracts: 'extracts/', snippets: 'snippets/', reports: 'reports/' }, searchGuidance: { startHere: ['AGENT_INDEX.json', 'indexes/pages-001.json', 'graph/graph.json', 'graph/site-graph.json', 'graph/workflows.json', 'MAP.md', 'page-map.json', 'reports/summary.md', 'sources.jsonl', 'text/*.clean.part-*.md', 'extracts/', 'snippets/'], avoidByDefault: config.noRaw ? [] : ['raw/'] } });

async function writeReports(sessionDir, config, { sources, failures, robots, ok, first, warnings, linksAll, headingsAll }) {
  await writeAtomic(join(sessionDir, 'reports/crawl-summary.md'), `# Crawl Summary\n\nEnabled: ${config.crawl}\nPages fetched: ${sources.length}\nMax pages: ${config.crawl ? config.maxPages : 1}\nSpan hosts: ${config.spanHosts}\nSitemap: ${config.sitemap}\nDelay ms: ${config.delayMs}\n`);
  await writeAtomic(join(sessionDir, 'reports/robots.md'), `# Robots Policy\n\n${config.crawl ? `Checked origins: ${robots.length}\n\n${robots.map((row) => `- ${row.origin}: ${row.status || 'fetch-error'} via ${row.url} at ${row.fetchedAt}`).join('\n') || '- No origin was reached.'}` : 'Not checked: this was one explicit URL, not a crawl.'}\n`);
  await writeAtomic(join(sessionDir, 'reports/failures.md'), failures.length || !ok ? `# Failures\n\n${failures.map((f) => `- ${f}`).join('\n')}${failures.length ? '\n' : ''}${sources.filter((s) => !s.ok).map((s) => `- ${s.pageId} status ${s.status}: ${s.targetLikelyError || s.providerDetail || s.error || 'HTTP error'}`).join('\n')}\n` : '# Failures\n\nNo fetch failure recorded.\n');
  await writeAtomic(join(sessionDir, 'reports/summary.md'), `# Scrape Summary\n\nTarget: ${config.targetUrl}\nRoute: ${config.provider}:${config.mode}\nFetched: ${new Date().toISOString()}\nPages: ${sources.length}\nStatus: ${first.status || 'fetch-error'}\nContent-Type: ${first.contentType || 'unknown'}\n\n## Why this corpus matters\n${config.provider} fetched/crawled/extracted the page data; this script normalized it into clean chunks and extracts so Octocode local tools can search/read/prove exact claims without context bloat.\n\n## Artifacts\n- Agent index: \`AGENT_INDEX.json\`\n- Session map: \`MAP.md\` / \`page-map.json\`\n- Source metadata: \`sources.jsonl\`\n- Clean text chunks: \`text/*.clean.part-*.md\`\n- Site graph: \`graph/site-graph.json\`\n- Page indexes: \`indexes/pages-*.json\` / \`indexes/top-links.jsonl\`\n- Metadata: \`extracts/metadata.json\`\n- Headings: \`extracts/headings.jsonl\` (${headingsAll.length})\n- Links: \`extracts/links.jsonl\` (${linksAll.length})\n- Costs: \`reports/costs.md\`\n- Raw audit: \`raw/\`\n\n## Search first\nUse Octocode local tools on \`AGENT_INDEX.json\`, \`indexes/\`, \`graph/site-graph.json\`, \`MAP.md\`, \`page-map.json\`, \`reports/\`, \`text/*.clean.part-*.md\`, \`extracts/\`, \`snippets/\`, \`manifest.json\`, and \`sources.jsonl\`. Read \`raw/\` only for audit/debug.\n${warnings.length ? `\n## Warnings\n${warnings.map((w) => `- ${w.pageId}: ${w.warning}`).join('\n')}\n` : ''}`);
}

export async function writeSession({ sessionDir, config, startedAt, sources, pageMaps, linksAll, headingsAll, elementsAll = [], resourcesAll = [], costs, failures, robots = [] }) {
  const rows = { sources, pageMaps, linksAll, headingsAll, elementsAll, resourcesAll };
  await writeExtracts(sessionDir, rows);
  await writeAtomic(join(sessionDir, 'extracts/metadata.json'), JSON.stringify({ sessionId: config.sessionId, pages: pageMaps.map((p) => ({ pageId: p.pageId, url: p.url, status: p.status, title: p.title, metadata: p.files.metadata, textParts: p.files.textParts.length })), totalPages: pageMaps.length }, null, 2));
  const knownTotal = await writeCosts(sessionDir, config, costs);
  const warnings = sessionWarnings(sources);
  const { slices, workflowIndex } = await writeGraphs(sessionDir, config, rows);
  const agentIndex = agentIndexFor(config, startedAt, { ...rows, knownTotal, warnings, slices, workflowIndex });
  await writeAtomic(join(sessionDir, 'AGENT_INDEX.json'), JSON.stringify(agentIndex, null, 2));
  await writeAtomic(join(sessionDir, 'page-map.json'), JSON.stringify({ sessionId: config.sessionId, createdAt: startedAt, pages: pageMaps }, null, 2));
  await writeAtomic(join(sessionDir, 'MAP.md'), `# Scrape Session Map\n\nSession: ${config.sessionId}\nCreated: ${startedAt}\n\n| Page | URL | Status | Title | Search first | Raw audit |\n|---|---|---:|---|---|---|\n${pageMaps.map((p) => `| ${p.pageId} | ${p.url} | ${p.status || 0} | ${p.title || ''} | \`${p.files.textIndex}\` → ${p.files.textParts.map((x) => `\`${x}\``).join(', ')} | ${p.files.raw ? `\`${p.files.raw}\`` : 'none'} |`).join('\n')}\n\n## Extracts\n- Metadata: \`extracts/metadata.json\` plus \`extracts/{pageId}-metadata.json\`\n- Headings: \`extracts/headings.jsonl\`\n- Links: \`extracts/links.jsonl\`\n- Costs: \`extracts/costs.jsonl\` / \`reports/costs.md\`\n- Sources: \`sources.jsonl\`\n`);
  await writeAtomic(join(sessionDir, 'manifest.json'), JSON.stringify(manifestFor(config, startedAt, sessionDir), null, 2));
  const ok = sources.length > 0 && sources.every((s) => s.ok);
  const first = sources[0] || {};
  await writeReports(sessionDir, config, { sources, failures, robots, ok, first, warnings, linksAll, headingsAll });
  await writeAtomic(join(sessionDir, 'README.md'), `# Scrape Session: ${config.sessionId}\n\nTarget: ${config.targetUrl}\nCreated: ${startedAt}\nRoute: ${config.provider}:${config.mode}\nProvider: ${config.provider}\n\n## Start here\n- \`AGENT_INDEX.json\` — compact machine-readable index and search targets\n- \`indexes/pages-001.json\` — paginated page rows for large crawls\n- \`graph/site-graph.json\` — site/workflow graph and smart link candidates\n- \`MAP.md\` / \`page-map.json\` — URL → files map\n- \`reports/summary.md\` — compact overview\n- \`sources.jsonl\` — URL/status/content-type/fetch metadata\n- \`text/\` — cleaned, chunked Markdown/text, good for search\n- \`extracts/\` — structured JSONL rows and metadata\n- \`snippets/\` — small focused evidence excerpts\n\n## Avoid first\n- \`raw/\` — large raw payloads; use only for audit/debug\n`);
  await rm(join(sessionDir, '.pending'), { recursive: true, force: true });
  return { ok, first, knownTotal };
}
