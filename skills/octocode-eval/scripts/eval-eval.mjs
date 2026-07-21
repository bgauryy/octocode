#!/usr/bin/env node
/**
 * Machine-checkable smoke evals for octocode-eval answers / loop reports.
 * Usage:
 *   node scripts/eval-eval.mjs --list
 *   node scripts/eval-eval.mjs --case define-kpi --input answer.md
 *   node scripts/eval-eval.mjs --self-test
 */
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SKILL_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..');
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

function runPattern(pattern, text) {
  return new RegExp(pattern, 'ims').test(text);
}

function evaluateCase(testCase, text) {
  const required = (testCase.required || []).map((check) => ({
    name: check.name,
    passed: runPattern(check.pattern, text),
  }));
  const forbidden = (testCase.forbidden || []).map((check) => ({
    name: check.name,
    passed: !runPattern(check.pattern, text),
  }));
  const binary = (testCase.binaryQuestions || []).map((q) => ({
    id: q.id,
    passed: runPattern(q.passPattern, text),
    question: q.question,
    failureSignature: q.failureSignature,
  }));
  const checks = [...required, ...forbidden, ...binary.map((b) => ({ name: b.id, passed: b.passed }))];
  const score = checks.length ? checks.filter((c) => c.passed).length / checks.length : 1;
  const passed = score >= (testCase.minScore || 1);
  return {
    id: testCase.id,
    mode: testCase.mode,
    score: Number(score.toFixed(3)),
    minScore: testCase.minScore || 1,
    passed,
    required,
    forbidden,
    binaryQuestions: binary,
    failedChecks: checks.filter((c) => !c.passed).map((c) => c.name),
  };
}

function readAnswer(input) {
  if (input) return readFileSync(resolve(process.cwd(), input), 'utf8');
  return readFileSync(0, 'utf8');
}

function strongSample(caseId) {
  const samples = {
    'define-kpi': `Mode: Define
Goal: Cut false skill triggers for octocode-eval.
Primary KPI: false-trigger rate on held-out prompts (lower-better) baseline=0.40 target=0.10
Guardrails: true-trigger recall >= 0.90; skill-review ERROR count = 0
Budget: 20 prompts, 1 trial each
Held-out: 8 prompts never used to invent the description edit
Subject under test: SKILL.md description only
Harness unchanged: yes`,
    'link-goal-kpi': `Mode: Define
Goal: Agents accept fewer vibe-only skill edits.
Primary KPI (lagging): held-out ACCEPT/REVERT accuracy (higher-better) baseline=0.55 target=0.85 — serves goal
Leading: eval-eval pass rate on link-goal-kpi + define-kpi cases
Guardrails: skill-review ERROR=0; true-trigger recall >= 0.9
Decision: ACCEPT if primary>=0.85 AND guardrails hold`,
    'run-keep-discard': `Mode: Run
## Goal
Improve loop-report completeness.

## KPI
- primary: loop-report pass rate (higher-better) baseline=0.50 result=1.00 target=1.00
- guardrails: eval-eval --self-test green

## Loop level
experiment

## Budget / trials
fixed: node scripts/loop-report.mjs --self-test

## Subject changed
references/output.md required sections list

## Harness unchanged? (yes/no)
yes

## Checks run
- node scripts/loop-report.mjs --self-test exit 0
- held-out: define-kpi case still passes

## Transcript note
Fair fail earlier: missing Verdict section.

## Verdict
ACCEPT

## Next
Ship skill.`,
    'nested-loops-pick': `Mode: Run
The experiment loop is flat after N keep/discard trials.
Escalate to the suite loop: run error analysis, grow failure-taxonomy cases, then re-baseline.
Do not edit the grader or cases to make the flat experiment pass — keep a frozen harness.
If the same failureSignature recurs after suite growth, escalate to the meta/harness loop with human gate.`,
    'choose-graders': `Mode: Suite
Grader mix: deterministic regex floor first; BinEval-style binaryQuestions for failure signatures; LLM rubric only for open-ended tone; human calibration weekly.
Capability vs regression: new hard cases stay in capability suite; saturated cases graduate to regression.
Outcome over path: grade test pass and state checks, not exact tool-call order.
Coding: require fail-to-pass plus pass-to-pass guardrails.
pass@1 for one-shot coding; pass^k when consistency matters.`,
    'pick-benchmark': `Mode: Benchmark
Do not use SWE-bench Verified as the only ship gate — public boards are orientation only and risk contamination/saturation.
Prefer a private suite from our real failures as the ship gate; treat public scores as weak unless transcripts are audited.
Retire saturated benches to regression smoke and build a fresh private capability suite.`,
    'error-analyze': `Mode: ErrorAnalyze
Gather traces into a dataset, open coding the first failure per trace, then axial-code a failure taxonomy by frequency.
Write eval cases from top failure modes with failureSignature keys — not from generic toxicity/helpfulness scores.
Stop when new traces add no categories; real failures beat vanity metrics.`,
    'reject-vibe': `Mode: Audit
Verdict: REVERT
Reason: narrative-only claim ("feels better") with no baseline KPI and an attempt to edit eval cases to pass.
Harness must stay frozen; results beat words.`,
    'tdd-red-green': `Mode: Run
## Goal
Prove the subject change with a TDD-shaped eval loop.

## KPI
- primary: case score baseline=0.4 result=0.9 target=0.85

## Loop level
experiment

## Subject changed
one paragraph in the skill lobby

## Harness unchanged? (yes/no)
Harness unchanged: yes (frozen harness)

## Checks run
Red: failing case first (held-out untouched). Green: re-measure with the same command after the subject change. Keep only if guardrails hold; else DISCARD. Never greenwash by rewriting cases mid-run.

## Verdict
ACCEPT

## Next
None.`,
  };
  if (!samples[caseId]) throw new Error(`No strong sample for ${caseId}`);
  return samples[caseId];
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  const data = loadCases();
  if (opts.help) {
    console.log(`Usage:
  node scripts/eval-eval.mjs --list
  node scripts/eval-eval.mjs --case <id> --input answer.md [--json]
  node scripts/eval-eval.mjs --self-test [--json]
`);
    return;
  }
  if (opts.list) {
    for (const c of data.cases) console.log(`${c.id}\t${c.mode || ''}\t${c.prompt || ''}`);
    return;
  }
  if (opts.selfTest) {
    const results = data.cases.map((c) => evaluateCase(c, strongSample(c.id)));
    const passed = results.every((r) => r.passed);
    const out = { selfTest: passed, results };
    console.log(opts.json ? JSON.stringify(out, null, 2) : `self-test: ${passed ? 'pass' : 'fail'} (${results.filter((r) => r.passed).length}/${results.length})`);
    process.exitCode = passed ? 0 : 1;
    return;
  }
  if (!opts.caseId) throw new Error('Provide --case <id> or --self-test');
  const testCase = data.cases.find((c) => c.id === opts.caseId);
  if (!testCase) throw new Error(`Unknown case: ${opts.caseId}`);
  const result = evaluateCase(testCase, readAnswer(opts.input));
  if (opts.json) console.log(JSON.stringify(result, null, 2));
  else {
    console.log(`${result.id}: ${result.passed ? 'pass' : 'fail'} score=${result.score}`);
    if (result.failedChecks.length) console.log(`  failed: ${result.failedChecks.join(', ')}`);
  }
  process.exitCode = result.passed ? 0 : 1;
}

main();
