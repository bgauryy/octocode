#!/usr/bin/env node
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { PROVIDERS } from './lib/providers.mjs';

const here = new URL('.', import.meta.url).pathname;
const fetchScript = resolve(here, 'scrapingant-fetch.mjs');
const checkScript = resolve(here, 'scrapingant-check.mjs');
const root = resolve(process.cwd());
const outBase = join(root, '.octocode/tmp/scrape-providers-eval');
const checks = [];
const assert = (name, condition, detail = '') => checks.push({ name, ok: Boolean(condition), detail });

await rm(outBase, { recursive: true, force: true });
await mkdir(outBase, { recursive: true });

// Registry descriptors conform to the schema's required-field contract.
const schema = JSON.parse(await readFile(resolve(here, 'schemas/provider.schema.json'), 'utf8'));
const descriptorSchema = schema.$defs.ProviderDescriptor;
for (const [name, descriptor] of Object.entries(PROVIDERS)) {
  const missing = descriptorSchema.required.filter((key) => !(key in descriptor));
  assert(`${name}: descriptor has all required fields`, missing.length === 0, missing.join(', '));
  assert(`${name}: fetch is a function`, typeof descriptor.fetch === 'function');
  assert(`${name}: supportsModes is a non-empty array`, Array.isArray(descriptor.supportsModes) && descriptor.supportsModes.length > 0);
}

// direct provider needs no key.
const checkDirect = spawnSync(process.execPath, [checkScript, '--provider', 'direct'], { encoding: 'utf8' });
const directCheck = JSON.parse(checkDirect.stdout || '{}');
assert('scrapingant-check.mjs --provider direct reports no key required', checkDirect.status === 0 && directCheck.key === 'not-required', checkDirect.stdout);

// auto-selection: no flags → auto=true, selected=scrapingant (SCRAPING_ANT is set in this env).
const checkAnt = spawnSync(process.execPath, [checkScript], { encoding: 'utf8' });
const antCheck = JSON.parse(checkAnt.stdout || '{}');
assert('scrapingant-check.mjs (no flags) auto-selects scrapingant when SCRAPING_ANT is set', antCheck.auto === true && antCheck.selected === 'scrapingant' && antCheck.apiKeyEnv === 'SCRAPING_ANT', checkAnt.stdout);

// unknown provider is rejected cleanly.
const badProvider = spawnSync(process.execPath, [fetchScript, '--url', 'https://example.com', '--provider', 'nonexistent'], { encoding: 'utf8' });
assert('unknown --provider rejected', badProvider.status !== 0 && badProvider.stderr.includes('Unknown --provider'), badProvider.stderr.slice(0, 300));

// direct provider cannot be used with a vendor-only mode.
const badMode = spawnSync(process.execPath, [fetchScript, '--url', 'https://example.com', '--provider', 'direct', '--mode', 'markdown'], { encoding: 'utf8' });
assert('direct + markdown mode rejected (vendor-only mode)', badMode.status !== 0 && badMode.stderr.includes('does not support'), badMode.stderr.slice(0, 300));

// direct provider: mocked fetch produces the same corpus shape as scrapingant (proves corpus/extraction logic is vendor-agnostic).
const html = `<!doctype html><html><head><title>Direct Hub</title></head><body><h1>Direct Hub</h1><a href="/pricing">Pricing</a></body></html>`;
const mockFile = join(outBase, 'mock.html');
await writeFile(mockFile, html);
const directRun = spawnSync(process.execPath, [fetchScript, '--url', 'https://example.com', '--mode', 'html', '--provider', 'direct', '--session', 'direct-run', '--mock-status', '200', '--mock-content-type', 'text/html', '--mock-body-file', mockFile, '--out', outBase], { encoding: 'utf8' });
checks.push({ name: 'direct provider mocked run succeeds', ok: directRun.status === 0, status: directRun.status, stderr: directRun.stderr.slice(0, 500) });
const directDir = join(outBase, 'direct-run');
for (const rel of ['AGENT_INDEX.json', 'graph/graph.json', 'graph/site-graph.json', 'graph/workflows.json', 'schemas/graph.schema.json', 'manifest.json']) assert(`direct provider: ${rel} exists`, existsSync(join(directDir, rel)));
const directAgent = JSON.parse(await readFile(join(directDir, 'AGENT_INDEX.json'), 'utf8'));
const directManifest = JSON.parse(await readFile(join(directDir, 'manifest.json'), 'utf8'));
assert('direct provider: manifest reports provider=direct', directManifest.provider === 'direct', directManifest.provider);
assert('direct provider: manifest apiKeyEnv is null', directManifest.apiKeyEnv === null, directManifest.apiKeyEnv);
assert('direct provider: route is direct:html', JSON.parse(directRun.stdout).route === 'direct:html', directRun.stdout.slice(0, 200));
assert('direct provider: workflow classification still runs (pricing found)', directAgent.totals.workflows >= 1);
assert('direct provider: no cost rows captured (no vendor cost concept)', (await readFile(join(directDir, 'extracts/costs.jsonl'), 'utf8')).trim() === '');

// live, real network proof: direct provider is genuinely usable with zero vendor/key ("curl from outside").
const liveRun = spawnSync(process.execPath, [fetchScript, '--url', 'https://example.com', '--mode', 'html', '--provider', 'direct', '--session', 'direct-live', '--out', outBase], { encoding: 'utf8' });
const liveData = JSON.parse(liveRun.stdout || '{}');
assert('direct provider: real live fetch (no API key) succeeds', liveRun.status === 0 && liveData.ok === true && liveData.status === 200, liveRun.stderr.slice(0, 300));

// cdp provider: real fetch via the sibling octocode-chrome-devtools skill. Gate gracefully —
// don't fail the whole suite in an environment without that skill or without Chrome.
const chromeDevtoolsDir = resolve(here, '../../octocode-chrome-devtools');
if (existsSync(chromeDevtoolsDir)) {
  const cdpRun = spawnSync(process.execPath, [fetchScript, '--url', 'https://example.com', '--mode', 'html', '--provider', 'cdp', '--session', 'cdp-live', '--out', outBase], { encoding: 'utf8', cwd: root });
  let cdpData = null;
  try { cdpData = JSON.parse(cdpRun.stdout); } catch {}
  checks.push({ name: 'cdp provider: real live fetch succeeds (no vendor key)', ok: cdpRun.status === 0 && cdpData?.ok === true && cdpData?.status === 200, detail: cdpData ? '' : cdpRun.stderr.slice(0, 500) });
  if (cdpData?.ok) {
    assert('cdp provider: route is cdp:html', cdpData.route === 'cdp:html', cdpData.route);
    const cdpDir = join(outBase, 'cdp-live');
    for (const rel of ['AGENT_INDEX.json', 'graph/graph.json', 'manifest.json']) assert(`cdp provider: ${rel} exists`, existsSync(join(cdpDir, rel)));
    const cdpManifest = JSON.parse(await readFile(join(cdpDir, 'manifest.json'), 'utf8'));
    assert('cdp provider: manifest reports provider=cdp, apiKeyEnv null', cdpManifest.provider === 'cdp' && cdpManifest.apiKeyEnv === null, JSON.stringify(cdpManifest.provider));
  }
} else {
  checks.push({ name: 'cdp provider: skipped (octocode-chrome-devtools not installed alongside)', ok: true, detail: chromeDevtoolsDir });
}

const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
