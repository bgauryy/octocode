#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SKILL_DIR = resolve(__dirname, '..');
const CASES_PATH = resolve(SKILL_DIR, 'evals', 'cases.json');

function die(message, code = 1) {
  process.stderr.write(`ERROR: ${message}\n`);
  process.exitCode = code;
}

function parseArgs(argv) {
  const opts = {
    input: '',
    caseId: '',
    list: false,
    json: false,
    selfTest: false,
    help: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { opts.help = true; continue; }
    if (arg === '--list') { opts.list = true; continue; }
    if (arg === '--json') { opts.json = true; continue; }
    if (arg === '--self-test') { opts.selfTest = true; continue; }
    if (arg === '--input' || arg === '-i') { opts.input = argv[++i] || ''; continue; }
    if (arg === '--case') { opts.caseId = argv[++i] || ''; continue; }
    die(`Unknown argument: ${arg}`);
    return null;
  }
  return opts;
}

function loadCases() {
  return JSON.parse(readFileSync(CASES_PATH, 'utf8'));
}

function readStdin() {
  return new Promise((resolvePromise, reject) => {
    let data = '';
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', chunk => { data += chunk; });
    process.stdin.on('end', () => resolvePromise(data));
    process.stdin.on('error', reject);
  });
}

function countCitations(text) {
  const urls = text.match(/https?:\/\/[^\s)]+/g) || [];
  const fileLines = text.match(/\b[\w./-]+\.(?:md|mjs|js|ts|tsx|json|py|sh):\d+\b/g) || [];
  return urls.length + fileLines.length;
}

function compile(pattern) {
  return new RegExp(pattern, 'ims');
}

function checkPattern(text, check) {
  return compile(check.pattern).test(text);
}

function evaluateCase(testCase, text) {
  const required = (testCase.required || []).map(check => ({
    name: check.name,
    passed: checkPattern(text, check),
    pattern: check.pattern,
  }));
  const forbidden = (testCase.forbidden || []).map(check => ({
    name: check.name,
    passed: !checkPattern(text, check),
    pattern: check.pattern,
  }));
  const citationCount = countCitations(text);
  const citationPassed = citationCount >= (testCase.minCitationCount || 0);
  const checks = [
    ...required,
    ...forbidden,
    {
      name: `citations >= ${testCase.minCitationCount || 0}`,
      passed: citationPassed,
      observed: citationCount,
    },
  ];
  const passedCount = checks.filter(check => check.passed).length;
  const score = checks.length ? passedCount / checks.length : 1;
  return {
    id: testCase.id,
    mode: testCase.mode,
    score: Number(score.toFixed(3)),
    minScore: testCase.minScore || 1,
    passed: score >= (testCase.minScore || 1),
    citationCount,
    required,
    forbidden,
    failedChecks: checks.filter(check => !check.passed).map(check => check.name),
  };
}

function renderText(results) {
  const lines = [];
  for (const result of results) {
    lines.push(`${result.passed ? 'PASS' : 'FAIL'} ${result.id}: ${result.score}/${result.minScore}`);
    if (result.failedChecks.length) {
      lines.push(`  failed: ${result.failedChecks.join(', ')}`);
    }
  }
  return lines.join('\n');
}

function usage() {
  return `Brainstorming answer evaluator

Usage:
  node scripts/eval-brainstorm.mjs --list
  node scripts/eval-brainstorm.mjs --case idea-validation --input answer.md --json
  cat answer.md | node scripts/eval-brainstorm.mjs --case idea-validation
  node scripts/eval-brainstorm.mjs --self-test

Options:
  --list          List eval cases
  --case <id>     Evaluate only one case
  --input, -i     Answer file. Omit to read stdin
  --json          Emit JSON result
  --self-test     Run evaluator smoke checks

Cases file: ${CASES_PATH}`;
}

function strongSample() {
  return `Mode: Validate

## Surface Plan
Local active; GitHub/packages active; Web active.

## Framings Considered
Researched: issue-to-plan CLI.

## Landscape
- Example source. \`moderate\` https://example.com/source
- Local source. \`moderate\` skills/octocode-brainstorming/SKILL.md:57

## Perspective Review
- Critical Architect: held claim because integration risk is bounded; evidence https://example.com/source.
- Visionary Entrepreneur: held claim because urgent workflow exists; evidence skills/octocode-brainstorming/SKILL.md:57.
- Product: held claim because MVP can test one workflow; evidence https://example.com/product.
- Conceded: broad automation claim dropped.

Decision: Prototype First

## Recommended Next Step
Prototype the hardest unknown first.`;
}

function weakSample() {
  return 'This is clearly proven. I implemented the code. Full transcript follows.';
}

function runSelfTest(cases) {
  const idea = cases.find(testCase => testCase.id === 'idea-validation');
  if (!idea) throw new Error('missing idea-validation case');
  const good = evaluateCase(idea, strongSample());
  const bad = evaluateCase(idea, weakSample());
  if (!good.passed) {
    throw new Error(`strong sample should pass: ${good.failedChecks.join(', ')}`);
  }
  if (bad.passed) {
    throw new Error('weak sample should fail');
  }
  return {
    ok: true,
    casesPath: CASES_PATH,
    strongSample: good,
    weakSample: bad,
  };
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (!opts) return;
  if (opts.help) {
    console.log(usage());
    return;
  }

  const data = loadCases();
  const cases = data.cases || [];

  if (opts.list) {
    const rows = cases.map(testCase => ({
      id: testCase.id,
      mode: testCase.mode,
      prompt: testCase.prompt,
      minScore: testCase.minScore,
      minCitationCount: testCase.minCitationCount || 0,
    }));
    console.log(opts.json ? JSON.stringify({ cases: rows }, null, 2) : rows.map(row => `${row.id} (${row.mode}) - ${row.prompt}`).join('\n'));
    return;
  }

  if (opts.selfTest) {
    try {
      const result = runSelfTest(cases);
      console.log(JSON.stringify(result, null, 2));
      return;
    } catch (err) {
      die(err.message || String(err));
      return;
    }
  }

  const selected = opts.caseId ? cases.filter(testCase => testCase.id === opts.caseId) : cases;
  if (!selected.length) {
    die(opts.caseId ? `No eval case found for id: ${opts.caseId}` : 'No eval cases found.');
    return;
  }

  let answer = '';
  if (opts.input) {
    answer = readFileSync(resolve(process.cwd(), opts.input), 'utf8');
  } else {
    answer = await readStdin();
  }
  if (!answer.trim()) {
    die('No answer text provided. Use --input or pipe text on stdin.');
    return;
  }

  const results = selected.map(testCase => evaluateCase(testCase, answer));
  const passed = results.every(result => result.passed);
  const payload = {
    ok: passed,
    casesPath: CASES_PATH,
    evaluated: results.length,
    results,
  };
  console.log(opts.json ? JSON.stringify(payload, null, 2) : renderText(results));
  process.exitCode = passed ? 0 : 1;
}

main().catch(err => die(err.message || String(err)));
