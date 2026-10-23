#!/usr/bin/env node
import { existsSync } from 'node:fs';
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { parseConfig } from './lib/args.mjs';
import { fail } from './lib/fail.mjs';
import { discoverSitemap, fetchRobotsPolicy, sleep } from './lib/client.mjs';
import { initCorpus, writePage, writeSession } from './lib/corpus.mjs';
import { resolvedUrlProblem } from './lib/net-guard.mjs';
import { autoSelectProvider, resolveProvider } from './lib/providers.mjs';

let config;
try {
  config = parseConfig(process.argv.slice(2));
} catch (error) {
  fail('invalidArguments', error instanceof Error ? error.message : String(error), 2);
}

// Reusing a populated session without --append would restart pageIds at
// page-001 and silently overwrite the prior crawl. Refuse; --append continues
// numbering and keeps the prior roster.
const priorSessionDir = join(config.outBase, config.sessionId);
const priorSourcesPath = join(priorSessionDir, 'sources.jsonl');
const pendingDir = join(priorSessionDir, '.pending');
let priorSources = [], recovered = [];
try {
  if (existsSync(pendingDir)) {
    for (const file of (await readdir(pendingDir)).filter(name => /^page-\d+\.json$/.test(name)).sort()) {
      const page = JSON.parse(await readFile(join(pendingDir, file), 'utf8'));
      if (page.sourceRow?.pageId + '.json' !== file || page.pageMap?.pageId !== page.sourceRow.pageId ||
          !['links', 'headings', 'elements', 'resources'].every(key => Array.isArray(page[key]))) throw Error('Invalid page checkpoint ' + file);
      recovered.push(page);
    }
  }
  if (existsSync(priorSourcesPath)) priorSources = (await readFile(priorSourcesPath, 'utf8')).trim().split('\n').filter(Boolean).map(JSON.parse);
} catch (error) { fail('sessionIncomplete', `Cannot recover session: ${error.message}`, 1, { sessionDir: priorSessionDir }); }
const priorIds = new Set([...priorSources.map(row => row.pageId), ...recovered.map(page => page.sourceRow.pageId)]);
for (const folder of ['pages', 'raw', 'text', 'extracts']) {
  const dir = join(priorSessionDir, folder);
  if (!existsSync(dir)) continue;
  for (const file of await readdir(dir)) {
    const id = file.match(/^(page-\d+)[.-]/)?.[1];
    if (id && !priorIds.has(id)) fail('sessionIncomplete', `Uncommitted page ${id}; preserve its artifacts and use a new session`, 1, { sessionDir: priorSessionDir });
  }
}
if (existsSync(priorSourcesPath) || recovered.length) {
  if (!config.append) fail('sessionExists', `--session ${config.sessionId} already contains evidence; pass --append to resume, or use a fresh session`, 2, { sessionDir: priorSessionDir });
  const merged = new Map(priorSources.map(row => [row.pageId, row]));
  for (const page of recovered) merged.set(page.sourceRow.pageId, page.sourceRow);
  priorSources = [...merged.values()];
}

// Resolve 'auto' → direct for html. Browser rendering is an explicit escalation
// after the direct result shows that static HTTP evidence is insufficient.
if (config.provider === 'auto') {
  try {
    config.provider = autoSelectProvider(config.mode, process.env);
  } catch (error) {
    fail('invalidArguments', error instanceof Error ? error.message : String(error), 2);
  }
}

let provider = resolveProvider(config.provider);
// Sync deferred fields so corpus.mjs (manifest.json) records the real provider metadata.
config.apiKeyEnv = provider.apiKeyEnv;
config.requiresApiKey = provider.requiresApiKey;
if (!provider.supportsModes.includes(config.mode)) {
  fail('invalidArguments', `--provider ${config.provider} does not support --mode ${config.mode} (supports: ${provider.supportsModes.join(', ')})`, 2, { provider: config.provider });
}
const apiKey = provider.requiresApiKey ? process.env[provider.apiKeyEnv]?.trim() : null;
if (provider.requiresApiKey && !apiKey && !config.mockStatus) {
  fail('missingKey', `${provider.apiKeyEnv} missing`, 1, { provider: config.provider });
}

// A page, sitemap, or saved frontier can point anywhere; only public hosts and the --url origin the user
// exempted with --allow-private-host are fetched. Mock runs never touch the network.
const hostProblem = (url) => config.mockStatus ? null : resolvedUrlProblem(url, { allowOrigin: config.privateOrigin });
const startProblem = await hostProblem(config.targetUrl);
if (startProblem) {
  fail('refusedHost', `${config.targetUrl}: ${startProblem}${/private/.test(startProblem) ? '; pass --allow-private-host only for a local server the user named' : ''}`, 2);
}

