#!/usr/bin/env node
/** Offline regression gate for documented tool contracts, not an agent benchmark. */
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const args = process.argv.slice(2);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
if (args.includes('--help')) {
  console.log('Usage: node scripts/check-guidance.mjs [--json] [--self-test]\nChecks local/external routing, completeness, and TDD guidance. No network or writes.');
  process.exit(0);
}
if (args.some(arg => !['--json', '--self-test'].includes(arg))) {
  console.error('Unknown option; use --help.');
  process.exit(2);
}

const cases = [
  { name: 'known anchors skip discovery', file: 'references/workflow-local.md',
    required: [/known (?:file|path|anchor)/i, /skip.*(?:discovery|orientation)/i] },
  { name: 'reachability verifies explicit or inferred roots', file: 'references/workflow-local.md',
    required: [/reachability[^\n]*optional[^\n]*entrypoints/i, /entrypointsResolved/, /unclassified/],
    forbidden: [/reachability[^\n]*required[^\n]*entrypoints/i] },
  { name: 'AST limits and rewrites are evidence', file: 'references/workflow-local.md',
    required: [/structural\.query\.rewritten/, /terminalLimit/, /(?:incomplete|partial)[^\n]*absence/i] },
  { name: 'LSP needs anchors and capability checks', file: 'references/workflow-local.md',
    required: [/uri[^\n]*symbolName[^\n]*lineHint/, /includeDeclaration:false/, /warmup/, /capabilit/i] },
  { name: 'graph coverage has independent diagnostics', file: 'references/workflow-local.md',
    required: [/diagnosticPage/, /unresolved[^\n]*CommonJS/, /rustWorkspace/, /syntactic/] },
  { name: 'npm intent and version provenance', file: 'references/workflow-external.md',
    required: [/packageName[^\n]*exact/i, /keywords[^\n]*discovery/i, /(?:version|release)[^\n]*(?:gitHead|tag|commit)/i] },
  { name: 'GitHub indexed search has explicit boundaries', file: 'references/workflow-external.md',
    required: [/code[^\n]*default branch/i, /1,000/, /(?:incomplete|partial)/i] },
  { name: 'file refs never silently substitute', file: 'references/workflow-external.md',
    required: [/ghGetFileContent[^\n]*explicit[^\n]*branch/i, /404[^\n]*(?:path|ref)/i],
    forbidden: [/fallback branch changes what was researched/i] },
  { name: 'history operations keep distinct identities', file: 'references/workflow-external.md',
    required: [/pullRequest[^\n]*issue[^\n]*number/, /commit[^\n]*ref/, /compare[^\n]*base[^\n]*head/, /omit[^\n]*keywords[^\n]*(?:path|branch)/i] },
  { name: 'materialization respects scoped completeness and storage', file: 'references/workflow-combination.md',
    required: [/complete[^\n]*(?:relative|requested scope)/i, /OCTOCODE_STORAGE_MODE/, /ENABLE_CLONE/, /shallow[^\n]*history/i],
    forbidden: [/complete:false/, /3rd\+|third read|3\+ remote reads/i] },
  { name: 'transport and continuation semantics', file: 'references/octocode.md',
    required: [/responsePagination/, /nested/, /hasMore[^\n]*false/, /status[^\n]*error/],
    forbidden: [/\$OCTO cache fetch/, /only `clone` and `cache`/, /10 tools are enabled by default/] },
  { name: 'portable CLI invocation', file: 'references/octocode.md',
    required: [/npx -y octocode/, /node packages\/octocode\/out\/octocode\.js tools/],
    forbidden: [/\$OCTO /] },
  { name: 'TDD and no compatibility scaffolding', file: 'references/workflow-change.md',
    required: [/RED[^\n]*GREEN[^\n]*REFACTOR/, /(?:fail|failing)[^\n]*before[^\n]*(?:patch|edit|implementation)/i, /(?:no|avoid)[^\n]*compatibility[^\n]*(?:unless|without)/i, /rebuild[^\n]*(?:CLI|MCP)/i] },
  { name: 'authorization persists and budgets do not abandon work', file: 'SKILL.md',
    required: [/authorization[^\n]*(?:persists|carry|already)/i, /checkpoint[^\n]*(?:budget|time)|budget[^\n]*checkpoint/i],
    forbidden: [/Ask before public\/broad contracts/, /third unrelated search space/] },
  { name: 'primary sources and untrusted content', file: 'references/workflow-external.md',
    required: [/primary[^\n]*(?:documentation|docs)/i, /untrusted[^\n]*(?:instructions|data)/i] },
  { name: 'one owner for adaptive routing', file: 'references/workflows.md',
    required: [/surface[^\n]*task/i, /skip[^\n]*(?:irrelevant|redundant|known)/i],
    forbidden: [/take exactly one/, /routes don't nest/, /graph for file topology → LSP/] },
];

const corpus = new Map();
for (const item of cases) {
  if (!corpus.has(item.file)) corpus.set(item.file, readFileSync(resolve(root, item.file), 'utf8'));
}
const accepts = (item, source) =>
  item.required.every(pattern => pattern.test(source)) &&
  (item.forbidden ?? []).every(pattern => !pattern.test(source));
const checks = cases.map(item => ({ name: item.name, file: item.file, pass: accepts(item, corpus.get(item.file)) }));
const selfChecks = args.includes('--self-test')
  ? cases.map(item => ({ name: `${item.name}: missing guidance rejected`, pass: !accepts(item, '') }))
  : [];
const all = [...checks, ...selfChecks];
const failed = all.filter(check => !check.pass);
const report = { pass: failed.length === 0, passed: all.length - failed.length, total: all.length, checks: all };
if (args.includes('--json')) console.log(JSON.stringify(report, null, 2));
else {
  console.log(`${report.pass ? 'PASS' : 'FAIL'} research-guidance ${report.passed}/${report.total}`);
  for (const check of failed) console.log(`  FAIL ${check.name}${check.file ? ` (${check.file})` : ''}`);
}
process.exitCode = report.pass ? 0 : 1;
