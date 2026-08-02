#!/usr/bin/env node
import { resolve } from 'node:path';
import { readJson as readJsonFile, readJsonl as readJsonlFile, takeArg } from './lib/cli.mjs';

function usage(code = 2) {
  console.error('Usage: dom-find.mjs --session-dir <dir> [--kind form|button|table|meta|jsonld|canonical|code-block] [--workflow <type>] [--page-id <id>] [--query <text>] [--limit <n>]');
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
const pageId = take('--page-id');
const query = take('--query').toLowerCase();
const limit = Number(take('--limit') || 50);
const readJsonl = (rel) => readJsonlFile(dir, rel);
const readJson = (rel, fallback = null) => readJsonFile(dir, rel, fallback);
let rows = await readJsonl('extracts/elements.jsonl');
if (!rows.length) {
  const files = ['meta', 'canonical', 'jsonld', 'forms', 'buttons', 'tables', 'code-blocks'];
  rows = (await Promise.all(files.map(async (f) => (await readJsonl(`extracts/${f}.jsonl`)).map((r) => ({ ...r, _file: f }))))).flat();
}
if (kind) rows = rows.filter((r) => r.kind === kind || r._file === kind);
if (workflow) rows = rows.filter((r) => r.workflowHint === workflow);
if (pageId) rows = rows.filter((r) => r.pageId === pageId);
if (query) rows = rows.filter((r) => JSON.stringify(r).toLowerCase().includes(query));
const agent = await readJson('AGENT_INDEX.json', {});
const pagesById = new Map((agent.pages || []).map((p) => [p.pageId, p]));
const matches = rows.slice(0, limit).map((r) => ({ ...r, pageUrl: pagesById.get(r.pageId)?.url || null, evidenceFile: `extracts/${r._file || 'elements'}.jsonl`, textEvidence: pagesById.get(r.pageId)?.files?.textParts?.[0] || null }));
console.log(JSON.stringify({ ok: true, sessionDir: dir, filters: { kind: kind || null, workflow: workflow || null, pageId: pageId || null, query: query || null }, totalMatches: rows.length, matches }, null, 2));