const sessionDir = await initCorpus(config);
const startedAt = new Date().toISOString();
const sources = [], pageMaps = [], linksAll = [], headingsAll = [], elementsAll = [], resourcesAll = [], costs = [], failures = [], refused = [];
// Crawl identity: fragment and trailing-slash variants are one page.
const crawlKey = (u) => {
  try { const x = new URL(u); return `${x.origin}${x.pathname.replace(/\/+$/, '')}${x.search}`; } catch { return u; }
};
const seen = new Set();
const queue = [config.targetUrl];
const queued = new Set([crawlKey(config.targetUrl)]);
const robotsByOrigin = new Map();
let pageIndex = 0;
// --max-pages budgets this run's fetches, so an appended crawl can still add pages to a large session.
function enqueue(href) {
  if (queue.length + pageIndex >= config.maxPages) return false;
  try {
    const next = new URL(href);
    next.hash = '';
    const key = crawlKey(next.href);
    if (!/^https?:$/.test(next.protocol) || seen.has(key) || queued.has(key)) return true;
    if (config.spanHosts || next.hostname === new URL(config.targetUrl).hostname) { queued.add(key); queue.push(next.href); }
  } catch {}
  return true;
}

async function robotsAllows(url) {
  if (!config.crawl || config.mockStatus) return { allowed: true, reason: config.mockStatus ? 'hermetic mock' : 'single explicit URL' };
  const origin = new URL(url).origin;
  if (!robotsByOrigin.has(origin)) robotsByOrigin.set(origin, fetchRobotsPolicy(url, { allowOrigin: config.privateOrigin }));
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
  } catch {
    if (priorSources.some(row => !recovered.some(page => page.sourceRow.pageId === row.pageId))) failures.push('append: prior page-map.json unreadable; only checkpointed pages can recover their map');
  }
  for (const [path, rows] of [['extracts/links.jsonl',linksAll],['extracts/headings.jsonl',headingsAll],['extracts/elements.jsonl',elementsAll],['extracts/resources.jsonl',resourcesAll],['extracts/costs.jsonl',costs]]) {
    if (existsSync(join(sessionDir,path))) rows.push(...(await readFile(join(sessionDir,path),'utf8')).split('\n').filter(Boolean).map(JSON.parse));
  }
  const recoveredIds = new Set(recovered.map(page => page.sourceRow.pageId));
  for (const [rows, field] of [[pageMaps, 'pageMap'], [linksAll, 'links'], [headingsAll, 'headings'], [elementsAll, 'elements'], [resourcesAll, 'resources'], [costs, 'cost']]) {
    const retained = rows.filter(row => !recoveredIds.has(row.pageId));
    rows.length = 0;
    rows.push(...retained);
    for (const page of recovered) {
      const value = page[field];
      if (Array.isArray(value)) rows.push(...value);
      else if (value) rows.push(value);
    }
  }
  // Resume a crawl from the links prior pages found but the session never fetched.
  if (config.crawl) for (const link of linksAll) if (link?.href && !enqueue(link.href)) break;
}

if (config.crawl && config.sitemap) {
  const { discovered, error } = await discoverSitemap(config);
  if (error) failures.push(error);
  for (const loc of discovered) {
    if (queue.length >= config.maxPages) break;
    if (!queued.has(crawlKey(loc))) { queued.add(crawlKey(loc)); queue.push(loc); }
  }
}

while (queue.length && pageIndex < config.maxPages) {
  const url = queue.shift();
  if (seen.has(crawlKey(url))) continue;
  seen.add(crawlKey(url));
  const problem = await hostProblem(url);
  if (problem) {
    refused.push({ url, reason: problem });
    failures.push(`refused ${url}: ${problem}`);
    continue;
  }
  const robots = await robotsAllows(url);
  if (!robots.allowed) {
    failures.push(`robots.txt skipped ${url}: ${robots.reason}`);
    continue;
  }
  pageIndex += 1;
  const pageNumber = basePageCount + pageIndex;
  const pageId = `page-${String(pageNumber).padStart(3, '0')}`;
  const response = await provider.fetch({ url, pageId, config, apiKey });
  for (const blocked of response.refusedRequests || []) refused.push({ url: blocked, reason: `browser request from ${pageId} to a private, local, or link-local host` });
  const written = await writePage({ sessionDir, config, response, pageIndex: pageNumber });
  sources.push(written.sourceRow);
  pageMaps.push(written.pageMap);
  linksAll.push(...written.links);
  headingsAll.push(...written.headings);
  elementsAll.push(...written.elements);
  resourcesAll.push(...written.resources);
  if (written.cost) costs.push(written.cost);

  if (config.crawl && written.sourceRow.ok) {
    for (const link of written.links) if (!enqueue(link.href)) break;
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
for (const row of refused) outputWarnings.push({ pageId: null, warning: `refused ${row.url}: ${row.reason}` });
const appendedNothing = config.append && pageIndex === 0;
if (appendedNothing) outputWarnings.push({ pageId: null, warning: 'no new pages fetched; every queued URL is already in the session' });

console.log(JSON.stringify({
  ok,
  sessionId: config.sessionId,
  sessionDir,
  route: `${config.provider}:${config.mode}`,
  status: first.status || 0,
  contentType: first.contentType || null,
  pages: sources.length,
  newPages: pageIndex,
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
process.exit(ok && !appendedNothing ? 0 : 1);
