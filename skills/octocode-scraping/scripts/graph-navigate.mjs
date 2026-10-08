#!/usr/bin/env node
import { paginate } from './lib/pagination.mjs';
import { fileURLToPath } from 'node:url';
import { resolve, join } from 'node:path';
import { readJson as readJsonFile, takeArg } from './lib/bridge.mjs';

function usage(code = 2) {
  console.error('Usage: graph-navigate.mjs --session-dir <dir> [--from <nodeId>] [--kind <edgeKind>] [--workflow <type>] [--risk <risk>] [--limit <n>] [--view <list>] [--cursor-<list> <n>] [--no-dedupe]');
  process.exit(code);
}
const args = process.argv.slice(2);
const take = (flag) => takeArg(args, flag);
if (args.includes('--help') || args.includes('-h')) usage(0);
const sessionDir = take('--session-dir');
if (!sessionDir) usage();
const dir = resolve(sessionDir);
const from = take('--from');
const kind = take('--kind');
const workflow = take('--workflow');
const risk = take('--risk');
const dedupe = args.includes('--dedupe');
const limit = Number(take('--limit') || 50);
const graph = await readJsonFile(dir, 'graph/graph.json', { nodes: [], edges: [], totals: {} });
const nodeById = new Map((graph.nodes || []).map((n) => [n.id, n]));
let edges = graph.edges || [];
if (from) edges = edges.filter((e) => e.from === from);
if (kind) edges = edges.filter((e) => e.kind === kind);
if (workflow) edges = edges.filter((e) => e.workflowType === workflow);
if (risk) edges = edges.filter((e) => e.risk === risk);
const sortedEdges = edges.sort((a, b) => (b.score || 0) - (a.score || 0));
const selectedEdges = [];
const seen = new Set();
for (const edge of sortedEdges) {
  const target = nodeById.get(edge.to) || {};
  const key = `${edge.kind}:${target.url || edge.to}:${edge.label || ''}:${edge.workflowType || ''}`;
  if (dedupe && seen.has(key)) continue;
  seen.add(key);
  selectedEdges.push(edge);

}
const routes = selectedEdges.map((e) => {
  const source = nodeById.get(e.from) || {};
  const target = nodeById.get(e.to) || {};
  return {
    from: e.from,
    fromKind: source.kind || null,
    fromUrl: source.url || null,
    to: e.to,
    toKind: target.kind || null,
    toUrl: target.url || null,
    edgeKind: e.kind,
    label: e.label || null,
    workflowType: e.workflowType || null,
    risk: e.risk || null,
    score: e.score || 0,
    confidence: e.confidence ?? null,
    evidence: e.source || null
  };
});
const actionNodes = (graph.nodes || []).filter((n) => ['form', 'input', 'button', 'table', 'pagination'].includes(n.kind));
const paging = await paginate({ lists: { routes, actionNodes }, files: ["graph/graph.json"].map(p => join(dir, p)), dir, args, script: fileURLToPath(import.meta.url), defaultLimit: limit });
console.log(JSON.stringify({ ok: true, sessionDir: dir, filters: { from: from || null, kind: kind || null, workflow: workflow || null, risk: risk || null, dedupe }, totals: graph.totals || {}, ...paging, sources: ['graph/graph.json', 'graph/site-graph.json', 'graph/workflows.json', 'indexes/top-links.jsonl', 'indexes/workflow-candidates.jsonl'] }, null, 2));
