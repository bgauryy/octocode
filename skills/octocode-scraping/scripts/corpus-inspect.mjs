#!/usr/bin/env node
import { existsSync } from 'node:fs';
import { paginate } from './lib/pagination.mjs';
import { fileURLToPath } from 'node:url';
import { resolve, join } from 'node:path';
import { readJson, readJsonl, takeArg, requireSession } from './lib/bridge.mjs';

function usage(code = 2) {
  console.error('Usage: corpus-inspect.mjs --session-dir <dir> [--workflow <type>] [--limit <n>] [--view <list>] [--cursor-<list> <n>]');
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
requireSession(dir);
const agent = await readJson(dir, 'AGENT_INDEX.json', {});
const roster = await readJson(dir, 'page-map.json', { pages: [] });
const workflows = await readJson(dir, 'graph/workflows.json', { workflows: [] });
const topLinks = await readJsonl(dir, 'indexes/top-links.jsonl');
const workflowRows = workflow ? workflows.workflows.filter((w) => w.workflowType === workflow) : workflows.workflows;
const paging = await paginate({ lists: { pageRows: roster.pages || [], workflows: workflowRows, topLinks, warnings: agent.warnings || [] }, files: ["AGENT_INDEX.json", "page-map.json", "graph/workflows.json", "indexes/top-links.jsonl"].map(p => join(dir, p)), dir, args, script: fileURLToPath(import.meta.url), defaultLimit: limit });
const out = {
  ok: Boolean(agent.sessionId),
  sessionDir: dir,
  sessionId: agent.sessionId || null,
  totals: agent.totals || {},
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
