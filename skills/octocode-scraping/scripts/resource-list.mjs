#!/usr/bin/env node
import { classifyLink } from './lib/analyzers.mjs';
import { paginate } from './lib/pagination.mjs';
import { fileURLToPath } from 'node:url';
import { resolve, join } from 'node:path';
import { readJson, readJsonl, takeArg, requireSession } from './lib/bridge.mjs';

function usage(code = 2) {
  console.error('Usage: resource-list.mjs --session-dir <dir> [--kind internal|external|asset|contact|subdomain|script|stylesheet|image|media|feed] [--workflow <type>] [--host <host>] [--min-score <n>] [--query <text>] [--limit <n>] [--view <list>] [--cursor-<list> <n>]');
  process.exit(code);
}
const args = process.argv.slice(2);
const take = (flag) => takeArg(args, flag);
if (args.includes('--help') || args.includes('-h')) usage(0);
const sessionDir = take('--session-dir');
if (!sessionDir) usage();
const dir = resolve(sessionDir);
requireSession(dir);
const kind = take('--kind');
const workflow = take('--workflow');
const host = take('--host');
const query = (take('--query') || '').toLowerCase();
const minScore = Number(take('--min-score') || 0);
const limit = Number(take('--limit') || 100);
const graph = await readJson(dir, 'graph/site-graph.json', { pages: [], edges: [] });
const rootUrl = graph.rootUrl || (await readJsonl(dir, 'sources.jsonl'))[0]?.url;
let rows = (await readJsonl(dir, 'extracts/links.jsonl')).map(row => classifyLink(row, rootUrl));
if (!rows.length) rows = await readJsonl(dir, 'indexes/top-links.jsonl');
if (!rows.length) rows = (graph.pages || []).flatMap((p) => (p.topLinks || []).map((l) => ({ pageId: p.pageId, pageUrl: p.url, ...l })));
// Page assets (scripts, stylesheets, images, media, feeds) come from the resource extract, not from links.
for (const r of await readJsonl(dir, 'extracts/resources.jsonl')) {
  let assetHost = null;
  try { assetHost = new URL(r.src).hostname; } catch { /* not a URL: no host */ }
  rows.push({ pageId: r.pageId, href: r.src, host: assetHost, kind: r.kind, source: 'resource' });
}
if (kind) rows = rows.filter((r) => r.kind === kind);
if (workflow) rows = rows.filter((r) => r.workflowType === workflow || r.workflowHint === workflow);
if (host) rows = rows.filter((r) => r.host === host);
if (Number.isFinite(minScore) && minScore > 0) rows = rows.filter((r) => Number(r.score || 0) >= minScore);
// Match values only; key names such as "source" are not content.
if (query) rows = rows.filter((r) => [r.text, r.href, r.host, r.path, r.kind, r.workflowType, r.workflowHint].filter(Boolean).join(' ').toLowerCase().includes(query));
rows = rows.toSorted((a, b) => (b.score || 0) - (a.score || 0));
const byWorkflow = {};
for (const r of rows) if (r.workflowType) byWorkflow[r.workflowType] = (byWorkflow[r.workflowType] || 0) + 1;
const byKind = {};
for (const r of rows) byKind[r.kind || 'unknown'] = (byKind[r.kind || 'unknown'] || 0) + 1;
const paging = await paginate({ lists: { resources: rows }, files: ["graph/site-graph.json", "indexes/top-links.jsonl", "extracts/links.jsonl", "extracts/resources.jsonl"].map(p => join(dir, p)), dir, args, script: fileURLToPath(import.meta.url), defaultLimit: limit });
console.log(JSON.stringify({ ok: true, sessionDir: dir, filters: { kind: kind || null, workflow: workflow || null, host: host || null, minScore, query: query || null }, totalMatches: rows.length, facets: { byKind, byWorkflow }, ...paging }, null, 2));
