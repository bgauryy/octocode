import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const GRAPH_SCHEMA_SOURCE = new URL('../schemas/graph.schema.json', import.meta.url);
import { bytes, cleanForAgent, chunkTextByBytes, detectTargetError, parsePayload, stripTags, titleFromHtml, truncate } from './text.mjs';
import { buildSiteGraph, buildUnifiedGraph, buildWorkflowIndex, pageSlices } from './analyzers.mjs';
import { extractButtonsFromHtml, extractCanonicalFromHtml, extractCodeBlocksFromMarkdown, extractFormsFromHtml, extractHeadings, extractJsonLdFromHtml, extractLinksFromHtml, extractLinksFromMarkdown, extractMetaFromHtml, extractResourcesFromHtml, extractTablesFromHtml, extendedExtractFiles, jsonl } from './extractors.mjs';

export async function initCorpus(config) {
  const sessionDir = resolve(process.cwd(), config.outBase, config.sessionId);
  await mkdir(sessionDir, { recursive: true });
  for (const dir of ['pages', 'raw', 'text', 'extracts', 'snippets', 'reports', 'graph', 'indexes', 'schemas']) await mkdir(join(sessionDir, dir), { recursive: true });
  await writeFile(join(sessionDir, 'schemas/graph.schema.json'), await readFile(GRAPH_SCHEMA_SOURCE, 'utf8'));
  return sessionDir;
}

