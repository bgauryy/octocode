#!/usr/bin/env node
import { resolve } from 'node:path';
import { readJson as readJsonFile, readJsonl as readJsonlFile, takeArg } from './lib/cli.mjs';

function usage(code = 2) {
  console.error('Usage: corpus-find.mjs --session-dir <dir> --query <text> [--limit <n>]');
  process.exit(code);
}
const args = process.argv.slice(2);
const take = (flag) => takeArg(args, flag);
if (args.includes('--help') || args.includes('-h')) usage(0);
const sessionDir = take('--session-dir');
const query = take('--query').trim();
if (!sessionDir || !query) usage();
const dir = resolve(sessionDir);
const limit = Number(take('--limit') || 20);
const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
const readJson = (rel, fallback = null) => readJsonFile(dir, rel, fallback);
const readJsonl = (rel) => readJsonlFile(dir, rel);
function scoreText(value) {
  const s = String(value || '').toLowerCase();
  return terms.reduce((n, t) => n + (s.includes(t) ? 1 : 0), 0);
}
const agent = await readJson('AGENT_INDEX.json', {});
const graph = await readJson('graph/site-graph.json', { pages: [], edges: [] });
const workflows = await readJson('graph/workflows.json', { workflows: [] });
const topLinks = await readJsonl('indexes/top-links.jsonl');
const elements = await readJsonl('extracts/elements.jsonl');
const resources = await readJsonl('extracts/resources.jsonl');
const candidates = [];
for (const p of graph.pages || []) candidates.push({ type: 'page', score: scoreText(`${p.title} ${p.url} ${JSON.stringify(p.headingOutline || [])}`), pageId: p.pageId, title: p.title, url: p.url, files: (agent.pages || []).find((x) => x.pageId === p.pageId)?.files });
for (const l of topLinks) candidates.push({ type: 'link', score: scoreText(`${l.text} ${l.href} ${l.workflowType || ''}`) + (l.score || 0) / 10, pageId: l.pageId, text: l.text, href: l.href, workflowType: l.workflowType || null });
for (const w of workflows.workflows || []) candidates.push({ type: 'workflow', score: scoreText(`${w.workflowType} ${w.label} ${w.entryUrl}`) + (w.confidence === 'high' ? 1 : 0), workflowType: w.workflowType, label: w.label, entryUrl: w.entryUrl, evidence: w.evidence });
for (const e of elements) candidates.push({ type: 'element', score: scoreText(JSON.stringify(e)), pageId: e.pageId, kind: e.kind || e._file, workflowHint: e.workflowHint || null, preview: JSON.stringify(e).slice(0, 500) });
for (const r of resources) candidates.push({ type: 'resource', score: scoreText(`${r.kind} ${r.src}`), pageId: r.pageId, kind: r.kind, src: r.src });
const matches = candidates.filter((c) => c.score > 0).sort((a, b) => b.score - a.score).slice(0, limit);
console.log(JSON.stringify({ ok: true, sessionDir: dir, query, matches, next: matches.slice(0, 5).map((m) => m.files?.textParts?.[0] || m.evidence?.[0]?.file || 'graph/site-graph.json') }, null, 2));
