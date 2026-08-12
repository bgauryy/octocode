#!/usr/bin/env node
/**
 * Deterministic bridge eval (no live Chrome / no live Walmart):
 * thin homepage scrape → CDP HAR+bodies fixture → har-ingest → corpus-run regex
 * proves a product API field without reopening Chrome.
 */
import { mkdir, rm, writeFile, readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const here = new URL('.', import.meta.url).pathname;
const root = resolve(process.cwd());
const outBase = join(root, '.octocode/tmp/scrape-bridge-eval');
const checks = [];
const assert = (name, condition, detail = '') => checks.push({ name, ok: Boolean(condition), detail: detail == null ? '' : String(detail).slice(0, 1200) });

await rm(outBase, { recursive: true, force: true });
await mkdir(outBase, { recursive: true });
await mkdir(join(outBase, 'cdp-run'), { recursive: true });

// Thin Walmart-like shell (mirrors real direct-fetch scale ~3KB class)
const thinHtml = `<!doctype html><html><head><title>Walmart | Save Money. Live better.</title>
<meta name="description" content="Shop Walmart.com today for Every Day Low Prices."></head>
<body><h1>Walmart | Save Money. Live better.</h1>
<nav>Pickup or delivery? Sign In Account</nav>
<div id="root">Loading…</div>
<script src="/runtime.js"></script>
<a href="/ip/Acer-Monitor/123">Acer Nitro ED0 Series Gaming Monitor</a>
</body></html>`;
const mockFile = join(outBase, 'thin-walmart.html');
await writeFile(mockFile, thinHtml);

const fetchScript = resolve(here, 'fetch.mjs');
const ingestScript = resolve(here, 'har-ingest.mjs');
const runScript = resolve(here, 'corpus-run.mjs');
const exportOnly = resolve(here, 'har-ingest.mjs');

const fetch = spawnSync(process.execPath, [
  fetchScript, '--provider', 'direct', '--url', 'https://www.walmart.com/', '--mode', 'html',
  '--session', 'walmart-thin', '--out', outBase,
  '--mock-status', '200', '--mock-content-type', 'text/html', '--mock-body-file', mockFile,
  '--max-text-bytes', '20000', '--chunk-bytes', '5000', '--no-raw',
], { cwd: root, encoding: 'utf8', timeout: 60000 });
assert('thin scrape fetch ok', fetch.status === 0, fetch.stderr || fetch.stdout);
const sessionDir = join(outBase, 'walmart-thin');
assert('session exists', existsSync(join(sessionDir, 'AGENT_INDEX.json')), sessionDir);

const sources = (await readFile(join(sessionDir, 'sources.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
const cleanBytes = sources[0]?.cleanTextBytes ?? 0;
assert('page is thin-class (<8KB clean)', cleanBytes > 0 && cleanBytes < 8000, cleanBytes);

// Export scrape→CDP packet (flow B)
const packetRun = spawnSync(process.execPath, [exportOnly, '--session-dir', sessionDir, '--export-packet'], { cwd: root, encoding: 'utf8', timeout: 30000 });
assert('export-packet ok', packetRun.status === 0, packetRun.stderr || packetRun.stdout);
assert('bridge-handoff exists', existsSync(join(sessionDir, 'extracts/bridge-handoff.json')));
const handoff = JSON.parse(await readFile(join(sessionDir, 'extracts/bridge-handoff.json'), 'utf8'));
assert('handoff records thinHints', Array.isArray(handoff.thinHints) && handoff.thinHints.length > 0, JSON.stringify(handoff.thinHints));

// CDP fixture: product API body (the thing thin HTML lacks)
const productApi = {
  ok: true,
  data: {
    product: {
      offerId: 'ACCT-000025',
      name: 'Acer Nitro ED0 Series Gaming Monitor 27"',
      price: { amount: 107.1, currency: 'USD' },
      availability: 'IN_STOCK',
    },
    items: [{ id: 1, name: 'alpha', offerId: 'ACCT-000025' }],
  },
};
const bodies = [{
  requestId: 'req-walmart-product-1',
  url: 'https://www.walmart.com/orchestra/home/graphql/GetProduct/abc',
  body: JSON.stringify(productApi),
}];
const har = {
  log: {
    version: '1.2',
    creator: { name: 'octocode-bridge-eval', version: '1' },
    entries: [
      {
        startedDateTime: new Date().toISOString(),
        time: 120,
        request: {
          method: 'GET',
          url: 'https://www.walmart.com/',
          httpVersion: 'HTTP/2',
          cookies: [{ name: 'auth', value: 'SUPERSECRET' }],
          headers: [{ name: 'Authorization', value: 'Bearer SECRET' }, { name: 'Accept', value: 'text/html' }],
          queryString: [],
          headersSize: -1,
          bodySize: 0,
        },
        response: {
          status: 200,
          statusText: 'OK',
          httpVersion: 'HTTP/2',
          cookies: [],
          headers: [{ name: 'content-type', value: 'text/html' }],
          content: { size: thinHtml.length, mimeType: 'text/html', text: '' },
          redirectURL: '',
          headersSize: -1,
          bodySize: thinHtml.length,
        },
        cache: {},
        timings: { send: 0, wait: 100, receive: 20 },
      },
      {
        startedDateTime: new Date().toISOString(),
        time: 45,
        request: {
          method: 'GET',
          url: 'https://www.walmart.com/orchestra/home/graphql/GetProduct/abc',
          httpVersion: 'HTTP/2',
          cookies: [],
          headers: [{ name: 'Accept', value: 'application/json' }],
          queryString: [],
          headersSize: -1,
          bodySize: 0,
        },
        response: {
          status: 200,
          statusText: 'OK',
          httpVersion: 'HTTP/2',
          cookies: [],
          headers: [{ name: 'content-type', value: 'application/json' }],
          content: { size: bodies[0].body.length, mimeType: 'application/json', text: bodies[0].body },
          redirectURL: '',
          headersSize: -1,
          bodySize: bodies[0].body.length,
        },
        cache: {},
        timings: { send: 0, wait: 40, receive: 5 },
        _resourceType: 'xhr',
      },
    ],
  },
};

const cdpRunDir = join(outBase, 'cdp-run');
const harPath = join(cdpRunDir, 'network-body.har');
const bodiesPath = join(cdpRunDir, 'network-bodies.json');
await writeFile(harPath, `${JSON.stringify(har, null, 2)}\n`);
await writeFile(bodiesPath, `${JSON.stringify(bodies, null, 2)}\n`);

const ingest = spawnSync(process.execPath, [
  ingestScript, '--session-dir', sessionDir, '--from-cdp-dir', cdpRunDir, '--filter', 'api',
], { cwd: root, encoding: 'utf8', timeout: 30000 });
assert('har-ingest ok', ingest.status === 0, ingest.stderr || ingest.stdout);
let ingestJson = {};
try { ingestJson = JSON.parse(ingest.stdout); } catch { /* ignore */ }
assert('ingest flow cdp→scrape', ingestJson.flow === 'cdp→scrape', ingest.stdout);
assert('redacted HAR written', existsSync(join(sessionDir, 'cdp')) && (await readFile(join(sessionDir, 'cdp/network-body.redacted.har'), 'utf8').catch(() => '')).includes('[REDACTED]'));
assert('cdp-network extract exists', existsSync(join(sessionDir, 'extracts/cdp-network.jsonl')));
assert('cdp-bodies extract exists', existsSync(join(sessionDir, 'extracts/cdp-bodies.jsonl')));
assert('AGENT_INDEX lists cdp', (await readFile(join(sessionDir, 'AGENT_INDEX.json'), 'utf8')).includes('cdp/'));

// Proof without Chrome: regex finds offerId in ingested body
const run = spawnSync(process.execPath, [
  runScript, '--session-dir', sessionDir, '--roots', 'cdp,extracts', '--regex', 'offerId|IN_STOCK|107\\.1', '--flags', 'g',
], { cwd: root, encoding: 'utf8', timeout: 30000 });
assert('corpus-run ok', run.status === 0, run.stderr || run.stdout);
let runJson = {};
try { runJson = JSON.parse(run.stdout); } catch { /* ignore */ }
assert('regex found product API field', runJson.matchCount > 0, run.stdout);
assert('match cites cdp body file', JSON.stringify(runJson.matches || []).includes('cdp/body-'), run.stdout);

// Also prove artifact-dir mode (chrome-devtools tree directly)
const runArt = spawnSync(process.execPath, [
  runScript, '--artifact-dir', cdpRunDir, '--regex', 'offerId', '--flags', 'g',
], { cwd: root, encoding: 'utf8', timeout: 30000 });
assert('corpus-run artifact-dir ok', runArt.status === 0, runArt.stderr || runArt.stdout);
assert('artifact-dir finds offerId', (runArt.stdout || '').includes('offerId'), runArt.stdout);

// Script mode over corpus
const helperScript = join(outBase, 'prove-offer.mjs');
await writeFile(helperScript, `export async function run(ctx) {
  const text = await ctx.read('cdp/body-001.txt');
  const ok = text.includes('offerId') && text.includes('IN_STOCK');
  return { ok, findings: [{ field: 'offerId', present: text.includes('offerId') }], detail: ok ? 'product api body proven' : 'missing' };
}
`);
const runScriptMode = spawnSync(process.execPath, [
  runScript, '--session-dir', sessionDir, '--script', helperScript,
], { cwd: root, encoding: 'utf8', timeout: 30000 });
assert('corpus-run script mode ok', runScriptMode.status === 0, runScriptMode.stderr || runScriptMode.stdout);
assert('script proven product api', (runScriptMode.stdout || '').includes('product api body proven'), runScriptMode.stdout);

const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
