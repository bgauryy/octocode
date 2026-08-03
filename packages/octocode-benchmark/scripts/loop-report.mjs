#!/usr/bin/env node
/**
 * Validate a benchmark-run SUMMARY.md has the required sections and no
 * cheat/vibe patterns. Self-contained (no external deps) so `yarn` /
 * `node packages/octocode-benchmark/scripts/loop-report.mjs` works without
 * depending on skill sync. Mirrors the octocode-graph-eval loop-report
 * contract, extended with benchmark-specific gates.
 *
 * Usage:
 *   node packages/octocode-benchmark/scripts/loop-report.mjs --input output/<run>/SUMMARY.md
 *   node packages/octocode-benchmark/scripts/loop-report.mjs --self-test
 *   cat SUMMARY.md | node packages/octocode-benchmark/scripts/loop-report.mjs
 *
 * Exit 0 = pass, 1 = fail. A score=1 (all checks pass) is required only for an
 * ACCEPT/WIN verdict; CONTINUE/DRAFT/INCONCLUSIVE runs may legitimately fail
 * the "verdict-eligible" gates and are reported as such.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const REQUIRED = [
  { name: 'Goal', pattern: /^#{1,3}\s*Goal\b/m },
  { name: 'KPI', pattern: /^#{1,3}\s*KPI\b/m },
  { name: 'primary metric', pattern: /\bprimary\b[\s\S]{0,160}\b(baseline|result)\b/i },
  { name: 'loop level', pattern: /^#{1,3}\s*Loop level\b|\bLoop level\b[\s\S]{0,40}\b(experiment|suite|meta)\b/im },
  { name: 'Checks run', pattern: /^#{1,3}\s*Checks run\b/m },
  {
    name: 'Verdict',
    pattern: /\bVerdict\b[\s\S]{0,80}\b(ACCEPT|REVERT|CONTINUE|WIN|TIE|LOSS|DRAFT|INCONCLUSIVE)\b/i,
  },
];

const FORBIDDEN = [
  { name: 'narrative-only accept', pattern: /\bfeels better\b|\bvibes?\b[\s\S]{0,40}\baccept/i },
  { name: 'harness cheat', pattern: /\bedited (the )?(eval|cases?|graders?|questions?|oracle|rubric)\b/i },
];

/**
 * Verdict-eligibility gates: a WIN/ACCEPT verdict is only trustworthy when the
 * run actually captured what the methodology requires. These do not fail
 * CONTINUE/DRAFT runs (they are expected to be Unavailable there) but they DO
 * block a claimed win that is missing its evidence.
 */
const VERDICT_GATES = [
  {
    name: 'runner-token capture for verdict',
    // A WIN/ACCEPT must not rest on estimated-only tokens.
    appliesTo: /\bVerdict\b[\s\S]{0,80}\b(WIN|ACCEPT)\b/i,
    // FAIL if a win is claimed while tokens are Unavailable/estimated-only.
    fail: /runner\s*tokens?[\s:=]*[\s\S]{0,40}\b(unavailable|estimated)\b/i,
  },
  {
    name: 'k>=3 for verdict',
    appliesTo: /\bVerdict\b[\s\S]{0,80}\b(WIN|ACCEPT)\b/i,
    fail: /\bpass\^?k[\s\S]{0,40}\bnot met\b|\bk\s*=\s*1\b/i,
  },
  {
    name: 'verified oracle for verdict',
    appliesTo: /\bVerdict\b[\s\S]{0,80}\b(WIN|ACCEPT)\b/i,
    fail: /\bUNVERIFIED_DRAFT\b|verdictAllowed[\s:=]*false/i,
  },
];

function parseArgs(argv) {
  const opts = { input: '', json: false, selfTest: false, help: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--help' || a === '-h') { opts.help = true; continue; }
    if (a === '--json') { opts.json = true; continue; }
    if (a === '--self-test') { opts.selfTest = true; continue; }
    if (a === '--input' || a === '-i') { opts.input = argv[++i] || ''; continue; }
    throw new Error(`Unknown argument: ${a}`);
  }
  return opts;
}

function evaluate(text) {
  const required = REQUIRED.map((c) => ({ name: c.name, passed: c.pattern.test(text) }));
  const forbidden = FORBIDDEN.map((c) => ({ name: c.name, passed: !c.pattern.test(text) }));
  const verdictGates = VERDICT_GATES
    .filter((g) => g.appliesTo.test(text))
    .map((g) => ({ name: g.name, passed: !g.fail.test(text) }));
  const checks = [...required, ...forbidden, ...verdictGates];
  const score = checks.length ? checks.filter((c) => c.passed).length / checks.length : 0;
  const passed =
    required.every((c) => c.passed) &&
    forbidden.every((c) => c.passed) &&
    verdictGates.every((c) => c.passed);
  return {
    score: Number(score.toFixed(3)),
    passed,
    failedChecks: checks.filter((c) => !c.passed).map((c) => c.name),
    required,
    forbidden,
    verdictGates,
  };
}

function readText(input) {
  if (input) return readFileSync(resolve(process.cwd(), input), 'utf8');
  return readFileSync(0, 'utf8');
}

const GOOD = `## Goal
Compare octocode vs gh+rtk on the github research-v2 bank.

## KPI
- primary (lagging): uncontaminated mean correctness (1-10) baseline=gh-rtk result=octocode target=non-inferior
- leading: median VRPT, false-confidence count
- guardrails: false confidence must not increase

## Loop level
suite

## Checks run
- runner tokens captured per question (tokenSource=runner)
- k=3 solvers per arm; pass^k met
- oracle VERIFIED_WITH_REVERIFICATION_CONTRACT

## Verdict
WIN — median VRPT higher, VR >= 0.6, correctness non-inferior.
`;

const BAD = `## Goal
Make it look good

## Verdict
WIN because it feels better after we edited the oracle; runner tokens Unavailable; k=1.
`;

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    console.log(`Usage:
  node packages/octocode-benchmark/scripts/loop-report.mjs [--input SUMMARY.md] [--json]
  node packages/octocode-benchmark/scripts/loop-report.mjs --self-test
`);
    return;
  }
  if (opts.selfTest) {
    const good = evaluate(GOOD);
    const bad = evaluate(BAD);
    const ok = good.passed && !bad.passed;
    const out = { selfTest: ok, good, bad };
    console.log(opts.json ? JSON.stringify(out, null, 2) : `self-test: ${ok ? 'pass' : 'fail'}`);
    process.exitCode = ok ? 0 : 1;
    return;
  }
  const result = evaluate(readText(opts.input));
  if (opts.json) console.log(JSON.stringify(result, null, 2));
  else {
    console.log(`${result.passed ? 'pass' : 'fail'} score=${result.score}`);
    if (result.failedChecks.length) console.log(`  failed: ${result.failedChecks.join(', ')}`);
  }
  process.exitCode = result.passed ? 0 : 1;
}

main();
