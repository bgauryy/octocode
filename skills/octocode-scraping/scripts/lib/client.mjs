import { readFile, writeFile, mkdir, unlink } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

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
    fetchError = error instanceof Error ? error.message : String(error);
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
      let res = await fetch(url, { headers: DIRECT_HEADERS, signal: AbortSignal.timeout(20_000) });
      retryAfter = retryAfterMs(res.headers.get('retry-after'));
      // Honor a short server-requested pause once. Longer delays are surfaced to
      // the caller instead of blocking an agent run for an unbounded interval.
      if ((res.status === 429 || res.status === 503) && retryAfter !== null && retryAfter <= 10_000) {
        await sleep(retryAfter);
        res = await fetch(url, { headers: DIRECT_HEADERS, signal: AbortSignal.timeout(20_000) });
        retryAfter = retryAfterMs(res.headers.get('retry-after'));
      }
      status = res.status;
      contentType = res.headers.get('content-type') || '';
      ({ body, truncated: bodyTruncated } = await readBodyBounded(res, responseLimit(config)));
    }
  } catch (error) {
    fetchError = error instanceof Error ? error.message : String(error);
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

function robotsRuleMatches(rulePath, path) {
  // RFC 9309 is prefix based. Also accept the common '*' and terminal '$'
  // extensions used by major search crawlers.
  const normalize = (value) => String(value)
    .replace(/[^\x00-\x7F]/g, (char) => encodeURIComponent(char))
    .replace(/%([0-9a-f]{2})/gi, (encoded, hex) => {
      const char = String.fromCharCode(Number.parseInt(hex, 16));
      return /[A-Za-z0-9._~-]/.test(char) ? char : encoded.toUpperCase();
    });
  const normalizedRule = normalize(rulePath);
  const normalizedPath = normalize(path);
  const terminal = normalizedRule.endsWith('$');
  const source = normalizedRule.replace(/\$$/, '').split('*').map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('.*');
  return new RegExp(`^${source}${terminal ? '$' : ''}`).test(normalizedPath);
}

/** Fetch and compile one origin's robots policy. 4xx means unavailable/allow;
 * transport errors and 5xx conservatively disallow this short crawl. */
export async function fetchRobotsPolicy(targetUrl) {
  const robotsUrl = new URL('/robots.txt', targetUrl).href;
  const fetchedAt = new Date().toISOString();
  try {
    const res = await fetch(robotsUrl, { headers: DIRECT_HEADERS, signal: AbortSignal.timeout(10_000) });
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
    return { url: robotsUrl, status: 0, fetchedAt, check: () => ({ allowed: false, reason: `robots.txt fetch failed: ${error instanceof Error ? error.message : String(error)}` }) };
  }
}

// --provider cdp shells out to the sibling octocode-chrome-devtools skill for real browser
// rendering + stealth, rather than duplicating a CDP client here. Degrades to a clean
// fetchError (not a crash) if that skill isn't installed alongside this one.
//
// chrome-devtools' own session tracking (for its --cleanup flag) is scoped to process.cwd()
// at launch time. If a prior call launched Chrome from a different cwd (crash, inconsistent
// invocation), cleanupCdp()'s call into open-browser.mjs --cleanup silently no-ops — it can't
// find a session file it never wrote. findAndKillPortListener() below is the real fallback:
// verify the port is actually free after that call, and if a genuine Chrome debug process is
// still there, kill it directly, regardless of which cwd tracked it.
const __cdpDir = dirname(fileURLToPath(import.meta.url));
export const CHROME_DEVTOOLS_DIR = resolve(__cdpDir, '../../../octocode-chrome-devtools');
let cdpBrowserLaunched = false;

// The CDP sandbox runs under Node.js Permission Model, granting --allow-fs-read to the cwd
// subtree. If cwd is a subdirectory (e.g. skills/octocode-scraping) the sandbox cannot resolve
// node_modules that live at the monorepo root → ERR_ACCESS_DENIED during ESM import. Fix: pin
// all CDP spawns to the nearest ancestor directory that contains node_modules, so module
// resolution is always within the granted read tree. Falls back to __cdpDir if none found.
function findNodeModulesRoot(from) {
  let d = from;
  while (true) {
    if (existsSync(join(d, 'node_modules'))) return d;
    const parent = dirname(d);
    if (parent === d) return from;
    d = parent;
  }
}
const CDP_SPAWN_CWD = findNodeModulesRoot(__cdpDir);

function findListeningPid(port) {
  try {
    const res = spawnSync('lsof', ['-ti', `:${port}`], { encoding: 'utf8' });
    if (res.status !== 0 || !res.stdout) return null;
    return res.stdout.trim().split('\n')[0] || null;
  } catch { return null; }
}

function isOurChromeDebugProcess(pid, port) {
  try {
    const res = spawnSync('ps', ['-p', pid, '-o', 'command='], { encoding: 'utf8' });
    const cmd = (res.stdout || '').trim();
    return cmd.includes(`--remote-debugging-port=${port}`) && /chrome/i.test(cmd);
  } catch { return false; }
}

/** Best-effort (macOS/Linux via lsof/ps; no-ops elsewhere): kill whatever Chrome debug process is still on `port`, independent of cwd-scoped session tracking. */
function findAndKillPortListener(port) {
  const pid = findListeningPid(port);
  if (pid && isOurChromeDebugProcess(pid, port)) {
    try { process.kill(Number(pid), 'SIGTERM'); return true; } catch { return false; }
  }
  return false;
}

async function ensureCdpBrowser(port) {
  if (cdpBrowserLaunched) return { ok: true };
  const openBrowser = resolve(CHROME_DEVTOOLS_DIR, 'scripts/open-browser.mjs');
  const res = spawnSync(process.execPath, [openBrowser, '--headless', '--port', port, '--url', 'about:blank'], { encoding: 'utf8', cwd: CDP_SPAWN_CWD });
  let parsed = null;
  try { parsed = JSON.parse(res.stdout); } catch {}
  if (parsed?.status === 'BROWSER_READY') { cdpBrowserLaunched = true; return { ok: true }; }
  return { ok: false, error: (res.stderr || 'failed to launch headless Chrome').slice(0, 300) };
}

export async function fetchCdp({ url, pageId, config }) {
  const fetchedAt = new Date().toISOString();
  // Hermetic mocks must not launch Chrome — same contract as direct/scrapingant.
  if (config.mockStatus) {
    return {
      pageId,
      url,
      status: Number(config.mockStatus),
      contentType: config.mockContentType || 'text/html',
      body: config.mockBodyFile ? await readFile(config.mockBodyFile, 'utf8') : '',
      fetchError: null,
      creditCost: null,
      fetchedAt,
    };
  }
  if (!existsSync(CHROME_DEVTOOLS_DIR)) {
    return { pageId, url, status: 0, contentType: '', body: '', fetchError: `octocode-chrome-devtools not found at ${CHROME_DEVTOOLS_DIR} — install it alongside octocode-scraping to use --provider cdp`, creditCost: null, fetchedAt };
  }
  const port = config.cdpPort || '9331';
  const launch = await ensureCdpBrowser(port);
  if (!launch.ok) {
    return { pageId, url, status: 0, contentType: '', body: '', fetchError: `Chrome launch failed: ${launch.error}`, creditCost: null, fetchedAt };
  }

  const sandbox = resolve(CHROME_DEVTOOLS_DIR, 'scripts/cdp-sandbox.mjs');
  const runnerDir = resolve(CDP_SPAWN_CWD, '.octocode', 'tmp', 'cdp-provider');
  await mkdir(runnerDir, { recursive: true });
  const runnerPath = join(runnerDir, `${pageId}-runner.mjs`);
  const bodyPath = join(runnerDir, `${pageId}-body.html`);
  const waitMs = config.cdpWaitMs ?? 2000;
  const stealthModuleUrl = pathToFileURL(resolve(CHROME_DEVTOOLS_DIR, 'scripts/undercover.mjs')).href;
  const stealthStep = config.cdpStealth === false ? '' : `
  const { applyStealthPatches, verifyStealth } = await import(${JSON.stringify(stealthModuleUrl)});
  await applyStealthPatches(cdp);
  const stealthResult = await verifyStealth(cdp);
  console.log('[METRIC] stealth self-test: ' + stealthResult.passed + '/' + stealthResult.total + ' passed');
  if (stealthResult.failed > 0) throw new Error('[STEALTH_GATE] cdp provider fetch blocked: ' + stealthResult.failed + ' stealth checks failed');`;
  await writeFile(runnerPath, `import { writeFileSync } from 'node:fs';
export async function run(cdp) {
  await cdp.send('Page.enable', {});
  await cdp.send('Network.enable', {});
  let status = 0;
  cdp.on('Network.responseReceived', (p) => { if (p.type === 'Document' && status === 0) status = p.response.status; });${stealthStep}
  await cdp.send('Page.navigate', { url: ${JSON.stringify(url)} });
  await new Promise((r) => setTimeout(r, ${waitMs}));
  const result = await cdp.send('Runtime.evaluate', { expression: 'document.documentElement.outerHTML', returnByValue: true });
  writeFileSync(${JSON.stringify(bodyPath)}, result.result.value || '', { mode: 0o600 });
  console.log('CDP_FETCH_RESULT:' + JSON.stringify({ status, bodyPath: ${JSON.stringify(bodyPath)} }));
}
`);

  let status = 0, body = '', fetchError = null;
  try {
    const run = spawnSync(process.execPath, [sandbox, runnerPath, '--port', port, '--new-tab', 'about:blank', '--timeout', String(waitMs + 15000), '--script-timeout', String(waitMs + 20000)], { encoding: 'utf8', cwd: CDP_SPAWN_CWD });
    const line = (run.stdout || '').split('\n').find((l) => l.startsWith('CDP_FETCH_RESULT:'));
    if (line) {
      const parsed = JSON.parse(line.slice('CDP_FETCH_RESULT:'.length));
      status = parsed.status || 0;
      if (parsed.bodyPath !== bodyPath) throw new Error('CDP body artifact path mismatch');
      body = await readFile(bodyPath, 'utf8');
    } else if (/ERR_ACCESS_DENIED|Access to this API has been restricted/.test(run.stderr || '')) {
      fetchError = 'CDP sandbox ERR_ACCESS_DENIED — the sandbox Permission Model blocked a path. CDP_SPAWN_CWD=' + CDP_SPAWN_CWD + '. This should resolve automatically; if it persists, ensure node_modules exists at or above that path.';
    } else {
      fetchError = (run.stderr || 'no result from CDP sandbox run').slice(0, 500);
    }
  } catch (error) {
    fetchError = error instanceof Error ? error.message : String(error);
  } finally {
    await unlink(runnerPath).catch(() => {});
    await unlink(bodyPath).catch(() => {});
  }

  return { pageId, url, status, contentType: 'text/html', body, fetchError, creditCost: null, fetchedAt };
}

export async function cleanupCdp(config = {}) {
  const port = config.cdpPort || '9331';
  if (cdpBrowserLaunched) {
    const openBrowser = resolve(CHROME_DEVTOOLS_DIR, 'scripts/open-browser.mjs');
    spawnSync(process.execPath, [openBrowser, '--port', port, '--cleanup'], { encoding: 'utf8', cwd: CDP_SPAWN_CWD });
    cdpBrowserLaunched = false;
  }
  // Defensive, always runs: the tracked-session cleanup above is cwd-scoped and can silently
  // no-op for a browser orphaned by a previous, differently-cwd'd invocation. Verify the port
  // is actually free; if a genuine Chrome debug process is still there, kill it directly.
  findAndKillPortListener(port);
}

export async function discoverSitemap({ targetUrl, maxPages, sameDomain }) {
  const discovered = [];
  try {
    const smUrl = new URL('/sitemap.xml', targetUrl).href;
    const response = await fetch(smUrl, { headers: DIRECT_HEADERS, signal: AbortSignal.timeout(20_000) });
    if (!response.ok) return { discovered, error: `Sitemap discovery failed: HTTP ${response.status}` };
    const { body: sm, truncated } = await readBodyBounded(response, 1_000_000);
    if (truncated) return { discovered, error: 'Sitemap discovery stopped at the 1 MB response bound' };
    const locs = [...sm.matchAll(/<loc>(.*?)<\/loc>/g)].map((m) => m[1].trim()).filter(Boolean);
    for (const loc of locs) {
      if (discovered.length >= maxPages) break;
      if (!sameDomain || new URL(loc).hostname === new URL(targetUrl).hostname) discovered.push(loc);
    }
    return { discovered, error: null };
  } catch (error) {
    return { discovered, error: `Sitemap discovery failed: ${error instanceof Error ? error.message : String(error)}` };
  }
}

export async function sleep(ms) {
  if (ms > 0) await new Promise((resolve) => setTimeout(resolve, ms));
}
