#!/usr/bin/env node
import { resolve, join } from 'node:path';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { readJson as readJsonFile, readJsonl as readJsonlFile } from './lib/bridge.mjs';

function usage(code = 2) {
  console.error('Usage: corpus-find.mjs --session-dir <dir> --query <text> [--limit <positive integer>] [--offset <non-negative integer>]');
  process.exit(code);
}
function invalid(message) {
  console.log(JSON.stringify({ ok: false, error: { code: 'invalidArguments', message } }));
  process.exit(2);
}
const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) usage(0);
const options = new Map();
const flags = new Set(['--session-dir', '--query', '--limit', '--offset']);
for (let index = 0; index < args.length; index += 2) {
  const flag = args[index];
  if (!flags.has(flag)) invalid(`Unknown option: ${flag}`);
  if (options.has(flag)) invalid(`Duplicate option: ${flag}`);
  const value = args[index + 1];
  if (value === undefined || value.startsWith('--')) invalid(`Missing value for ${flag}`);
  options.set(flag, value);
}
function integerOption(flag, fallback, minimum) {
  const raw = options.get(flag) ?? String(fallback);
  const value = Number(raw);
  if (!/^\d+$/.test(raw) || !Number.isSafeInteger(value) || value < minimum) {
    invalid(`${flag} must be a ${minimum === 0 ? 'non-negative' : 'positive'} safe integer`);
  }
  return value;
}
const sessionDir = options.get('--session-dir');
const query = (options.get('--query') ?? '').trim();
if (!sessionDir?.trim() || !query) invalid('--session-dir and a non-empty --query are required');
const dir = resolve(sessionDir);
const limit = integerOption('--limit', 20, 1);
const offset = integerOption('--offset', 0, 0);
const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
const readJson = (rel, fallback = null) => readJsonFile(dir, rel, fallback);
const readJsonl = (rel) => readJsonlFile(dir, rel);
function scoreText(value) {
  const s = String(value || '').toLowerCase();
  return terms.reduce((n, t) => n + (s.includes(t) ? 1 : 0), 0);
}
function scoreLabel(label, url = '') {
  const text = String(label || '').toLowerCase();
  const href = String(url || '').toLowerCase();
  let score = scoreText(`${text} ${href}`);
  for (const t of terms) {
    if (text === t) score += 4;
    else if (text.split(/\s+/).includes(t)) score += 2;
    if (href.endsWith(`/${t}`) || href.includes(`/${t}/`)) score += 2;
  }
  return score;
}
const agent = await readJson('AGENT_INDEX.json', {});
const graph = await readJson('graph/site-graph.json', { pages: [], edges: [] });

