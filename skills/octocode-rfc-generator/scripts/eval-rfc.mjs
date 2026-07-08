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
  const prs = text.match(/\b(?:PR|commit|issue|RFC)\s*[#:]?\s*[A-Za-z0-9_-]+/gi) || [];
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
  const samples = {
    'brainstorm-handoff-rfc': `Status: Ready for Review
Mode: RFC
Handoff intake: chosen framing is agent research reliability; value thesis is shorter validated briefs; first slice is claim-ledger validation.
Claim ledger
claim | evidence | confidence | section | gap / next proof
Long briefs need claim traceability | skills/octocode-research/references/long-research.md:1 | confirmed | Rationale | none
Existing workflow validates citations | skills/octocode-rfc-generator/references/workflow.md:65 | confirmed | Validation | none
Alternatives Considered: do nothing, add prose only, add ledger plus evaluator.
Risks: extra ceremony on small plans. Open Questions: whether to persist ledgers.
Acceptance Criteria: Done when generated RFCs include cited claims and no placeholders. Success signal: reviewer can trace each decision.
Rollback trigger: if agents overuse ledgers on small edits, make ledger optional.`,
    'public-contract-migration': `Mode: Migration
Migration plan for public API compatibility.
Current State: packages/octocode/src/index.ts:12 exports the old API.
Target State: packages/octocode/src/index.ts:44 exports the new API with a compatibility wrapper.
Public contract gate: breaking change risk requires owner approval and compatibility notes.
Phase 1: add wrapper and tests. Phase 2: migrate internal callers. Phase 3: remove old entry after release gate.
Question Gate: no open clarifying questions; owner, scope, and compatibility priority are assumed from the prompt.
Verification commands: yarn typecheck && yarn test && yarn lint.
Rollback trigger: any downstream package fails contract tests.
Owner / approver: Tools core maintainer.`,
    'existing-code-folder-rfc': `Status: Ready for Review
Mode: RFC — folder set in .octocode/rfc/token-cache/

RFC.md
Decision type: irreversible (one-way door).
Goals: unify the token cache across packages. Non-Goals: no change to the public API surface.
Alternatives Considered: do nothing, minimal patch, full redesign. Prior art reviewed.
Unresolved Questions: which default cache TTL to use.

PREREQUISITES.md
Required current-state evidence: packages/core/src/cache.ts:42 defines the existing TTL behavior.
Baseline verification: yarn test packages/core.
Blockers before implementation: cache owner confirms compatibility for downstream packages.

IMPLEMENTATION.md
Resolved Questions (were open in RFC.md): TTL default resolved to 300s per packages/core/src/cache.ts:42 and confirmed at https://github.com/owner/repo/pull/128.
Steps ordered by dependency; touches packages/core/src/cache.ts:60.
Rollback trigger: contract tests fail.

KPI.md — Success and Verification
User Stories to Check: As a developer, I want faster lookups, so that latency drops.
Acceptance Criteria (Gherkin):
Given a warm cache
When a repeated lookup runs
Then it returns without a network call.
Success Metrics: primary lagging latency baseline 120ms Target 60ms; guardrail error rate must not regress.
Decision Rule: Roll back if latency stays above the target after the rollout window.
Traceability matrix: RFC requirement to user story to acceptance criteria to verification method to status.`,
  };
  return samples[caseId] || '';
}

function weakSample() {
  return `Decision: Ship it
This is clearly best. TBD. We'll do it next week. No alternatives, citations, rollback, or verification needed.`;
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
  return `RFC generator output evaluator

Usage:
  node scripts/eval-rfc.mjs --list
  node scripts/eval-rfc.mjs --case brainstorm-handoff-rfc --input answer.md --json
  cat answer.md | node scripts/eval-rfc.mjs --case public-contract-migration
  node scripts/eval-rfc.mjs --self-test

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
