#!/usr/bin/env node
import { resolve, join } from 'node:path';
import { createReadStream } from 'node:fs';
import { paginate } from './lib/pagination.mjs';
import { fileURLToPath } from 'node:url';
import { readJson, readJsonl, requireSession } from './lib/bridge.mjs';
import { fail } from './lib/fail.mjs';

function usage(code = 2) {
  console.error('Usage: corpus-find.mjs --session-dir <dir> --query <text> [--limit <positive integer>] [--offset <non-negative integer>]');
  process.exit(code);
}
function invalid(message) {
  fail('invalidArguments', message, 2);
}
const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) usage(0);
const options = new Map();
const flags = new Set(['--session-dir', '--query', '--limit', '--offset', '--snapshot']);
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
if (limit > 300) invalid('--limit must be <= 300');
const offset = integerOption('--offset', 0, 0);
requireSession(dir);
const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
function scoreText(value) {
  const s = String(value || '').toLowerCase();
  return terms.reduce((n, t) => n + (s.includes(t) ? 1 : 0), 0);
}
function scoreLabel(label, url) {
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
// Counts the term hits in `body` that end past the carried-over `fresh` prefix (capped at 32); true when any did.
function countTerm(body, term, fresh, counts) {
  let at = 0, hit, found = false;
  while ((hit = body.indexOf(term, at)) !== -1) {
    if (hit + term.length > fresh) { counts.set(term, Math.min(32, counts.get(term) + 1)); found = true; }
    at = hit + term.length;
  }
  return found;
}
// Streams one body file, carrying `tail` across chunk and file boundaries so split terms still count.
async function scanBodyFile(file, counts, tail, overlap) {
  let matched = false;
  for await (const chunk of createReadStream(file, {encoding:'utf8',highWaterMark:65536})) {
    const body = tail + chunk.toLowerCase();
    for (const term of terms) if (countTerm(body, term, tail.length, counts)) matched = true;
    tail = overlap ? body.slice(-overlap) : '';
  }
  return { tail, matched };
}
const agent = await readJson(dir, 'AGENT_INDEX.json', {});
const graph = await readJson(dir, 'graph/site-graph.json', { pages: [], edges: [] });

// Body-text scan with document-frequency damping: nav/sidebar text repeats on
// every page and must not dominate ranking, while a term that appears in only
// a few page bodies is a strong signal even when titles/links miss it.
const sourceRows = await readJsonl(dir, 'sources.jsonl');
const pageTexts = new Map(), bodyFiles = [];
for (const row of sourceRows) {
  const parts = Array.isArray(row.textParts) ? row.textParts : [];
  if (!row.pageId || !parts.length) continue;
  const counts = new Map(terms.map(term => [term, 0])); let firstMatch = null, tail = '';
  const overlap = Math.max(...terms.map(t => t.length)) - 1;
  for (const rel of parts) {
    const file = join(dir, rel); bodyFiles.push(file);
    try {
      const scanned = await scanBodyFile(file, counts, tail, overlap);
      tail = scanned.tail;
      if (scanned.matched) firstMatch ??= rel;
    } catch (error) { invalid('Cannot scan source '+file+': '+error.message); }
  }
  pageTexts.set(row.pageId, {rel:firstMatch || parts[0], counts, url:row.url});
}
const docFrequency = new Map(terms.map((t) => [t, 0]));
for (const { counts } of pageTexts.values()) {
  for (const t of terms) if (counts.get(t) > 0) docFrequency.set(t, docFrequency.get(t) + 1);
}
const totalTexts = Math.max(1, pageTexts.size);
const termWeight = (t) => {
  const df = docFrequency.get(t) || 0;
  return df === 0 ? 0 : Math.log2(1 + totalTexts / df);
};
// Occurrence-frequency weighting: a page that uses a term throughout its body
// outranks a page whose only hit is a nav/sidebar mention.
function textScore(pageId) {
  const rec = pageTexts.get(pageId);
  if (!rec) return 0;
  let score = 0;
  for (const t of terms) {
    const hits = rec.counts.get(t) || 0;
    if (hits > 0) score += termWeight(t) * (1 + Math.log2(hits));
  }
  return score;
}
// Score values, never serialized rows: JSON key names such as "source" or "pageId" are not content.
function leafText(value) {
  if (value == null) return [];
  if (typeof value !== 'object') return [String(value)];
  return (Array.isArray(value) ? value : Object.values(value)).flatMap(leafText);
}
function elementText(e) {
  const inputs = (e.inputs || []).flatMap((i) => [i.name, i.placeholder, i.type]);
  return [e.text, e.preview, e.textPreview, e.action, e.href, e.key, e.content, e.language, e.type, e.kind, e.workflowHint, ...(e.labels || []), ...(e.headers || []), ...inputs, ...leafText(e.json)].filter(Boolean).join(' ');
}
// The capped AGENT_INDEX lists only the first pages; the roster in page-map.json lists them all.
const roster = await readJson(dir, 'page-map.json', { pages: [] });
const filesByPage = new Map([...(agent.pages || []), ...(roster.pages || [])].filter((p) => p?.pageId && p.files).map((p) => [p.pageId, p.files]));
const automationGraph = await readJson(dir, 'graph/graph.json', { nodes: [], edges: [] });
const workflows = await readJson(dir, 'graph/workflows.json', { workflows: [] });
const topLinks = await readJsonl(dir, 'indexes/top-links.jsonl');
const elements = await readJsonl(dir, 'extracts/elements.jsonl');
const resources = await readJsonl(dir, 'extracts/resources.jsonl');
const candidates = [];
const graphPageIds = new Set();
for (const p of graph.pages || []) {
  graphPageIds.add(p.pageId);
  candidates.push({ type: 'page', score: scoreText(`${p.title} ${p.url} ${(p.headingOutline || []).map((h) => h?.text ?? h).join(' ')}`) + textScore(p.pageId), textScore: textScore(p.pageId) || undefined, pageId: p.pageId, title: p.title, url: p.url, files: filesByPage.get(p.pageId) });
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
for (const e of automationGraph.edges || []) candidates.push({ type: `edge:${e.kind}`, score: scoreLabel(e.label || e.kind, '') + scoreText(e.workflowType || ''), edgeKind: e.kind, from: e.from, to: e.to, label: e.label || null, workflowType: e.workflowType || null, risk: e.risk || null, evidence: [e.source || { file: 'graph/graph.json' }] });
for (const w of workflows.workflows || []) candidates.push({ type: 'workflow', score: scoreText(`${w.workflowType} ${w.label} ${w.entryUrl}`) + (w.confidence === 'high' ? 1 : 0), workflowType: w.workflowType, label: w.label, entryUrl: w.entryUrl, evidence: w.evidence });
for (const e of elements) candidates.push({ type: 'element', score: scoreText(elementText(e)), pageId: e.pageId, kind: e.kind || e._file, workflowHint: e.workflowHint || null, element: e });
for (const r of resources) candidates.push({ type: 'resource', score: scoreText(`${r.kind} ${r.src}`), pageId: r.pageId, kind: r.kind, src: r.src });
const ranked = candidates.filter((c) => c.score > 0).toSorted((a, b) => b.score - a.score);
const paging = await paginate({lists:{matches:ranked},files:['AGENT_INDEX.json','sources.jsonl','graph/site-graph.json','graph/graph.json','graph/workflows.json','indexes/top-links.jsonl','extracts/elements.jsonl','extracts/resources.jsonl'].map(p=>join(dir,p)).concat(bodyFiles),dir,args:['--limit',String(limit),'--cursor-matches',String(Math.min(offset,ranked.length)),...(options.get('--snapshot')?['--snapshot',options.get('--snapshot')]:[])],script:fileURLToPath(import.meta.url)});
const matches = paging.matches;
const remainingMatches = Math.max(0, ranked.length - offset - matches.length);
const hasMore = remainingMatches > 0;
console.log(JSON.stringify({
  ok: true,
  sessionDir: dir,
  query,
  snapshot: paging.snapshot,
  matches,
  isPartial: hasMore,
  completeness: hasMore ? 'partial' : 'complete',
  pagination: { offset, limit, totalMatches: ranked.length, returnedMatches: matches.length, remainingMatches, hasMore },
  next: hasMore ? {
    page: {
      command: process.execPath,
      args: [fileURLToPath(import.meta.url), '--session-dir', dir, '--query', query, '--limit', String(limit), '--offset', String(offset + matches.length), '--snapshot', paging.snapshot],
    },
  } : null,
  suggestedFiles: [...new Set(matches.map((m) => m.files?.textParts?.[0] || m.evidence?.[0]?.file || 'graph/site-graph.json'))].slice(0, 5),
}, null, 2));