// Body-text scan with document-frequency damping: nav/sidebar text repeats on
// every page and must not dominate ranking, while a term that appears in only
// a few page bodies is a strong signal even when titles/links miss it.
const TEXT_SCAN_CAP = 65536;
const sourceRows = await readJsonl('sources.jsonl');
const pageTexts = new Map();
for (const row of sourceRows) {
  const rel = Array.isArray(row.textParts) ? row.textParts[0] : null;
  if (!row.pageId || !rel) continue;
  try {
    const body = (await readFile(join(dir, rel), 'utf8')).slice(0, TEXT_SCAN_CAP).toLowerCase();
    if (body.trim()) pageTexts.set(row.pageId, { rel, body, url: row.url });
  } catch {}
}
const docFrequency = new Map(terms.map((t) => [t, 0]));
for (const { body } of pageTexts.values()) {
  for (const t of terms) if (body.includes(t)) docFrequency.set(t, docFrequency.get(t) + 1);
}
const totalTexts = Math.max(1, pageTexts.size);
const termWeight = (t) => {
  const df = docFrequency.get(t) || 0;
  return df === 0 ? 0 : Math.log2(1 + totalTexts / df);
};
function countOccurrences(haystack, needle, cap = 32) {
  let count = 0, at = 0;
  while (count < cap) {
    at = haystack.indexOf(needle, at);
    if (at === -1) break;
    count += 1;
    at += needle.length;
  }
  return count;
}
// Occurrence-frequency weighting: a page that uses a term throughout its body
// outranks a page whose only hit is a nav/sidebar mention.
function textScore(pageId) {
  const rec = pageTexts.get(pageId);
  if (!rec) return 0;
  let score = 0;
  for (const t of terms) {
    const hits = countOccurrences(rec.body, t);
    if (hits > 0) score += termWeight(t) * (1 + Math.log2(hits));
  }
  return score;
}
const automationGraph = await readJson('graph/graph.json', { nodes: [], edges: [] });
const workflows = await readJson('graph/workflows.json', { workflows: [] });
const topLinks = await readJsonl('indexes/top-links.jsonl');
const elements = await readJsonl('extracts/elements.jsonl');
const resources = await readJsonl('extracts/resources.jsonl');
const candidates = [];
const graphPageIds = new Set();
for (const p of graph.pages || []) {
  graphPageIds.add(p.pageId);
  candidates.push({ type: 'page', score: scoreText(`${p.title} ${p.url} ${JSON.stringify(p.headingOutline || [])}`) + textScore(p.pageId), textScore: textScore(p.pageId) || undefined, pageId: p.pageId, title: p.title, url: p.url, files: (agent.pages || []).find((x) => x.pageId === p.pageId)?.files });
}
// Pages absent from the graph (e.g. plain-text payloads stored as one heading)
// are still findable through their body text.
for (const [pageId, rec] of pageTexts) {
  if (graphPageIds.has(pageId)) continue;
  const score = textScore(pageId);
  if (score > 0) candidates.push({ type: 'page', score, textScore: score, pageId, title: null, url: rec.url, files: { textParts: [rec.rel] } });
}
for (const l of topLinks) candidates.push({ type: 'link', score: scoreLabel(l.text, l.href) + scoreText(l.workflowType || '') + (l.score || 0) / 10, pageId: l.pageId, text: l.text, href: l.href, workflowType: l.workflowType || null });
for (const n of automationGraph.nodes || []) candidates.push({ type: `graph:${n.kind}`, score: scoreLabel(n.text || n.title || n.kind, n.url) + scoreText((n.workflowTypes || []).join(' ')), pageId: n.pageId, nodeId: n.id, url: n.url, text: n.text || n.title || null, workflowTypes: n.workflowTypes || [], risk: n.risk || null, evidence: [n.source || { file: 'graph/graph.json' }] });
for (const e of automationGraph.edges || []) candidates.push({ type: `edge:${e.kind}`, score: scoreLabel(e.label || e.kind, JSON.stringify(e.source || {})) + scoreText(e.workflowType || ''), edgeKind: e.kind, from: e.from, to: e.to, label: e.label || null, workflowType: e.workflowType || null, risk: e.risk || null, evidence: [e.source || { file: 'graph/graph.json' }] });
for (const w of workflows.workflows || []) candidates.push({ type: 'workflow', score: scoreText(`${w.workflowType} ${w.label} ${w.entryUrl}`) + (w.confidence === 'high' ? 1 : 0), workflowType: w.workflowType, label: w.label, entryUrl: w.entryUrl, evidence: w.evidence });
for (const e of elements) candidates.push({ type: 'element', score: scoreText(JSON.stringify(e)), pageId: e.pageId, kind: e.kind || e._file, workflowHint: e.workflowHint || null, preview: JSON.stringify(e).slice(0, 500) });
for (const r of resources) candidates.push({ type: 'resource', score: scoreText(`${r.kind} ${r.src}`), pageId: r.pageId, kind: r.kind, src: r.src });
const ranked = candidates.filter((c) => c.score > 0).sort((a, b) => b.score - a.score);
const matches = ranked.slice(offset, offset + limit);
const remainingMatches = Math.max(0, ranked.length - offset - matches.length);
const hasMore = remainingMatches > 0;
console.log(JSON.stringify({
  ok: true,
  sessionDir: dir,
  query,
  matches,
  isPartial: hasMore,
  completeness: hasMore ? 'partial' : 'complete',
  pagination: { offset, limit, totalMatches: ranked.length, returnedMatches: matches.length, remainingMatches, hasMore },
  next: hasMore ? {
    page: {
      command: process.execPath,
      args: [fileURLToPath(import.meta.url), '--session-dir', dir, '--query', query, '--limit', String(limit), '--offset', String(offset + matches.length)],
    },
  } : null,
  suggestedFiles: [...new Set(matches.map((m) => m.files?.textParts?.[0] || m.evidence?.[0]?.file || 'graph/site-graph.json'))].slice(0, 5),
}, null, 2));
