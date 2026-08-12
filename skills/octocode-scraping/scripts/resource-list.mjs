#!/usr/bin/env node
import { resolve } from 'node:path';
import { readJson as readJsonFile, readJsonl as readJsonlFile, takeArg } from './lib/bridge.mjs';

function usage(code = 2) {
  console.error('Usage: resource-list.mjs --session-dir <dir> [--kind internal|external|asset|contact|subdomain] [--workflow <type>] [--host <host>] [--min-score <n>] [--query <text>] [--limit <n>]');
  process.exit(code);
}
const args = process.argv.slice(2);
const take = (flag) => takeArg(args, flag);
if (args.includes('--help') || args.includes('-h')) usage(0);
const sessionDir = take('--session-dir');
if (!sessionDir) usage();
const dir = resolve(sessionDir);
const kind = take('--kind');
const workflow = take('--workflow');
const host = take('--host');
const query = take('--query').toLowerCase();
const minScore = Number(take('--min-score') || 0);
const limit = Number(take('--limit') || 100);
const readJson = (rel, fallback = null) => readJsonFile(dir, rel, fallback);
const readJsonl = (rel) => readJsonlFile(dir, rel);
const graph = await readJson('graph/site-graph.json', { pages: [], edges: [] });
let rows = await readJsonl('indexes/top-links.jsonl');
if (!rows.length) rows = (graph.pages || []).flatMap((p) => (p.topLinks || []).map((l) => ({ pageId: p.pageId, pageUrl: p.url, ...l })));
if (kind) rows = rows.filter((r) => r.kind === kind);
if (workflow) rows = rows.filter((r) => r.workflowType === workflow || r.workflowHint === workflow);
if (host) rows = rows.filter((r) => r.host === host);
if (Number.isFinite(minScore) && minScore > 0) rows = rows.filter((r) => Number(r.score || 0) >= minScore);
if (query) rows = rows.filter((r) => JSON.stringify(r).toLowerCase().includes(query));
rows = rows.sort((a, b) => (b.score || 0) - (a.score || 0));
const byWorkflow = {};
for (const r of rows) if (r.workflowType) byWorkflow[r.workflowType] = (byWorkflow[r.workflowType] || 0) + 1;
const byKind = {};
for (const r of rows) byKind[r.kind || 'unknown'] = (byKind[r.kind || 'unknown'] || 0) + 1;
console.log(JSON.stringify({ ok: true, sessionDir: dir, filters: { kind: kind || null, workflow: workflow || null, host: host || null, minScore, query: query || null }, totalMatches: rows.length, facets: { byKind, byWorkflow }, resources: rows.slice(0, limit) }, null, 2));
