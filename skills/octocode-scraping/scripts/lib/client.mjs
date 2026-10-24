import { readFile, writeFile, mkdir, unlink } from 'node:fs/promises';
import { existsSync, realpathSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { basename, dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { errorMessage } from './fail.mjs';
import { guardedFetch } from './net-guard.mjs';

export async function fetchScrapingAnt({ url, pageId, config, apiKey }) {
  const apiUrl = new URL(`https://api.scrapingant.com/v2/${config.endpoint}`);
  apiUrl.searchParams.set('url', url);
  if (apiKey) apiUrl.searchParams.set('x-api-key', apiKey);
  if (config.mode === 'extract') apiUrl.searchParams.set('extract_properties', config.extractProperties);
  if (config.browser) apiUrl.searchParams.set('browser', 'true');
  if (config.waitFor) apiUrl.searchParams.set('wait_for_selector', config.waitFor);
  if (config.proxyType) apiUrl.searchParams.set('proxy_type', config.proxyType);
  if (config.proxyCountry) apiUrl.searchParams.set('proxy_country', config.proxyCountry);
  for (const value of config.blockResources) apiUrl.searchParams.append('block_resource', value);
  for (const [key, value] of config.passParams) apiUrl.searchParams.set(key, value);

  let status = 0, contentType = '', body = '', fetchError = null, creditCost = null, bodyTruncated = false;
  try {
    if (config.mockStatus) {
      status = Number(config.mockStatus);
      contentType = config.mockContentType;
      body = config.mockBodyFile ? await readFile(config.mockBodyFile, 'utf8') : '{"detail":"mock"}';
      creditCost = config.mockCreditCost;
    } else {
      const res = await fetch(apiUrl, { headers: { 'user-agent': 'octocode-scraping/0.1' }, signal: AbortSignal.timeout(20_000) });
      status = res.status;
      contentType = res.headers.get('content-type') || '';
      creditCost = res.headers.get('ant-credits-cost');
      ({ body, truncated: bodyTruncated } = await readBodyBounded(res, responseLimit(config)));
    }
  } catch (error) {
    fetchError = errorMessage(error);
  }
  return { pageId, url, status, contentType, body, fetchError, bodyTruncated, creditCost, fetchedAt: new Date().toISOString() };
}

const CRAWLER_TOKEN = 'OctocodeScraper';
const DIRECT_HEADERS = {
  'user-agent': `${CRAWLER_TOKEN}/1.0 (+https://github.com/bgauryy/octocode)`,
  'accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
  'accept-language': 'en-US,en;q=0.9',
};

function responseLimit(config) {
  return Math.max(Number(config.maxRawBytes) || 0, Number(config.maxTextBytes) || 0, 64_000);
}

async function readBodyBounded(res, maxBytes) {
  if (!res.body) return { body: '', truncated: false };
  const reader = res.body.getReader();
  const chunks = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    const remaining = maxBytes - total;
    if (remaining <= 0) {
      await reader.cancel();
      return { body: Buffer.concat(chunks, total).toString('utf8'), truncated: true };
    }
    const chunk = Buffer.from(value);
    chunks.push(chunk.length > remaining ? chunk.subarray(0, remaining) : chunk);
    total += Math.min(chunk.length, remaining);
    if (chunk.length > remaining) {
      await reader.cancel();
      return { body: Buffer.concat(chunks, total).toString('utf8'), truncated: true };
    }
  }
  return { body: Buffer.concat(chunks, total).toString('utf8'), truncated: false };
}

function retryAfterMs(value, now = Date.now()) {
  if (!value) return null;
  if (/^\d+$/.test(value.trim())) return Number(value.trim()) * 1000;
  const at = Date.parse(value);
  return Number.isFinite(at) ? Math.max(0, at - now) : null;
}

export async function fetchDirect({ url, pageId, config }) {
  let status = 0, contentType = '', body = '', fetchError = null, bodyTruncated = false, retryAfter = null;
  try {
    if (config.mockStatus) {
      status = Number(config.mockStatus);
      contentType = config.mockContentType;
      body = config.mockBodyFile ? await readFile(config.mockBodyFile, 'utf8') : '';
    } else {
      // Every redirect hop is checked, so a public page cannot bounce the fetch onto an internal host.
      const guard = { allowOrigin: config.privateOrigin || null };
      let res = await guardedFetch(url, { headers: DIRECT_HEADERS, signal: AbortSignal.timeout(20_000) }, guard);
      retryAfter = retryAfterMs(res.headers.get('retry-after'));
      // Honor a short server-requested pause once. Longer delays are surfaced to
      // the caller instead of blocking an agent run for an unbounded interval.
      if ((res.status === 429 || res.status === 503) && retryAfter !== null && retryAfter <= 10_000) {
        await sleep(retryAfter);
        res = await guardedFetch(url, { headers: DIRECT_HEADERS, signal: AbortSignal.timeout(20_000) }, guard);
        retryAfter = retryAfterMs(res.headers.get('retry-after'));
      }
      status = res.status;
      contentType = res.headers.get('content-type') || '';
      ({ body, truncated: bodyTruncated } = await readBodyBounded(res, responseLimit(config)));
    }
  } catch (error) {
    fetchError = errorMessage(error);
  }
  return { pageId, url, status, contentType, body, fetchError, bodyTruncated, retryAfterMs: retryAfter, creditCost: null, fetchedAt: new Date().toISOString() };
}

function parseRobots(body, crawlerToken = CRAWLER_TOKEN) {
  const groups = [];
  let agents = [];
  let rules = [];
  const flush = () => {
    if (agents.length) groups.push({ agents, rules });
    agents = [];
    rules = [];
  };
  for (const raw of String(body || '').split(/\r?\n/)) {
    const line = raw.replace(/#.*$/, '').trim();
    if (!line) continue;
    const match = line.match(/^([^:]+):\s*(.*)$/);
    if (!match) continue;
    const field = match[1].trim().toLowerCase();
    const value = match[2].trim();
    if (field === 'user-agent') {
      if (rules.length) flush();
      agents.push(value.toLowerCase());
    } else if ((field === 'allow' || field === 'disallow') && agents.length && value) {
      rules.push({ allow: field === 'allow', path: value });
    }
  }
  flush();
  const token = crawlerToken.toLowerCase();
  const specific = groups.filter((group) => group.agents.includes(token));
  return (specific.length ? specific : groups.filter((group) => group.agents.includes('*'))).flatMap((group) => group.rules);
}

// Percent-encodes non-ASCII and decodes unreserved escapes, so equivalent paths compare equal.
const normalizeRobotsPath = (value) => String(value)
  .replace(/[\u0080-\uFFFF]/g, (char) => encodeURIComponent(char))
  .replace(/%([0-9a-f]{2})/gi, (encoded, hex) => {
    const char = String.fromCharCode(Number.parseInt(hex, 16));
    return /[A-Za-z0-9._~-]/.test(char) ? char : encoded.toUpperCase();
  });

function robotsRuleMatches(rulePath, path) {
  // RFC 9309 is prefix based. Also accept the common '*' and terminal '$'
  // extensions used by major search crawlers.
  const normalizedRule = normalizeRobotsPath(rulePath);
  const normalizedPath = normalizeRobotsPath(path);
  const terminal = normalizedRule.endsWith('$');
  const source = normalizedRule.replace(/\$$/, '').split('*').map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('.*');
  return new RegExp(`^${source}${terminal ? '$' : ''}`).test(normalizedPath);
}

/** Fetch and compile one origin's robots policy. 4xx means unavailable/allow;
 * transport errors and 5xx conservatively disallow this short crawl. */
export async function fetchRobotsPolicy(targetUrl, { allowOrigin = null } = {}) {
  const robotsUrl = new URL('/robots.txt', targetUrl).href;
  const fetchedAt = new Date().toISOString();
  try {
    const res = await guardedFetch(robotsUrl, { headers: DIRECT_HEADERS, signal: AbortSignal.timeout(10_000) }, { allowOrigin });
    if (res.status >= 500) {
      return { url: robotsUrl, status: res.status, fetchedAt, check: () => ({ allowed: false, reason: `robots.txt temporarily unavailable (HTTP ${res.status})` }) };
    }
    if (res.status >= 400) {
      return { url: robotsUrl, status: res.status, fetchedAt, check: () => ({ allowed: true, reason: `robots.txt unavailable (HTTP ${res.status})` }) };
    }
    const { body } = await readBodyBounded(res, 512_000);
    const rules = parseRobots(body);
    return {
      url: robotsUrl,
      status: res.status,
      fetchedAt,
      check(url) {
        const target = new URL(url);
        const path = `${target.pathname}${target.search}`;
        const matches = rules.filter((rule) => robotsRuleMatches(rule.path, path));
        if (!matches.length) return { allowed: true, reason: 'no matching robots rule' };
        matches.sort((a, b) => b.path.length - a.path.length || Number(b.allow) - Number(a.allow));
        return { allowed: matches[0].allow, reason: `${matches[0].allow ? 'Allow' : 'Disallow'}: ${matches[0].path}` };
      },
    };
  } catch (error) {
    return { url: robotsUrl, status: 0, fetchedAt, check: () => ({ allowed: false, reason: `robots.txt fetch failed: ${errorMessage(error)}` }) };
  }
}

// --provider cdp drives the Chrome package through its public executable (`/raw open`,
// `script`, `cleanup`); this skill never imports package internals. Resolve a workspace
// checkout first, then an installed npm package; a missing package yields a fetchError.
const LIB_DIR = dirname(fileURLToPath(import.meta.url));
const CHROME_BIN = 'bin/octocode-chrome-devtools.mjs';
function chromePackageRoot() {
  const workspace = resolve(LIB_DIR, '../../../../packages/octocode-chrome-devtools');
  if (existsSync(join(workspace, CHROME_BIN))) return workspace;
  try { return dirname(createRequire(import.meta.url).resolve('@octocodeai/octocode-chrome-devtools/package.json')); }
  catch { return workspace; }
}
export const CHROME_DEVTOOLS_DIR = chromePackageRoot();
let cdpBrowserLaunched = false; // this process has a ready browser on the port
let cdpBrowserOwned = false;    // ...and launched it (reused sessions belong to the caller)

// Executables are resolved from their installation; state belongs to the caller's workspace.
const chrome = (args, options = {}) =>
  spawnSync(process.execPath, [resolve(CHROME_DEVTOOLS_DIR, CHROME_BIN), '/raw', ...args], { encoding: 'utf8', cwd: process.cwd(), ...options });

async function ensureCdpBrowser(port) {
  if (cdpBrowserLaunched) return { ok: true };
  const res = chrome(['open', '--headless', '--port', port, '--url', 'about:blank']);
  let parsed = null;
  try { parsed = JSON.parse(res.stdout); } catch { /* no JSON status: launch failed */ }
  if (parsed?.status === 'BROWSER_READY') { cdpBrowserLaunched = true; cdpBrowserOwned = parsed.reused !== true; return { ok: true }; }
  return { ok: false, error: (res.stderr || 'failed to launch headless Chrome').slice(0, 300) };
}

// The script the Chrome sandbox runs for one page: navigate, wait, and write the HTML to TMPDIR.
function cdpRunnerSource({ url, config, waitMs, bodyName, netGuardUrl }) {
  return `import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { resolvedUrlProblem } from ${JSON.stringify(netGuardUrl)};
export async function run(cdp) {
  // The Chrome sandbox stages its helpers under <cwd>/.octocode/octocode-chrome-devtools/tmp/helpers.
  const { waitForPageReady } = await import(pathToFileURL(resolve(process.cwd(), '.octocode', 'octocode-chrome-devtools', 'tmp', 'helpers', 'dom-actionability.mjs')).href);
  // Chrome follows redirects and runs page scripts itself, so every request it makes is paused here and
  // failed when it targets a private, local, or link-local host (only the --allow-private-host origin is exempt).
  const refused = [];
  cdp.on('Fetch.requestPaused', async ({ requestId, request }) => {
    const problem = /^https?:/i.test(request.url) ? await resolvedUrlProblem(request.url, { allowOrigin: ${JSON.stringify(config.privateOrigin || null)} }) : null;
    if (problem) { refused.push(request.url); await cdp.send('Fetch.failRequest', { requestId, errorReason: 'AccessDenied' }).catch(() => {}); }
    else await cdp.send('Fetch.continueRequest', { requestId }).catch(() => {});
  });
  await cdp.send('Fetch.enable', { patterns: [{ urlPattern: '*', requestStage: 'Request' }] });
  await cdp.send('Page.enable', {});
  await cdp.send('Network.enable', {});
  let status = 0;
  const { frameTree } = await cdp.send('Page.getFrameTree');
  cdp.on('Network.responseReceived', (p) => { if (p.type === 'Document' && p.frameId === frameTree.frame.id) status = p.response.status; });
  const navigation = await cdp.send('Page.navigate', { url: ${JSON.stringify(url)} });
  if (navigation.errorText) throw new Error(refused.length ? 'refused: private, local, or link-local host ' + refused[0] : navigation.errorText);
  if (!await waitForPageReady(cdp, ${waitMs}, { selector: ${JSON.stringify(config.waitFor || '')}, text: ${JSON.stringify(config.waitText || '')} })) throw new Error('CDP readiness timed out');
  const result = await cdp.send('Runtime.evaluate', { expression: 'document.documentElement.outerHTML', returnByValue: true });
  if (result.exceptionDetails || typeof result.result?.value !== 'string') throw new Error('CDP HTML extraction failed');
  const bodyPath = join(process.env.TMPDIR || tmpdir(), ${JSON.stringify(bodyName)});
  writeFileSync(bodyPath, result.result.value, { mode: 0o600 });
  console.log('CDP_FETCH_RESULT:' + JSON.stringify({ status, bodyPath, refused }));
}
`;
}

const realPath = (path) => { try { return realpathSync(path); } catch { return resolve(path); } };

const runFailure = (run) => run.error?.message || (run.stderr.match(/\[STEALTH_GATE\][^\n]*/)?.[0]) || run.stderr || 'CDP runner failed';
const noResultError = (stderr) => (/ERR_ACCESS_DENIED|Access to this API has been restricted/.test(stderr || '')
  ? 'CDP sandbox ERR_ACCESS_DENIED — the sandbox Permission Model blocked a path. workspace=' + process.cwd() + '. Check the installed Chrome package sandbox read grants.'
  : (stderr || 'no result from CDP sandbox run').slice(0, 500));

// The page body the run wrote, accepted only from the sandbox tree under the expected name.
async function readCdpBody(parsed, { sandboxTree, bodyName }) {
  const result = { status: parsed.status || 0, refusedRequests: parsed.refused || [] };
  const reported = realPath(String(parsed.bodyPath || ''));
  if (!reported.startsWith(realPath(sandboxTree) + sep) || basename(reported) !== bodyName) return { ...result, fetchError: 'CDP body artifact path mismatch' };
  try { return { ...result, bodyPath: reported, body: await readFile(reported, 'utf8') }; }
  catch (error) { return { ...result, bodyPath: reported, fetchError: errorMessage(error) }; }
}

// The sandbox run's outcome: status, refused requests, body (and its file), or a fetch error.
async function readCdpRun(run, paths) {
  if (run.error || run.status !== 0) return { fetchError: String(runFailure(run)) };
  const line = (run.stdout || '').split('\n').find((l) => l.startsWith('CDP_FETCH_RESULT:'));
  if (!line) return { fetchError: noResultError(run.stderr) };
  return readCdpBody(JSON.parse(line.slice('CDP_FETCH_RESULT:'.length)), paths);
}

const mockCdpResult = async ({ url, pageId, config }, fetchedAt) => ({
  pageId,
  url,
  status: Number(config.mockStatus),
  contentType: config.mockContentType || 'text/html',
  body: config.mockBodyFile ? await readFile(config.mockBodyFile, 'utf8') : '',
  fetchError: null,
  creditCost: null,
  fetchedAt,
});

export async function fetchCdp({ url, pageId, config }) {
  const fetchedAt = new Date().toISOString();
  // Hermetic mocks must not launch Chrome — same contract as direct/scrapingant.
  if (config.mockStatus) return mockCdpResult({ url, pageId, config }, fetchedAt);
  const failed = (fetchError) => ({ pageId, url, status: 0, contentType: '', body: '', fetchError, creditCost: null, fetchedAt });
  if (!existsSync(join(CHROME_DEVTOOLS_DIR, CHROME_BIN))) {
    return failed(`Chrome runtime not found at ${CHROME_DEVTOOLS_DIR} — install @octocodeai/octocode-chrome-devtools to use --provider cdp`);
  }
  const port = config.cdpPort || '9331';
  const launch = await ensureCdpBrowser(port);
  if (!launch.ok) return failed(`Chrome launch failed: ${launch.error}`);

  const runnerDir = resolve(process.cwd(), '.octocode', 'octocode-scraping', 'tmp', 'cdp-provider');
  await mkdir(runnerDir, { recursive: true });
  const runnerPath = join(runnerDir, `${pageId}-${randomUUID()}-runner.mjs`);
  // The sandbox writes only below .octocode/octocode-chrome-devtools/tmp; its TMPDIR is there, so the body goes to TMPDIR.
  const bodyName = `${pageId}-${randomUUID()}-body.html`;
  const sandboxTree = resolve(process.cwd(), '.octocode', 'octocode-chrome-devtools', 'tmp');
  const waitMs = config.cdpWaitMs ?? 8000;
  // The runner's sandbox reads only its own folder, so the guard is staged beside it.
  const netGuardPath = join(runnerDir, `${pageId}-${randomUUID()}-net-guard.mjs`);
  await writeFile(netGuardPath, await readFile(join(LIB_DIR, 'net-guard.mjs')), { mode: 0o600 });
  const netGuardUrl = pathToFileURL(netGuardPath).href;
  await writeFile(runnerPath, cdpRunnerSource({ url, config, waitMs, bodyName, netGuardUrl }));

  let outcome = { status: 0, body: '', fetchError: null, refusedRequests: [] };
  try {
    // --stealth applies the package's emulation and its self-test gate before run(cdp).
    const run = chrome(['script', runnerPath, '--port', port, '--new-tab', 'about:blank', '--timeout', String(waitMs + 15000), '--no-reload', ...(config.cdpStealth === true ? ['--stealth'] : [])], { timeout: waitMs + 30000, maxBuffer: 16 * 1024 * 1024 });
    outcome = { ...outcome, ...await readCdpRun(run, { sandboxTree, bodyName }) };
  } catch (error) {
    outcome = { ...outcome, fetchError: errorMessage(error) };
  } finally {
    await unlink(runnerPath).catch(() => {});
    await unlink(netGuardPath).catch(() => {});
    if (outcome.bodyPath) await unlink(outcome.bodyPath).catch(() => {});
  }

  const { status, body, fetchError, refusedRequests } = outcome;
  return { pageId, url, status, contentType: 'text/html', body, fetchError, refusedRequests, creditCost: null, fetchedAt };
}

export async function cleanupCdp(config = {}) {
  const port = config.cdpPort || '9331';
  // Reused browsers belong to their launcher, including on the default port.
  const owned = cdpBrowserLaunched && cdpBrowserOwned;
  cdpBrowserLaunched = false;
  cdpBrowserOwned = false;
  // A reused browser (e.g. a logged-in chrome-devtools session on --cdp-port) is the caller's.
  if (!owned) return;
  // Cleanup finds the tracked session from the browser on the port, whichever cwd launched it.
  chrome(['cleanup', '--port', port]);
}

export async function discoverSitemap({ targetUrl, maxPages, spanHosts = false, privateOrigin = null }) {
  const discovered = [];
  try {
    const readSitemap = async (url) => {
      const response = await guardedFetch(url, { headers: DIRECT_HEADERS, signal: AbortSignal.timeout(20_000) }, { allowOrigin: privateOrigin });
      if (!response.ok) throw new Error(`HTTP ${response.status} for ${url}`);
      const { body, truncated } = await readBodyBounded(response, 1_000_000);
      if (truncated) throw new Error(`stopped at the 1 MB response bound for ${url}`);
      return { isIndex: /<sitemapindex[\s>]/i.test(body), locs: [...body.matchAll(/<loc>\s*(.*?)\s*<\/loc>/g)].map((m) => m[1].trim()).filter(Boolean) };
    };
    const addPages = (locs) => {
      for (const loc of locs) {
        if (discovered.length >= maxPages) break;
        if (spanHosts || new URL(loc).hostname === new URL(targetUrl).hostname) discovered.push(loc);
      }
    };
    const root = await readSitemap(new URL('/sitemap.xml', targetUrl).href);
    if (!root.isIndex) { addPages(root.locs); return { discovered, error: null }; }
    // A sitemap index lists child sitemaps, not pages: read one level of children within the page budget.
    for (const child of root.locs) {
      if (discovered.length >= maxPages) break;
      if (!spanHosts && new URL(child).hostname !== new URL(targetUrl).hostname) continue;
      const sitemap = await readSitemap(child);
      if (!sitemap.isIndex) addPages(sitemap.locs);
    }
    return { discovered, error: null };
  } catch (error) {
    return { discovered, error: `Sitemap discovery failed: ${errorMessage(error)}` };
  }
}

export async function sleep(ms) {
  if (ms > 0) await new Promise((done) => setTimeout(done, ms));
}