export async function writePage({ sessionDir, config, response, pageIndex }) {
  const pageId = response.pageId;
  const rawExt = config.mode === 'html' && /html/i.test(response.contentType) ? 'html' : 'json';
  const rawRel = `raw/${pageId}.${rawExt}`;
  const raw = truncate(response.body, config.maxRawBytes);
  if (!config.noRaw && response.body) await writeFile(join(sessionDir, rawRel), raw.text);

  const parsed = parsePayload(config.mode, response.contentType, response.body);
  const providerDetail = parsed.json?.detail ? String(parsed.json.detail) : null;
  const providerStatus = Number(parsed.json?.status ?? parsed.json?.status_code ?? parsed.json?.statusCode);
  const targetLikelyError = detectTargetError({ status: response.status, providerStatus, text: parsed.text, json: parsed.json });
  const cleanSource = cleanForAgent(parsed.text || '');
  const text = truncate(cleanSource, config.maxTextBytes);
  const chunks = chunkTextByBytes(text.text, config.chunkBytes);
  const textParts = [];
  for (let i = 0; i < chunks.length; i += 1) {
    const partRel = `text/${pageId}.clean.part-${String(i + 1).padStart(3, '0')}.md`;
    textParts.push(partRel);
    await writeFile(join(sessionDir, partRel), chunks[i]);
  }
  const textRel = `text/${pageId}.md`;
  await writeFile(join(sessionDir, textRel), `# Text Index: ${pageId}\n\nSource: ${response.url}\nStatus: ${response.status || 'fetch-error'}\nContent-Type: ${response.contentType || 'unknown'}\nCleaned: true\nOriginal text bytes: ${bytes(parsed.text)}\nClean text bytes: ${bytes(cleanSource)}\nChunks: ${textParts.length}\nTruncated: ${text.truncated}\n\n## Search clean chunks\n${textParts.map((p) => `- \`${p}\``).join('\n')}\n\n## Sample\n\n\`\`\`text\n${text.text.slice(0, 2000)}\n\`\`\`\n`);

  const links = /html/i.test(response.contentType) || config.mode === 'html'
    ? extractLinksFromHtml(response.body, response.url, pageId)
    : extractLinksFromMarkdown(cleanSource || parsed.text || '', response.url, pageId);
  const headings = extractHeadings(cleanSource, pageId);
  const h1 = headings.find((h) => h.level === 1)?.text;
  const htmlH1 = /html/i.test(response.contentType) ? stripTags((response.body.match(/<h1\b[^>]*>([\s\S]*?)<\/h1>/i) || [])[1] || '').slice(0, 300) : '';
  const jsonTitle = parsed.json?.title ? String(parsed.json.title).slice(0, 300) : '';
  const htmlTitle = /html/i.test(response.contentType) ? titleFromHtml(response.body) : '';
  const fallbackTitle = parsed.text && !providerDetail ? parsed.text.split(/\r?\n/).find((line) => line.trim())?.replace(/^#+\s*/, '').trim().slice(0, 300) || '' : '';
  const title = h1 || htmlH1 || htmlTitle || jsonTitle || fallbackTitle;
  if (title) await writeFile(join(sessionDir, `snippets/${pageId}-title.txt`), `${title}\n`);

  const htmlLike = /html/i.test(response.contentType) || config.mode === 'html';
  const elementRows = [];
  // resourceRows never flows into buildWorkflowIndex — a stylesheet/script/image has no
  // "workflow type", and folding it into elementRows would feed it through the same
  // JSON.stringify(row)-then-classify path that false-positives on this pipeline's own
  // pageId bookkeeping field (see the pagination-detection fix for the same bug class).
  const resourceRows = [];
  if (htmlLike) {
    elementRows.push(...extractMetaFromHtml(response.body, pageId).map((r) => ({ ...r, _file: 'meta' })));
    elementRows.push(...extractCanonicalFromHtml(response.body, response.url, pageId).map((r) => ({ ...r, _file: 'canonical' })));
    elementRows.push(...extractJsonLdFromHtml(response.body, pageId).map((r) => ({ ...r, _file: 'jsonld' })));
    elementRows.push(...extractFormsFromHtml(response.body, response.url, pageId).map((r) => ({ ...r, _file: 'forms' })));
    elementRows.push(...extractButtonsFromHtml(response.body, response.url, pageId).map((r) => ({ ...r, _file: 'buttons' })));
    elementRows.push(...extractTablesFromHtml(response.body, pageId).map((r) => ({ ...r, _file: 'tables' })));
    resourceRows.push(...extractResourcesFromHtml(response.body, response.url, pageId));
  }
  elementRows.push(...extractCodeBlocksFromMarkdown(cleanSource || parsed.text || '', pageId).map((r) => ({ ...r, _file: 'code-blocks' })));

  if (config.mode === 'extended') {
    for (const [file, rows] of extendedExtractFiles(parsed.json, pageId)) await writeFile(join(sessionDir, 'extracts', file), jsonl(rows));
  }
  if (config.mode === 'extract' && parsed.json) await writeFile(join(sessionDir, `extracts/${pageId}-ai-extract.json`), JSON.stringify(parsed.json, null, 2));

  const sourceRow = {
    pageId,
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
    rawBytes: raw.bytes,
    textTruncated: text.truncated,
    textBytes: bytes(parsed.text),
    cleanTextBytes: bytes(cleanSource),
    antCreditsCost: response.creditCost,
    error: response.fetchError,
    providerStatus: Number.isFinite(providerStatus) ? providerStatus : null,
    providerDetail,
    targetLikelyError
  };
  const pageMeta = {
    pageId,
    url: response.url,
    title,
    status: response.status,
    providerStatus: sourceRow.providerStatus,
    targetLikelyError,
    contentType: response.contentType,
    route: sourceRow.route,
    artifacts: { raw: config.noRaw ? null : rawRel, text: textRel, textParts, links: links.length ? 'extracts/links.jsonl' : null, elements: elementRows.length ? 'extracts/elements.jsonl' : null },
    params: { browser: config.browser, waitFor: config.waitFor, mode: config.mode, extractProperties: config.mode === 'extract' ? config.extractProperties : null },
    limits: { maxRawBytes: config.maxRawBytes, maxTextBytes: config.maxTextBytes, chunkBytes: config.chunkBytes },
    truncation: { raw: raw.truncated, text: text.truncated },
    antCreditsCost: response.creditCost
  };
  await writeFile(join(sessionDir, `pages/${pageId}.json`), JSON.stringify(pageMeta, null, 2));
  const metadata = { pageId, url: response.url, title: title || null, status: response.status, providerStatus: sourceRow.providerStatus, contentType: response.contentType, route: sourceRow.route, textBytes: bytes(parsed.text), cleanBytes: bytes(cleanSource), rawBytes: raw.bytes, textParts: textParts.length, targetLikelyError, antCreditsCost: response.creditCost };
  await writeFile(join(sessionDir, `extracts/${pageId}-metadata.json`), JSON.stringify(metadata, null, 2));
  const pageMap = {
    pageId,
    url: response.url,
    status: response.status,
    title: title || null,
    files: { summary: 'reports/summary.md', failures: 'reports/failures.md', source: 'sources.jsonl', pageMeta: `pages/${pageId}.json`, metadata: `extracts/${pageId}-metadata.json`, headings: 'extracts/headings.jsonl', links: links.length ? 'extracts/links.jsonl' : null, elements: elementRows.length ? 'extracts/elements.jsonl' : null, aiExtract: config.mode === 'extract' ? `extracts/${pageId}-ai-extract.json` : null, textIndex: textRel, textParts, raw: config.noRaw ? null : rawRel },
    searchFirst: [textRel, ...textParts, config.mode === 'extract' ? `extracts/${pageId}-ai-extract.json` : null, 'extracts/headings.jsonl', links.length ? 'extracts/links.jsonl' : null, elementRows.length ? 'extracts/elements.jsonl' : null].filter(Boolean)
  };
  return { sourceRow, pageMap, links, headings, elements: elementRows, resources: resourceRows, cost: response.creditCost ? { pageId, url: response.url, antCreditsCost: Number(response.creditCost), route: sourceRow.route, fetchedAt: response.fetchedAt } : null };
}

