#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SKILL_DIR = resolve(__dirname, '..');
const CASES_PATH = resolve(SKILL_DIR, 'evals', 'cases.json');

function parseArgs(argv) {
  const opts = { caseId: '', input: '', json: false, list: false, selfTest: false, help: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { opts.help = true; continue; }
    if (arg === '--list') { opts.list = true; continue; }
    if (arg === '--json') { opts.json = true; continue; }
    if (arg === '--self-test') { opts.selfTest = true; continue; }
    if (arg === '--case') { opts.caseId = argv[++i] || ''; continue; }
    if (arg === '--input' || arg === '-i') { opts.input = argv[++i] || ''; continue; }
    throw new Error(`Unknown argument: ${arg}`);
  }
  return opts;
}

function loadCases() {
  const raw = JSON.parse(readFileSync(CASES_PATH, 'utf8'));
  if (!Array.isArray(raw.cases)) throw new Error('evals/cases.json must contain a cases array');
  return raw;
}

function countEvidenceAnchors(text) {
  const urls = text.match(/https?:\/\/[^\s)]+/g) || [];
  const fileLines = text.match(/\b[A-Za-z0-9_.\/-]+:\d+\b/g) || [];
  const prs = text.match(/\b(?:PR|commit|issue)\s*[#:]?\s*[A-Za-z0-9_-]+/gi) || [];
  return new Set([...urls, ...fileLines, ...prs]).size;
}

function runPattern(pattern, text) {
  return new RegExp(pattern, 'ims').test(text);
}

function evaluateCase(testCase, text) {
  const required = (testCase.required || []).map(check => ({
    name: check.name,
    passed: runPattern(check.pattern, text),
    pattern: check.pattern,
  }));
  const forbidden = (testCase.forbidden || []).map(check => ({
    name: check.name,
    passed: !runPattern(check.pattern, text),
    pattern: check.pattern,
  }));
  const citationCount = countEvidenceAnchors(text);
  const citationPassed = citationCount >= (testCase.minCitationCount || 0);
  const checks = [...required, ...forbidden, { name: `evidence anchors >= ${testCase.minCitationCount || 0}`, passed: citationPassed }];
  const score = checks.length ? checks.filter(c => c.passed).length / checks.length : 1;
  const passed = score >= (testCase.minScore || 1);
  return {
    id: testCase.id,
    mode: testCase.mode,
    score: Number(score.toFixed(3)),
    minScore: testCase.minScore || 1,
    passed,
    citationCount,
    required,
    forbidden,
    failedChecks: checks.filter(c => !c.passed).map(c => c.name),
  };
}

function readAnswer(input) {
  if (input) return readFileSync(resolve(process.cwd(), input), 'utf8');
  return readFileSync(0, 'utf8');
}

function printResult(result, asJson) {
  if (asJson) {
    console.log(JSON.stringify(result, null, 2));
    return;
  }
  const rows = Array.isArray(result.results) ? result.results : [result];
  for (const row of rows) {
    console.log(`${row.id}: ${row.passed ? 'pass' : 'fail'} score=${row.score} citations=${row.citationCount}`);
    if (row.failedChecks?.length) console.log(`  failed: ${row.failedChecks.join(', ')}`);
  }
}

function strongSample(caseId) {
  const base = {
    'code-investigation': `Mode: Investigate
Scope: local code and current CLI help; active surfaces: local tree, exact file reads, LSP.
Hypotheses: path scope is wrong; alternate query-shape uses structural syntax; disconfirm with exact read.
Exact evidence: packages/octocode/src/cli/main-help.ts:42 and packages/octocode-tools-core/src/tools/local_search/execution.ts:88.
Finding: zero matches do not prove absence; verify path, branch, and query shape.
Confidence: confirmed
Next: run documentSymbols then references with the line anchor.`,
    'prior-art-map': `Mode: Map
Landscape: repositories and packages clustered as active, partial, and abandoned.
Packages and repositories: ast-grep package plus tree-sitter repositories.
Package health: last publish, maintainer count, issue ratio, release cadence, and dependency freshness were checked.
- ast-grep: structural search CLI. \`moderate\` https://github.com/ast-grep/ast-grep
- tree-sitter: parser ecosystem. \`moderate\` https://tree-sitter.github.io/tree-sitter/
Confidence: likely
Next: compare APIs against local use cases.`,
    'oql-graph-proof': `Mode: Investigate
First ran search --scheme --compact before OQL JSON.
Used target:"research" then target:"graph" for research/graph proof.
Candidate rows are not proof; mark them as tentative until upgraded.
Upgrade proof: exact read at packages/foo/src/index.ts:12, import search, AST checks, LSP references, and tests.
Gate broad deletion until confirmed by LSP and tests.`,
    'degraded-transport': `Mode: Investigate
Degraded transport: Octocode unavailable, so confidence degraded.
Fallback path: continue with rg and file reads, plus web only if local files cannot answer.
This does not block the answer; npx octocode, or npx octocode auth login, is only needed if GitHub/private data is required.
Confidence: uncertain
Next: install/run npx octocode only if local evidence is insufficient.`,
  };
  return base[caseId] || '';
}

function weakSample() {
  return `I know the answer. Empty result means absence. Confidence: confirmed. Safe to delete all candidates.`;
}

function selfTest() {
  const data = loadCases();
  const results = data.cases.map(testCase => {
    const strong = evaluateCase(testCase, strongSample(testCase.id));
    const weak = evaluateCase(testCase, weakSample());
    return { id: testCase.id, strongPassed: strong.passed, weakPassed: weak.passed, strong, weak };
  });
  const ok = results.every(r => r.strongPassed && !r.weakPassed);
  return { ok, casesPath: CASES_PATH, results };
}

function usage() {
  return `Research answer evaluator

Usage:
  node scripts/eval-research.mjs --list
  node scripts/eval-research.mjs --case code-investigation --input answer.md --json
  cat answer.md | node scripts/eval-research.mjs --case prior-art-map
  node scripts/eval-research.mjs --self-test

Options:
  --list          List eval cases
  --case <id>     Evaluate one case
  --input, -i     Answer file. Omit to read stdin
  --json          Emit JSON result
  --self-test     Run evaluator smoke checks

Cases file: ${CASES_PATH}`;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) { console.log(usage()); return; }
  const data = loadCases();
  if (opts.list) {
    for (const c of data.cases) console.log(`${c.id}\t${c.mode}\t${c.prompt}`);
    return;
  }
  if (opts.selfTest) {
    const result = selfTest();
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.ok ? 0 : 1;
    return;
  }
  const selected = opts.caseId ? data.cases.filter(c => c.id === opts.caseId) : data.cases;
  if (!selected.length) throw new Error(`No eval case found for ${opts.caseId}`);
  const answer = readAnswer(opts.input);
  const results = selected.map(c => evaluateCase(c, answer));
  const result = selected.length === 1 ? results[0] : { ok: results.every(r => r.passed), results };
  printResult(result, opts.json);
  process.exitCode = Array.isArray(result.results)
    ? (result.ok ? 0 : 1)
    : (result.passed ? 0 : 1);
}

main().catch(err => {
  process.stderr.write(`ERROR: ${err.message || String(err)}\n`);
  process.exitCode = 1;
});
