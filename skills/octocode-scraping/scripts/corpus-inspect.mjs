#!/usr/bin/env node
import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { paginate } from './lib/pagination.mjs';
import { fileURLToPath } from 'node:url';
import { resolve, join } from 'node:path';
import { readJson, readJsonl, takeArg, requireSession, listFilesRecursive, redactUrl, redactSecretsInText } from './lib/bridge.mjs';
import { fail } from './lib/fail.mjs';

function usage(code = 2) {
  console.error(`Usage: corpus-inspect.mjs --session-dir <dir> [--workflow <type>] [--limit <n>] [--view <list>] [--cursor-<list> <n>]
       corpus-inspect.mjs --session-dir <dir> --view captures [--limit <n>]
       corpus-inspect.mjs --session-dir <dir> --format coverage-md
Default: JSON with pageRows, workflows, topLinks and warnings, each paged; run next.<list> unchanged until it is absent.
--view captures: JSON rows {file, bytes, sha256, empty, duplicateOf} for every saved capture (text/*.clean*.md, cdp/,
  snippets/, extracts/cdp-*.jsonl); empty means no non-whitespace byte, duplicateOf names the first file with the same sha256.
--format coverage-md: prints Markdown, not JSON: a count line and the | URL | Status | Session file | Notes | table, one row
  per sources.jsonl row plus each URL the latest fetch refused or skipped for robots (reports/failures.md). URLs and notes
  are redacted. Exit 1 when the session has no sources.jsonl row.`);
  process.exit(code);
}
const args = process.argv.slice(2);
const take = (flag) => takeArg(args, flag);
if (args.includes('--help') || args.includes('-h')) usage(0);
const sessionDir = take('--session-dir');
if (!sessionDir) usage();
const dir = resolve(sessionDir);
const limit = Number(take('--limit') || 20);
const workflow = take('--workflow');
const format = take('--format');
if (format && format !== 'coverage-md') fail('invalidArguments', '--format must be coverage-md', 2);
requireSession(dir);

// One table cell: one line, pipes escaped, credentials masked.
const cell = (value) => redactSecretsInText(String(value ?? '')).replace(/\s+/g, ' ').trim().replaceAll('|', '\\|');

function coverageRow(source) {
  const status = `${source.status || 'no response'}${source.ok ? '' : ' failed'}`;
  const parts = source.textParts || [];
  const file = parts.length ? parts.join(', ') : source.text || source.raw || 'sources.jsonl';
  const notes = [
    source.error && `error: ${source.error}`,
    source.targetLikelyError && `target error: ${source.targetLikelyError}`,
    source.providerDetail && `provider: ${source.providerDetail}`,
    source.browserRecommended && `browser recommended: ${source.browserReason}`,
    source.networkTruncated && 'network cap reached',
    source.rawTruncated && 'raw truncated',
    source.textTruncated && 'text truncated',
    source.retryAfterMs && `retry after ${source.retryAfterMs} ms`,
    source.bridge && `bridge: ${source.bridge.type}`,
  ].filter(Boolean).join('; ');
  return [redactUrl(source.url), status, file, notes];
}

// The latest fetch run's refusals and robots skips, in the line format fetch.mjs writes.
async function notFetched() {
  const path = join(dir, 'reports/failures.md');
  if (!existsSync(path)) return [];
  const rows = [];
  for (const line of (await readFile(path, 'utf8')).split('\n')) {
    const refused = line.match(/^- refused (\S+): (.*)$/), robots = line.match(/^- robots\.txt skipped (\S+): (.*)$/);
    if (refused) rows.push([redactUrl(refused[1]), 'refused', 'reports/failures.md', refused[2]]);
    if (robots) rows.push([redactUrl(robots[1]), 'robots skipped', 'reports/failures.md', robots[2]]);
  }
  return rows;
}

if (format === 'coverage-md') {
  const sources = await readJsonl(dir, 'sources.jsonl');
  const fetched = sources.map(coverageRow), skipped = await notFetched();
  const okCount = sources.filter((s) => s.ok).length;
  const lines = [
    `${sources.length} fetched (${okCount} ok, ${sources.length - okCount} failed); ${skipped.length} not fetched (refused or robots).`,
    '',
    '| URL | Status | Session file | Notes |',
    '|---|---|---|---|',
    ...[...fetched, ...skipped].map((row) => `| ${row.map(cell).join(' | ')} |`),
  ];
  console.log(lines.join('\n'));
  process.exit(sources.length ? 0 : 1);
}

const CAPTURE = /^(?:text\/[^/]+\.clean(?:\.part-\d+)?\.md|cdp\/.+|snippets\/.+|extracts\/cdp-[^/]+\.jsonl)$/;
// Every saved capture with its size and hash, so empty and repeated captures are dropped by data, not by eye.
async function captureRows() {
  const files = (await listFilesRecursive(dir, { include: (rel) => CAPTURE.test(rel) })).toSorted((a, b) => a.rel.localeCompare(b.rel));
  const firstByHash = new Map();
  const rows = [];
  for (const file of files) {
    const bytes = await readFile(file.abs);
    const sha256 = createHash('sha256').update(bytes).digest('hex');
    rows.push({ file: file.rel, bytes: bytes.length, sha256, empty: !/\S/.test(bytes.toString('utf8')), duplicateOf: firstByHash.get(sha256) ?? null });
    if (!firstByHash.has(sha256)) firstByHash.set(sha256, file.rel);
  }
  return { rows, files: files.map((file) => file.abs) };
}

const agent = await readJson(dir, 'AGENT_INDEX.json', {});
const roster = await readJson(dir, 'page-map.json', { pages: [] });
const workflows = await readJson(dir, 'graph/workflows.json', { workflows: [] });
const topLinks = await readJsonl(dir, 'indexes/top-links.jsonl');
const workflowRows = workflow ? workflows.workflows.filter((w) => w.workflowType === workflow) : workflows.workflows;
const lists = { pageRows: roster.pages || [], workflows: workflowRows, topLinks, warnings: agent.warnings || [] };
const snapshotFiles = ['AGENT_INDEX.json', 'page-map.json', 'graph/workflows.json', 'indexes/top-links.jsonl'].map((p) => join(dir, p));
let captureSummary = null;
if (take('--view') === 'captures') {
  const captures = await captureRows();
  lists.captures = captures.rows;
  snapshotFiles.push(...captures.files);
  captureSummary = { files: captures.rows.length, empty: captures.rows.filter((row) => row.empty).length, duplicates: captures.rows.filter((row) => row.duplicateOf).length };
}
const paging = await paginate({ lists, files: snapshotFiles, dir, args, script: fileURLToPath(import.meta.url), defaultLimit: limit });
const out = {
  ok: Boolean(agent.sessionId),
  sessionDir: dir,
  sessionId: agent.sessionId || null,
  totals: agent.totals || {},
  ...(captureSummary ? { captureSummary } : {}),
  ...paging,
  nextRead: [
    'AGENT_INDEX.json',
    'indexes/pages-001.json',
    'graph/site-graph.json',
    'graph/workflows.json',
    'indexes/top-links.jsonl',
    'indexes/workflow-candidates.jsonl'
  ].filter((rel) => existsSync(join(dir, rel))),
  rawAudit: existsSync(join(dir, 'raw')) ? 'raw/' : null
};
console.log(JSON.stringify(out, null, 2));
process.exit(out.ok ? 0 : 1);
