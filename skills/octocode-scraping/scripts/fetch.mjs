#!/usr/bin/env node
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { propagateOctocodeEnv } from './octocode-config.mjs';
import { parseConfig } from './lib/args.mjs';
import { discoverSitemap, fetchRobotsPolicy, sleep } from './lib/client.mjs';
import { initCorpus, writePage, writeSession } from './lib/corpus.mjs';
import { autoSelectProvider, resolveProvider } from './lib/providers.mjs';

let config;
try {
  config = parseConfig(process.argv.slice(2));
} catch (error) {
  console.error(JSON.stringify({ ok: false, error: error instanceof Error ? error.message : String(error) }, null, 2));
  process.exit(2);
}

// Reusing a populated session without --append would restart pageIds at
// page-001 and silently overwrite the prior crawl. Refuse; --append continues
// numbering and keeps the prior roster.
const priorSourcesPath = join(config.outBase, config.sessionId, 'sources.jsonl');
let priorSources = [];
if (existsSync(priorSourcesPath)) {
  if (!config.append) {
    console.error(JSON.stringify({
      ok: false,
      code: 'SESSION_EXISTS',
      sessionDir: join(config.outBase, config.sessionId),
      error: `--session ${config.sessionId} already holds ${priorSourcesPath}; pass --append to continue its numbering, or omit --session for a fresh session`,
    }, null, 2));
    process.exit(2);
  }
  priorSources = (await readFile(priorSourcesPath, 'utf8')).trim().split('\n').filter(Boolean).map((l) => JSON.parse(l));
}

// Propagate env so keys exist for explicit hosted / non-html modes.
propagateOctocodeEnv({ cwd: process.cwd(), trusted: true });

// Resolve 'auto' → direct for html. Browser rendering is an explicit escalation
// after the direct result shows that static HTTP evidence is insufficient.
if (config.provider === 'auto') {
  try {
    config.provider = autoSelectProvider(config.mode, process.env);
  } catch (error) {
    console.error(JSON.stringify({ ok: false, error: error instanceof Error ? error.message : String(error) }, null, 2));
    process.exit(2);
  }
}

let provider = resolveProvider(config.provider);
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
// Crawl identity: fragment and trailing-slash variants are one page.
const crawlKey = (u) => {
  try { const x = new URL(u); return `${x.origin}${x.pathname.replace(/\/+$/, '')}${x.search}`; } catch { return u; }
};
const seen = new Set();
const queue = [config.targetUrl];
const queued = new Set([crawlKey(config.targetUrl)]);
const robotsByOrigin = new Map();

async function robotsAllows(url) {
  if (!config.crawl || config.mockStatus) return { allowed: true, reason: config.mockStatus ? 'hermetic mock' : 'single explicit URL' };
  const origin = new URL(url).origin;
  if (!robotsByOrigin.has(origin)) robotsByOrigin.set(origin, fetchRobotsPolicy(url));
  const policy = await robotsByOrigin.get(origin);
  return policy.check(url);
}

// --append: keep the prior roster and continue pageId numbering after it.
let basePageCount = 0;
if (priorSources.length) {
  sources.push(...priorSources);
  for (const row of priorSources) if (row.url) seen.add(crawlKey(row.url));
  basePageCount = priorSources.reduce((max, row) => {
    const n = Number((String(row.pageId || '').match(/^page-(\d+)$/) || [])[1] || 0);
    return Math.max(max, n);
  }, 0);
  try {
    const priorMap = JSON.parse(await readFile(join(sessionDir, 'page-map.json'), 'utf8'));
    if (Array.isArray(priorMap?.pages)) pageMaps.push(...priorMap.pages);
  } catch { failures.push('append: prior page-map.json unreadable; roster kept from sources.jsonl only'); }
  for (const [path, rows] of [['extracts/links.jsonl',linksAll],['extracts/headings.jsonl',headingsAll],['extracts/elements.jsonl',elementsAll],['extracts/resources.jsonl',resourcesAll],['extracts/costs.jsonl',costs]]) {
    if (existsSync(join(sessionDir,path))) rows.push(...(await readFile(join(sessionDir,path),'utf8')).split('\n').filter(Boolean).map(JSON.parse));
  }
}

