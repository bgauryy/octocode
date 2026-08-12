#!/usr/bin/env node
import { propagateOctocodeEnv } from '@octocodeai/config';
import { parseConfig } from './lib/args.mjs';
import { discoverSitemap, sleep } from './lib/client.mjs';
import { initCorpus, writePage, writeSession } from './lib/corpus.mjs';
import { autoSelectProvider, resolveProvider } from './lib/providers.mjs';

let config;
try {
  config = parseConfig(process.argv.slice(2));
} catch (error) {
  console.error(JSON.stringify({ ok: false, error: error instanceof Error ? error.message : String(error) }, null, 2));
  process.exit(2);
}

// Propagate env so keys exist for explicit hosted / non-html modes.
propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

// Resolve 'auto' → keyless html route (cdp → direct). Never auto-picks scrapingant.
if (config.provider === 'auto') {
  try {
    config.provider = autoSelectProvider(config.mode, process.env);
  } catch (error) {
    console.error(JSON.stringify({ ok: false, error: error instanceof Error ? error.message : String(error) }, null, 2));
    process.exit(2);
  }
}

const provider = resolveProvider(config.provider);
// Sync deferred fields so corpus.mjs (manifest.json) records the real provider metadata.
config.apiKeyEnv = provider.apiKeyEnv;
config.requiresApiKey = provider.requiresApiKey;
if (!provider.supportsModes.includes(config.mode)) {
  console.error(JSON.stringify({ ok: false, provider: config.provider, error: `--provider ${config.provider} does not support --mode ${config.mode} (supports: ${provider.supportsModes.join(', ')})` }, null, 2));
  process.exit(2);
}
const apiKey = provider.requiresApiKey ? process.env[provider.apiKeyEnv]?.trim() : null;
if (provider.requiresApiKey && !apiKey && !config.mockStatus) {
  console.error(JSON.stringify({ ok: false, provider: config.provider, error: `${provider.apiKeyEnv} missing` }, null, 2));
  process.exit(1);
}

const sessionDir = await initCorpus(config);
const startedAt = new Date().toISOString();
const sources = [], pageMaps = [], linksAll = [], headingsAll = [], elementsAll = [], resourcesAll = [], costs = [], failures = [];
const seen = new Set();
const queue = [config.targetUrl];

if (config.crawl && config.sitemap) {
  const { discovered, error } = await discoverSitemap(config);
  if (error) failures.push(error);
  for (const loc of discovered) if (queue.length < config.maxPages) queue.push(loc);
}

let pageIndex = 0;
while (queue.length && pageIndex < config.maxPages) {
  const url = queue.shift();
  if (seen.has(url)) continue;
  seen.add(url);
  pageIndex += 1;
  const pageId = `page-${String(pageIndex).padStart(3, '0')}`;
  const response = await provider.fetch({ url, pageId, config, apiKey });
  const written = await writePage({ sessionDir, config, response, pageIndex });
  sources.push(written.sourceRow);
  pageMaps.push(written.pageMap);
  linksAll.push(...written.links);
  headingsAll.push(...written.headings);
  elementsAll.push(...written.elements);
  resourcesAll.push(...written.resources);
  if (written.cost) costs.push(written.cost);

  if (config.crawl && written.sourceRow.ok) {
    for (const link of written.links) {
      if (queue.length + seen.size >= config.maxPages) break;
      try {
        const next = new URL(link.href);
        if ((!config.sameDomain || next.hostname === new URL(config.targetUrl).hostname) && !seen.has(next.href)) queue.push(next.href);
      } catch {}
    }
  }
  if (config.crawl && pageIndex < config.maxPages) await sleep(config.delayMs);
}

if (provider.cleanup) await provider.cleanup(config);

const { ok, first, knownTotal } = await writeSession({ sessionDir, config, startedAt, sources, pageMaps, linksAll, headingsAll, elementsAll, resourcesAll, costs, failures });

console.log(JSON.stringify({
  ok,
  sessionId: config.sessionId,
  sessionDir,
  route: `${config.provider}:${config.mode}`,
  status: first.status || 0,
  contentType: first.contentType || null,
  pages: sources.length,
  antCreditsKnownTotal: knownTotal,
  providerDetail: sources.find((s) => s.providerDetail)?.providerDetail || null,
  warnings: sources.filter((s) => s.targetLikelyError).map((s) => ({ pageId: s.pageId, warning: s.targetLikelyError })),
  agentIndex: 'AGENT_INDEX.json',
  analysis: { pageIndex: 'indexes/pages-001.json', siteGraph: 'graph/site-graph.json', workflows: 'graph/workflows.json', topLinks: 'indexes/top-links.jsonl', workflowCandidates: 'indexes/workflow-candidates.jsonl' },
  searchFirst: ['AGENT_INDEX.json', 'indexes/pages-001.json', 'graph/site-graph.json', 'graph/workflows.json', 'MAP.md', 'page-map.json', 'reports/summary.md', 'sources.jsonl', 'text/*.clean.part-*.md', 'extracts/', 'snippets/'],
  rawAudit: config.noRaw ? null : 'raw/'
}, null, 2));
process.exit(ok ? 0 : 1);
