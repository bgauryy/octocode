#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });
const root = resolve(process.cwd());
const outBase = join(root, '.octocode/tmp/cdp-graph-handoff-eval');
rmSync(outBase, { recursive: true, force: true });
mkdirSync(outBase, { recursive: true });
const html = '<!doctype html><form action="/search"><input name="q" value="cookie"><button type="submit">Search</button></form><a rel="next" href="/page/2">Next</a>';
const mockFile = join(outBase, 'page.html');
writeFileSync(mockFile, html);
const fetch = spawnSync(process.execPath, ['skills/octocode-scraping/scripts/fetch.mjs', '--provider', 'direct', '--url', 'https://example.com/search', '--mode', 'html', '--session', 'handoff', '--out', outBase, '--mock-status', '200', '--mock-content-type', 'text/html', '--mock-body-file', mockFile], { encoding: 'utf8', timeout: 60000 });
assert('scraping graph fixture created', fetch.status === 0, fetch.stderr || fetch.stdout);
const sessionDir = join(outBase, 'handoff');
const graphPath = join(sessionDir, 'graph/graph.json');
assert('real scraping graph exists', existsSync(graphPath), graphPath);
if (existsSync(graphPath)) {
  const graph = JSON.parse(readFileSync(graphPath, 'utf8'));
  assert('scraping graph has action nodes', graph.nodes.some(n => ['form', 'button', 'pagination'].includes(n.kind)), JSON.stringify(graph.nodes));
}
const port = '9295';
const open = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--headless', '--port', port, '--url', 'about:blank'], { encoding: 'utf8', timeout: 60000 });
assert('headless Chrome launches', open.status === 0, open.stderr || open.stdout);
const pageUrl = `data:text/html,${encodeURIComponent(html)}`;
const run = spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs', 'skills/octocode-chrome-devtools/examples/graph-actionability-check.mjs', '--port', port, '--new-tab', pageUrl, '--graph', graphPath, '--timeout', '30000', '--script-timeout', '45000'], { encoding: 'utf8', timeout: 70000, maxBuffer: 5 * 1024 * 1024 });
assert('CDP actionability script runs against scraping graph', run.status === 0, run.stderr.slice(0, 1200));
assert('prints ACTIONABILITY rows', run.stdout.includes('[ACTIONABILITY]'), run.stdout.slice(0, 1200));
const artifactLine = run.stdout.split(/\n/).find(l => l.includes('[ARTIFACT] ACTIONABILITY')) || '';
const artifact = artifactLine.replace(/^.*\[ARTIFACT\] ACTIONABILITY\s+/, '').trim();
assert('actionability artifact exists', artifact && existsSync(artifact), artifact);
if (artifact && existsSync(artifact)) {
  const data = JSON.parse(readFileSync(artifact, 'utf8'));
  assert('graph handoff found form/button/input candidates', data.rows?.some(r => ['form', 'button', 'input', 'a'].includes(r.tag)), JSON.stringify(data.rows?.slice(0, 5)));
  assert('graph handoff has operable candidate', data.rows?.some(r => r.canOperate), JSON.stringify(data.rows?.slice(0, 5)));
}
spawnSync(process.execPath, ['skills/octocode-chrome-devtools/scripts/open-browser.mjs', '--cleanup', '--port', port], { encoding: 'utf8', timeout: 30000 });
const failed = checks.filter(c => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
