#!/usr/bin/env node
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const here = new URL('.', import.meta.url).pathname;
const fetchScript = resolve(here, 'scrapingant-fetch.mjs');
const schemaScript = resolve(here, 'schema-helper.mjs');
const root = resolve(process.cwd());
const outBase = join(root, '.octocode/tmp/scrape-analysis-eval');
const checks = [];
const assert = (name, condition, detail = '') => checks.push({ name, ok: Boolean(condition), detail });
const run = (name, args, expectOk = true) => {
  const res = spawnSync(process.execPath, [fetchScript, ...args, '--out', outBase], { cwd: root, encoding: 'utf8' });
  checks.push({ name, ok: expectOk ? res.status === 0 : res.status !== 0, status: res.status, stdout: res.stdout.slice(0, 1000), stderr: res.stderr.slice(0, 1000) });
  return res;
};
await rm(outBase, { recursive: true, force: true });
await mkdir(outBase, { recursive: true });
const html = `<!doctype html><html><head><title>Noisy Banner Title</title><link rel="canonical" href="/home"><link rel="stylesheet" href="/styles/main.css"><link rel="alternate" type="application/rss+xml" href="/feed.xml"><script src="/app.js"></script><meta name="description" content="API docs and pricing"><script type="application/ld+json">{"@type":"SoftwareApplication","name":"Demo"}</script></head><body><nav><a href="/docs/api">API Reference</a><a href="/pricing">Pricing</a></nav><h1>Developer Platform</h1><img src="/hero.png" alt="hero"><a href="/signup">Start free trial</a><form action="/signup" method="post"><label>Email</label><input type="email" name="email"><button>Sign up</button></form><table><tr><th>Plan</th><th>Price</th></tr><tr><td>Free</td><td>$0</td></tr></table><pre><code>curl https://api.example.com/v1/items</code></pre><nav class="pagination-nav"><a class="pagination-nav__link pagination-nav__link--prev" href="/docs/intro"><div>Previous</div><div>Intro</div></a><a class="pagination-nav__link pagination-nav__link--next" href="/docs/advanced"><div>Next</div><div>Advanced</div></a></nav></body></html>`;
const mockFile = join(outBase, 'mock-site.html');
await writeFile(mockFile, html);
run('html analysis corpus', ['--url', 'https://example.com', '--mode', 'html', '--session', 'site-analysis', '--mock-status', '200', '--mock-content-type', 'text/html', '--mock-body-file', mockFile]);
const dir = join(outBase, 'site-analysis');
for (const rel of ['AGENT_INDEX.json', 'graph/graph.json', 'graph/site-graph.json', 'graph/workflows.json', 'indexes/pages-001.json', 'indexes/pages-summary.json', 'indexes/top-links.jsonl', 'indexes/workflow-candidates.jsonl', 'extracts/forms.jsonl', 'extracts/buttons.jsonl', 'extracts/tables.jsonl', 'extracts/meta.jsonl', 'extracts/canonical.jsonl', 'extracts/jsonld.jsonl', 'extracts/elements.jsonl']) assert(`${rel} exists`, existsSync(join(dir, rel)));
const agent = JSON.parse(await readFile(join(dir, 'AGENT_INDEX.json'), 'utf8'));
assert('agent has element/workflow totals', agent.totals.elements >= 5 && agent.totals.workflows >= 3, JSON.stringify(agent.totals));
assert('title prefers h1', agent.pages[0].title === 'Developer Platform', agent.pages[0].title);
const automationGraph = JSON.parse(await readFile(join(dir, 'graph/graph.json'), 'utf8'));
assert('automation graph is schema v2', automationGraph.schemaVersion === 2);
assert('automation graph has form/input/button/table/resource nodes', ['form', 'input', 'button', 'table', 'resource'].every((kind) => automationGraph.nodes.some((n) => n.kind === kind)));
assert('automation graph edges carry evidence', automationGraph.edges.every((e) => e.source && e.source.file));
assert('automation action nodes carry selectors', automationGraph.nodes.filter((n) => ['form', 'input', 'button', 'table'].includes(n.kind)).every((n) => typeof n.selector === 'string' && n.selector.length > 0));
const workflows = JSON.parse(await readFile(join(dir, 'graph/workflows.json'), 'utf8'));
assert('api workflow found', workflows.workflows.some((w) => w.workflowType === 'api-reference'));
assert('pricing workflow found', workflows.workflows.some((w) => w.workflowType === 'pricing'));
assert('signup workflow found', workflows.workflows.some((w) => w.workflowType === 'signup'));
const paginationCandidates = workflows.workflows.filter((w) => w.workflowType === 'pagination');
assert('pagination detected via class (no rel attribute, Docusaurus-style)', paginationCandidates.some((w) => w.entryUrl.endsWith('/docs/advanced')) && paginationCandidates.some((w) => w.entryUrl.endsWith('/docs/intro')));
// Regression guard: pagination must come from real rel/class attributes, never from a generic
// blob-matched heuristic — otherwise this false-positives on the pipeline's own internal
// pageId bookkeeping field ("page-001") the moment any element/heading gets JSON-stringified.
assert('pagination candidates come only from the two real pagination-nav links (no bookkeeping false positives)', paginationCandidates.length === 2, JSON.stringify(paginationCandidates.map((w) => w.entryUrl)));
assert('boilerplate skip/nav labels are not workflow candidates', !workflows.workflows.some((w) => /Skip to main content|Products|Resources/.test(w.label || '')));
// Regression guard: meta description "API docs and pricing" would match api-reference/docs/
// pricing rules on content alone, but a meta tag has no href of its own — its entryUrl always
// falls back to the current page, producing a non-actionable duplicate. Verified live on
// docs.scrapingant.com, where 15 of 24 element-sourced candidates were exactly this pattern.
// meta/jsonld rows have no `.kind`, so they'd surface as the generic source "element" — assert
// that value never appears (forms/buttons/tables all set a distinct .kind and are unaffected).
assert('no bare "element"-sourced workflow candidates (meta/jsonld have no actionable target of their own)', workflows.workflows.every((w) => w.source !== 'element'), JSON.stringify(workflows.workflows.filter((w) => w.source === 'element')));
const forms = await readFile(join(dir, 'extracts/forms.jsonl'), 'utf8');
assert('form row captured', forms.includes('"action":"https://example.com/signup"') && forms.includes('"email"'));
assert('resources.jsonl exists', existsSync(join(dir, 'extracts/resources.jsonl')));
const resources = (await readFile(join(dir, 'extracts/resources.jsonl'), 'utf8')).trim().split('\n').filter(Boolean).map((l) => JSON.parse(l));
assert('script resource captured', resources.some((r) => r.kind === 'script' && r.src.endsWith('/app.js')), JSON.stringify(resources.filter((r) => r.kind === 'script')));
assert('stylesheet resource captured', resources.some((r) => r.kind === 'stylesheet' && r.src.endsWith('/styles/main.css')));
assert('feed resource captured', resources.some((r) => r.kind === 'feed' && r.src.endsWith('/feed.xml')));
assert('image resource captured', resources.some((r) => r.kind === 'image' && r.src.endsWith('/hero.png')));
assert('agent index reports resource total', agent.totals.resources === resources.length, JSON.stringify(agent.totals));
const schema = spawnSync(process.execPath, [schemaScript, '--intent', 'extract pricing and features'], { cwd: root, encoding: 'utf8' });
assert('schema helper pricing', schema.status === 0 && schema.stdout.includes('plan_name') && schema.stdout.includes('features'));
const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