const PAGE_SIZE = 20;

export async function writeSession({ sessionDir, config, startedAt, sources, pageMaps, linksAll, headingsAll, elementsAll = [], resourcesAll = [], costs, failures }) {
  await writeFile(join(sessionDir, 'sources.jsonl'), jsonl(sources));
  await writeFile(join(sessionDir, 'extracts/links.jsonl'), jsonl(linksAll));
  await writeFile(join(sessionDir, 'extracts/headings.jsonl'), jsonl(headingsAll));
  await writeFile(join(sessionDir, 'extracts/elements.jsonl'), jsonl(elementsAll));
  await writeFile(join(sessionDir, 'extracts/resources.jsonl'), jsonl(resourcesAll));
  for (const name of ['meta', 'canonical', 'jsonld', 'forms', 'buttons', 'tables', 'code-blocks']) await writeFile(join(sessionDir, 'extracts', `${name}.jsonl`), jsonl(elementsAll.filter((r) => r._file === name)));

  await writeFile(join(sessionDir, 'extracts/metadata.json'), JSON.stringify({ sessionId: config.sessionId, pages: pageMaps.map((p) => ({ pageId: p.pageId, url: p.url, status: p.status, title: p.title, metadata: p.files.metadata, textParts: p.files.textParts.length })), totalPages: pageMaps.length }, null, 2));
  await writeFile(join(sessionDir, 'extracts/costs.jsonl'), jsonl(costs));
  const knownTotal = costs.reduce((n, r) => n + (Number.isFinite(r.antCreditsCost) ? r.antCreditsCost : 0), 0);
  await writeFile(join(sessionDir, 'reports/costs.md'), `# Fetch Costs (${config.provider})\n\nKnown total credits: ${knownTotal}\n\n${costs.map((r) => `- ${r.pageId} ${r.url}: ${r.antCreditsCost}`).join('\n') || 'No cost header captured (provider may not report per-request cost).'}\n`);
  const warnings = sources.filter((s) => s.targetLikelyError).map((s) => ({ pageId: s.pageId, url: s.url, warning: s.targetLikelyError }));
  const graph = buildSiteGraph({ rootUrl: config.targetUrl, pageMaps, sources, linksAll, headingsAll, elementRows: elementsAll });
  const slices = pageSlices({ pageMaps, sources, linksAll, headingsAll, pageSize: PAGE_SIZE });
  const workflowIndex = buildWorkflowIndex({ rootUrl: config.targetUrl, pageMaps, linksAll, headingsAll, elementRows: elementsAll });
  const unifiedGraph = buildUnifiedGraph({ rootUrl: config.targetUrl, pageMaps, siteGraph: graph, workflowIndex, elementRows: elementsAll, resourcesAll });
  await writeFile(join(sessionDir, 'graph/site-graph.json'), JSON.stringify(graph, null, 2));
  await writeFile(join(sessionDir, 'graph/workflows.json'), JSON.stringify(workflowIndex, null, 2));
  await writeFile(join(sessionDir, 'graph/graph.json'), JSON.stringify(unifiedGraph, null, 2));
  for (const slice of slices) await writeFile(join(sessionDir, 'indexes', `pages-${String(slice.page).padStart(3, '0')}.json`), JSON.stringify(slice, null, 2));
  await writeFile(join(sessionDir, 'indexes/pages-summary.json'), JSON.stringify({ totalItems: pageMaps.length, pageSize: PAGE_SIZE, totalPages: slices.length, files: slices.map((s) => `indexes/pages-${String(s.page).padStart(3, '0')}.json`) }, null, 2));
  await writeFile(join(sessionDir, 'indexes/top-links.jsonl'), jsonl(graph.pages.flatMap((p) => p.topLinks.map((l) => ({ pageId: p.pageId, pageUrl: p.url, ...l })))));
  await writeFile(join(sessionDir, 'indexes/workflow-candidates.jsonl'), jsonl(workflowIndex.workflows));
  await writeFile(join(sessionDir, 'reports/workflows.md'), `# Workflow Candidates\n\n${workflowIndex.workflows.slice(0, 50).map((w) => `- ${w.workflowType} (${w.confidence}): ${w.label || w.entryUrl} → ${w.entryUrl}`).join('\n') || 'No workflow candidates found.'}\n`);
  const agentIndex = {
    schemaVersion: 1,
    sessionId: config.sessionId,
    createdAt: startedAt,
    ok: sources.every((s) => s.ok),
    warnings,
    totals: { pages: sources.length, links: linksAll.length, headings: headingsAll.length, elements: elementsAll.length, resources: resourcesAll.length, workflows: workflowIndex.workflows.length, knownAntCredits: knownTotal },
    startHere: ['AGENT_INDEX.json', 'MAP.md', 'page-map.json', 'reports/summary.md', 'sources.jsonl'],
    searchTargets: ['text/*.clean.part-*.md', 'indexes/*.json', 'indexes/*.jsonl', 'graph/graph.json', 'graph/site-graph.json', 'graph/workflows.json', 'extracts/*.jsonl', 'extracts/*-ai-extract.json', 'snippets/*.txt'],
    analysis: { automationGraph: 'graph/graph.json', automationGraphSchema: 'schemas/graph.schema.json', siteGraph: 'graph/site-graph.json', workflows: 'graph/workflows.json', firstPageIndex: 'indexes/pages-001.json', pageSummary: 'indexes/pages-summary.json', topLinks: 'indexes/top-links.jsonl', workflowCandidates: 'indexes/workflow-candidates.jsonl' },
    pagination: { pages: { pattern: 'indexes/pages-*.json', pageSize: PAGE_SIZE, totalPages: slices.length } },
    avoidByDefault: config.noRaw ? [] : ['raw/'],
    pages: pageMaps.slice(0, PAGE_SIZE).map((p) => ({ pageId: p.pageId, url: p.url, status: p.status, title: p.title, searchFirst: p.searchFirst, files: p.files }))
  };
  await writeFile(join(sessionDir, 'AGENT_INDEX.json'), JSON.stringify(agentIndex, null, 2));
  await writeFile(join(sessionDir, 'page-map.json'), JSON.stringify({ sessionId: config.sessionId, createdAt: startedAt, pages: pageMaps }, null, 2));
  await writeFile(join(sessionDir, 'MAP.md'), `# Scrape Session Map\n\nSession: ${config.sessionId}\nCreated: ${startedAt}\n\n| Page | URL | Status | Title | Search first | Raw audit |\n|---|---|---:|---|---|---|\n${pageMaps.map((p) => `| ${p.pageId} | ${p.url} | ${p.status || 0} | ${p.title || ''} | \`${p.files.textIndex}\` → ${p.files.textParts.map((x) => `\`${x}\``).join(', ')} | ${p.files.raw ? `\`${p.files.raw}\`` : 'none'} |`).join('\n')}\n\n## Extracts\n- Metadata: \`extracts/metadata.json\` plus \`extracts/{pageId}-metadata.json\`\n- Headings: \`extracts/headings.jsonl\`\n- Links: \`extracts/links.jsonl\`\n- Costs: \`extracts/costs.jsonl\` / \`reports/costs.md\`\n- Sources: \`sources.jsonl\`\n`);
  await writeFile(join(sessionDir, 'manifest.json'), JSON.stringify({ sessionId: config.sessionId, createdAt: startedAt, target: config.targetUrl, route: `${config.provider}:${config.mode}`, provider: config.provider, apiKeyEnv: config.apiKeyEnv, sessionDir, crawl: config.crawl ? { maxPages: config.maxPages, delayMs: config.delayMs, sameDomain: config.sameDomain, sitemap: config.sitemap } : null, outputs: { agentIndex: 'AGENT_INDEX.json', map: 'MAP.md', pageMap: 'page-map.json', automationGraph: 'graph/graph.json', siteGraph: 'graph/site-graph.json', workflows: 'graph/workflows.json', pageIndexes: 'indexes/pages-*.json', topLinks: 'indexes/top-links.jsonl', workflowCandidates: 'indexes/workflow-candidates.jsonl', raw: config.noRaw ? null : 'raw/', text: 'text/', extracts: 'extracts/', snippets: 'snippets/', reports: 'reports/' }, searchGuidance: { startHere: ['AGENT_INDEX.json', 'indexes/pages-001.json', 'graph/graph.json', 'graph/site-graph.json', 'graph/workflows.json', 'MAP.md', 'page-map.json', 'reports/summary.md', 'sources.jsonl', 'text/*.clean.part-*.md', 'extracts/', 'snippets/'], avoidByDefault: config.noRaw ? [] : ['raw/'] } }, null, 2));
  const ok = sources.every((s) => s.ok);
  const first = sources[0] || {};
  await writeFile(join(sessionDir, 'reports/crawl-summary.md'), `# Crawl Summary\n\nEnabled: ${config.crawl}\nPages fetched: ${sources.length}\nMax pages: ${config.crawl ? config.maxPages : 1}\nSame domain: ${config.sameDomain}\nSitemap: ${config.sitemap}\nDelay ms: ${config.delayMs}\n`);
  await writeFile(join(sessionDir, 'reports/failures.md'), failures.length || !ok ? `# Failures\n\n${failures.map((f) => `- ${f}`).join('\n')}${failures.length ? '\n' : ''}${sources.filter((s) => !s.ok).map((s) => `- ${s.pageId} status ${s.status}: ${s.targetLikelyError || s.providerDetail || s.error || 'HTTP error'}`).join('\n')}\n` : '# Failures\n\nNo fetch failure recorded.\n');
  await writeFile(join(sessionDir, 'reports/summary.md'), `# Scrape Summary\n\nTarget: ${config.targetUrl}\nRoute: ${config.provider}:${config.mode}\nFetched: ${new Date().toISOString()}\nPages: ${sources.length}\nStatus: ${first.status || 'fetch-error'}\nContent-Type: ${first.contentType || 'unknown'}\n\n## Why this corpus matters\n${config.provider} fetched/crawled/extracted the page data; this script normalized it into clean chunks and extracts so Octocode local tools can search/read/prove exact claims without context bloat.\n\n## Artifacts\n- Agent index: \`AGENT_INDEX.json\`\n- Session map: \`MAP.md\` / \`page-map.json\`\n- Source metadata: \`sources.jsonl\`\n- Clean text chunks: \`text/*.clean.part-*.md\`\n- Site graph: \`graph/site-graph.json\`\n- Page indexes: \`indexes/pages-*.json\` / \`indexes/top-links.jsonl\`\n- Metadata: \`extracts/metadata.json\`\n- Headings: \`extracts/headings.jsonl\` (${headingsAll.length})\n- Links: \`extracts/links.jsonl\` (${linksAll.length})\n- Costs: \`reports/costs.md\`\n- Raw audit: \`raw/\`\n\n## Search first\nUse Octocode local tools on \`AGENT_INDEX.json\`, \`indexes/\`, \`graph/site-graph.json\`, \`MAP.md\`, \`page-map.json\`, \`reports/\`, \`text/*.clean.part-*.md\`, \`extracts/\`, \`snippets/\`, \`manifest.json\`, and \`sources.jsonl\`. Read \`raw/\` only for audit/debug.\n${warnings.length ? `\n## Warnings\n${warnings.map((w) => `- ${w.pageId}: ${w.warning}`).join('\n')}\n` : ''}`);
  await writeFile(join(sessionDir, 'README.md'), `# Scrape Session: ${config.sessionId}\n\nTarget: ${config.targetUrl}\nCreated: ${startedAt}\nRoute: ${config.provider}:${config.mode}\nProvider: ${config.provider}\n\n## Start here\n- \`AGENT_INDEX.json\` — compact machine-readable index and search targets\n- \`indexes/pages-001.json\` — paginated page rows for large crawls\n- \`graph/site-graph.json\` — site/workflow graph and smart link candidates\n- \`MAP.md\` / \`page-map.json\` — URL → files map\n- \`reports/summary.md\` — compact overview\n- \`sources.jsonl\` — URL/status/content-type/fetch metadata\n- \`text/\` — cleaned, chunked Markdown/text, good for search\n- \`extracts/\` — structured JSONL rows and metadata\n- \`snippets/\` — small focused evidence excerpts\n\n## Avoid first\n- \`raw/\` — large raw payloads; use only for audit/debug\n`);
  return { ok, first, knownTotal };
}
