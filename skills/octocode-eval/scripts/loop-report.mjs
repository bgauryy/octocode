#!/usr/bin/env node
/**
 * Validate an octocode-eval loop report has the required sections.
 * Usage:
 *   node scripts/loop-report.mjs --input report.md
 *   node scripts/loop-report.mjs --self-test
 *   cat report.md | node scripts/loop-report.mjs
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const REQUIRED = [
  { name: 'Goal', pattern: /^#{1,3}\s*Goal\b/m },
  { name: 'KPI', pattern: /^#{1,3}\s*KPI\b/m },
  { name: 'primary metric', pattern: /\bprimary\b[\s\S]{0,120}\b(baseline|result)\b/i },
  { name: 'loop level', pattern: /^#{1,3}\s*Loop level\b|\bLoop level\b[\s\S]{0,40}\b(experiment|suite|meta)\b/im },
  { name: 'Checks run', pattern: /^#{1,3}\s*Checks run\b/m },
  { name: 'Verdict', pattern: /\bVerdict\b[\s\S]{0,80}\b(ACCEPT|REVERT|CONTINUE)\b/i },
];

const FORBIDDEN = [
  { name: 'narrative-only accept', pattern: /\bfeels better\b|\bvibes?\b.*\baccept/i },
  { name: 'harness cheat', pattern: /\bedited (the )?(eval|cases|grader)/i },
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
  const checks = [...required, ...forbidden];
  const score = checks.filter((c) => c.passed).length / checks.length;
  const passed = required.every((c) => c.passed) && forbidden.every((c) => c.passed);
  return {
    score: Number(score.toFixed(3)),
    passed,
    failedChecks: checks.filter((c) => !c.passed).map((c) => c.name),
    required,
    forbidden,
  };
}

function readText(input) {
  if (input) return readFileSync(resolve(process.cwd(), input), 'utf8');
  return readFileSync(0, 'utf8');
}

const GOOD = `## Goal
Raise skill-review pass rate for octocode-eval.

## KPI
- primary (lagging): skill-review ERROR count (lower-better) baseline=2 result=0 target=0 — serves goal
- leading: eval-eval case pass count
- guardrails: eval-eval --self-test stays green

## Loop level
meta

## Budget / trials
1 trial, fixed command set

## Subject changed
references/eval-techniques.md wording only

## Harness unchanged? (yes/no)
yes

## Checks run
- node scripts/eval-eval.mjs --self-test → exit 0
- held-out: case reject-vibe still pass

## Transcript note
Failure was fair; grader matched missing Verdict section.

## Verdict
ACCEPT

## Next
Install skill to common platform.
`;

const BAD = `## Goal
Make it nicer

## Verdict
ACCEPT because it feels better after we edited the eval cases
`;

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    console.log(`Usage:
  node scripts/loop-report.mjs [--input report.md] [--json]
  node scripts/loop-report.mjs --self-test
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