if (config.crawl && config.sitemap) {
  const { discovered, error } = await discoverSitemap(config);
  if (error) failures.push(error);
  for (const loc of discovered) {
    if (queue.length >= config.maxPages) break;
    if (!queued.has(crawlKey(loc))) { queued.add(crawlKey(loc)); queue.push(loc); }
  }
}

let pageIndex = 0;
while (queue.length && pageIndex < config.maxPages) {
  const url = queue.shift();
  if (seen.has(crawlKey(url))) continue;
  seen.add(crawlKey(url));
  const robots = await robotsAllows(url);
  if (!robots.allowed) {
    failures.push(`robots.txt skipped ${url}: ${robots.reason}`);
    continue;
  }
  pageIndex += 1;
  const pageNumber = basePageCount + pageIndex;
  const pageId = `page-${String(pageNumber).padStart(3, '0')}`;
  const response = await provider.fetch({ url, pageId, config, apiKey });
  const written = await writePage({ sessionDir, config, response, pageIndex: pageNumber });
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
        next.hash = '';
        const key = crawlKey(next.href);
        if (!/^https?:$/.test(next.protocol) || seen.has(key) || queued.has(key)) continue;
        if (!config.sameDomain || next.hostname === new URL(config.targetUrl).hostname) { queued.add(key); queue.push(next.href); }
      } catch {}
    }
  }
  if (config.crawl && pageIndex < config.maxPages) await sleep(config.delayMs);
}

if (provider.cleanup) await provider.cleanup(config);

const robots = await Promise.all([...robotsByOrigin.entries()].map(async ([origin, promise]) => {
  const policy = await promise;
  return { origin, url: policy.url, status: policy.status, fetchedAt: policy.fetchedAt };
}));
const { ok, first, knownTotal } = await writeSession({ sessionDir, config, startedAt, sources, pageMaps, linksAll, headingsAll, elementsAll, resourcesAll, costs, failures, robots });
const browserCandidate = sources.find((source) => source.browserRecommended);
const outputWarnings = sources.flatMap((source) => [
  source.targetLikelyError ? { pageId: source.pageId, warning: source.targetLikelyError } : null,
  source.networkTruncated ? { pageId: source.pageId, warning: 'response body reached the network byte cap; evidence is partial' } : null,
].filter(Boolean));

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
  warnings: outputWarnings,
  next: browserCandidate ? {
    route: 'octocode-chrome-devtools',
    reason: browserCandidate.browserReason,
    pageId: browserCandidate.pageId,
    instruction: 'Reuse this scrape session; capture the live page once, then bridge the retained CDP artifact back with har-ingest.mjs.',
  } : null,
  robots: config.crawl ? { checkedOrigins: robots.length, results: robots, report: 'reports/robots.md' } : { checkedOrigins: 0, reason: 'single explicit URL' },
  agentIndex: 'AGENT_INDEX.json',
  analysis: { pageIndex: 'indexes/pages-001.json', siteGraph: 'graph/site-graph.json', workflows: 'graph/workflows.json', topLinks: 'indexes/top-links.jsonl', workflowCandidates: 'indexes/workflow-candidates.jsonl' },
  searchFirst: ['AGENT_INDEX.json', 'indexes/pages-001.json', 'graph/site-graph.json', 'graph/workflows.json', 'MAP.md', 'page-map.json', 'reports/summary.md', 'sources.jsonl', 'text/*.clean.part-*.md', 'extracts/', 'snippets/'],
  rawAudit: config.noRaw ? null : 'raw/'
}));
process.exit(ok ? 0 : 1);
