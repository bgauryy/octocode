import { readFile, writeFile, mkdir, unlink } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

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

  let status = 0, contentType = '', body = '', fetchError = null, creditCost = null;
  try {
    if (config.mockStatus) {
      status = Number(config.mockStatus);
      contentType = config.mockContentType;
      body = config.mockBodyFile ? await readFile(config.mockBodyFile, 'utf8') : '{"detail":"mock"}';
      creditCost = config.mockCreditCost;
    } else {
      const res = await fetch(apiUrl, { headers: { 'user-agent': 'octocode-scraping/0.1' } });
      status = res.status;
      contentType = res.headers.get('content-type') || '';
      creditCost = res.headers.get('ant-credits-cost');
      body = await res.text();
    }
  } catch (error) {
    fetchError = error instanceof Error ? error.message : String(error);
  }
  return { pageId, url, status, contentType, body, fetchError, creditCost, fetchedAt: new Date().toISOString() };
}

// Realistic browser headers for direct HTTP fetches — reduces naive bot detection without a
// vendor. Matches what Chrome 124 on macOS sends for a top-level document navigation.
const DIRECT_STEALTH_HEADERS = {
  'user-agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36',
  'accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7',
  'accept-language': 'en-US,en;q=0.9',
  'accept-encoding': 'gzip, deflate, br',
  'cache-control': 'no-cache',
  'pragma': 'no-cache',
  'sec-ch-ua': '"Chromium";v="124", "Google Chrome";v="124", "Not-A.Brand";v="99"',
  'sec-ch-ua-mobile': '?0',
  'sec-ch-ua-platform': '"macOS"',
  'sec-fetch-dest': 'document',
  'sec-fetch-mode': 'navigate',
  'sec-fetch-site': 'none',
  'sec-fetch-user': '?1',
  'upgrade-insecure-requests': '1',
};

export async function fetchDirect({ url, pageId, config }) {
  let status = 0, contentType = '', body = '', fetchError = null;
  try {
    if (config.mockStatus) {
      status = Number(config.mockStatus);
      contentType = config.mockContentType;
      body = config.mockBodyFile ? await readFile(config.mockBodyFile, 'utf8') : '';
    } else {
      const res = await fetch(url, { headers: DIRECT_STEALTH_HEADERS });
      status = res.status;
      contentType = res.headers.get('content-type') || '';
      body = await res.text();
    }
  } catch (error) {
    fetchError = error instanceof Error ? error.message : String(error);
  }
  return { pageId, url, status, contentType, body, fetchError, creditCost: null, fetchedAt: new Date().toISOString() };
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
  const waitMs = config.cdpWaitMs ?? 2000;
  const stealthStep = config.cdpStealth === false ? '' : `
  try {
    const { applyStealthPatches } = await import(pathToFileURL(resolve(CDP_SPAWN_CWD, '.octocode', 'undercover.mjs')).href);
    await applyStealthPatches(cdp);
  } catch (_) {}`;
  await writeFile(runnerPath, `import { resolve } from 'path';
import { pathToFileURL } from 'url';
export async function run(cdp) {
  await cdp.send('Page.enable', {});
  await cdp.send('Network.enable', {});
  let status = 0;
  cdp.on('Network.responseReceived', (p) => { if (p.type === 'Document' && status === 0) status = p.response.status; });${stealthStep}
  await cdp.send('Page.navigate', { url: ${JSON.stringify(url)} });
  await new Promise((r) => setTimeout(r, ${waitMs}));
  const result = await cdp.send('Runtime.evaluate', { expression: 'document.documentElement.outerHTML', returnByValue: true });
  console.log('CDP_FETCH_RESULT:' + JSON.stringify({ status, html: result.result.value }));
}
`);

  let status = 0, body = '', fetchError = null;
  try {
    const run = spawnSync(process.execPath, [sandbox, runnerPath, '--port', port, '--new-tab', 'about:blank', '--timeout', String(waitMs + 15000), '--script-timeout', String(waitMs + 20000)], { encoding: 'utf8', cwd: CDP_SPAWN_CWD });
    const line = (run.stdout || '').split('\n').find((l) => l.startsWith('CDP_FETCH_RESULT:'));
    if (line) {
      const parsed = JSON.parse(line.slice('CDP_FETCH_RESULT:'.length));
      status = parsed.status || 0;
      body = parsed.html || '';
    } else if (/ERR_ACCESS_DENIED|Access to this API has been restricted/.test(run.stderr || '')) {
      fetchError = 'CDP sandbox ERR_ACCESS_DENIED — the sandbox Permission Model blocked a path. CDP_SPAWN_CWD=' + CDP_SPAWN_CWD + '. This should resolve automatically; if it persists, ensure node_modules exists at or above that path.';
    } else {
      fetchError = (run.stderr || 'no result from CDP sandbox run').slice(0, 500);
    }
  } catch (error) {
    fetchError = error instanceof Error ? error.message : String(error);
  } finally {
    await unlink(runnerPath).catch(() => {});
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
    const sm = await fetch(smUrl).then((r) => r.text());
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
