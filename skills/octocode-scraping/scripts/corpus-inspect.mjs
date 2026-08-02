#!/usr/bin/env node
import { existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { readJson as readJsonFile, readJsonl as readJsonlFile, takeArg } from './lib/cli.mjs';

function usage(code = 2) {
  console.error('Usage: corpus-inspect.mjs --session-dir <dir> [--page <n>] [--workflow <type>] [--limit <n>]');
  process.exit(code);
}
const args = process.argv.slice(2);
const take = (flag) => takeArg(args, flag);
if (args.includes('--help') || args.includes('-h')) usage(0);
const sessionDir = take('--session-dir');
if (!sessionDir) usage();
const dir = resolve(sessionDir);
const limit = Number(take('--limit') || 20);
const page = Number(take('--page') || 1);
const workflow = take('--workflow');
const readJson = (rel, fallback = null) => readJsonFile(dir, rel, fallback);
const readJsonl = (rel) => readJsonlFile(dir, rel);
const agent = await readJson('AGENT_INDEX.json', {});
const pageIndex = await readJson(`indexes/pages-${String(page).padStart(3, '0')}.json`, { rows: [] });
const workflows = await readJson('graph/workflows.json', { workflows: [] });
const topLinks = await readJsonl('indexes/top-links.jsonl');
const workflowRows = workflow ? workflows.workflows.filter((w) => w.workflowType === workflow).slice(0, limit) : workflows.workflows.slice(0, limit);
const out = {
  ok: Boolean(agent.sessionId),
  sessionDir: dir,
  sessionId: agent.sessionId || null,
  warnings: agent.warnings || [],
  totals: agent.totals || {},
  page: pageIndex.page || page,
  pageRows: (pageIndex.rows || []).slice(0, limit),
  workflows: workflowRows,
  topLinks: topLinks.slice(0, limit),
  nextRead: [
    'AGENT_INDEX.json',
    `indexes/pages-${String(page).padStart(3, '0')}.json`,
    'graph/site-graph.json',
    'graph/workflows.json',
    'indexes/top-links.jsonl',
    'indexes/workflow-candidates.jsonl'
  ].filter((rel) => existsSync(join(dir, rel))),
  rawAudit: existsSync(join(dir, 'raw')) ? 'raw/' : null
};
console.log(JSON.stringify(out, null, 2));
process.exit(out.ok ? 0 : 1);
