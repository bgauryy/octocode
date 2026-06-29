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
    agentic: false,
    help: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { opts.help = true; continue; }
    if (arg === '--list') { opts.list = true; continue; }
    if (arg === '--json') { opts.json = true; continue; }
    if (arg === '--self-test') { opts.selfTest = true; continue; }
    if (arg === '--agentic') { opts.agentic = true; continue; }
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

const STOP_WORDS = new Set([
  'about', 'after', 'again', 'answer', 'before', 'build', 'could', 'does', 'from',
  'have', 'idea', 'into', 'local', 'mode', 'more', 'only', 'prompt', 'should',
  'that', 'their', 'there', 'this', 'what', 'when', 'where', 'which', 'while',
  'with', 'without', 'would',
]);

function extractIntentTerms(text, limit = 8) {
  const counts = new Map();
  for (const raw of String(text || '').toLowerCase().match(/[a-z][a-z0-9-]{3,}/g) || []) {
    const term = raw.replace(/^-+|-+$/g, '');
    if (!term || STOP_WORDS.has(term)) continue;
    counts.set(term, (counts.get(term) || 0) + 1);
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, limit)
    .map(([term]) => term);
}

function modeQuestion(testCase) {
  if (testCase.mode === 'Map') {
    return 'Did the answer map who has tried this, what worked, and where the gaps remain?';
  }
  if (testCase.mode === 'Generate') {
    return 'Did the answer expand the space before narrowing to the most promising directions?';
  }
  return 'Did the answer test whether this is worth pursuing for a specific user, pain, and success signal?';
}

function buildAgenticEval(testCase, text) {
  const prompt = testCase.prompt || `${testCase.mode || 'Brainstorm'} request`;
  const rubric = Array.isArray(testCase.rubric) ? testCase.rubric.join(' ') : '';
  const intentTerms = extractIntentTerms(`${prompt} ${rubric}`, 6);
  const answerSignals = extractIntentTerms(text, 6);
  const generatedQuestions = [
    {
      id: 'agentic-user-problem-fit',
      dimension: 'intent',
      question: `For the request "${prompt}", did the answer identify the user, painful situation, and desired outcome well enough to judge the idea?`,
    },
    {
      id: 'agentic-mode-fit',
      dimension: 'framing',
      question: modeQuestion(testCase),
    },
    {
      id: 'agentic-evidence-to-decision',
      dimension: 'decision',
      question: 'Did the verdict follow from the strongest evidence and concessions, rather than from enthusiasm or template compliance?',
    },
    {
      id: 'agentic-scope-razor',
      dimension: 'scope',
      question: 'Did the answer choose a scope razor or next experiment that would actually change the decision?',
    },
  ];
  if (intentTerms.length) {
    generatedQuestions.splice(1, 0, {
      id: 'agentic-intent-terms',
      dimension: 'intent',
      question: `Did the answer engage the salient intent terms (${intentTerms.join(', ')}) as context for judgment, without reducing the evaluation to keyword matching?`,
    });
  }
  return {
    advisoryOnly: true,
    affectsScore: false,
    intent: prompt,
    intentTerms,
    answerSignals,
    generatedQuestions,
    evaluatorPrompt: [
      'You are an eval agent for brainstorming, not a fixed checklist. Create 3-5 binary questions from the user intent, the case mode, and the answer.',
      'Use the generatedQuestions as seeds only: rewrite, add, or drop questions when the idea demands it.',
      'Use answerSignals only to notice what the answer emphasized; do not require those terms.',
      'Prefer questions about user/problem/success signal, evidence quality, differentiated wedge, scope, and decision usefulness.',
      'Answer each question yes/no/uncertain with evidence and a suggested lesson. Do not use advisory questions as a rigid gate.',
    ].join(' '),
    answerShape: {
      question: 'string',
      verdict: 'yes | no | uncertain',
      evidence: 'short quote or file/URL anchor from the answer',
      suggestedLesson: 'one reusable improvement, if any',
      failureSignature: 'mechanism:<area>|cause:<reason> when verdict is no',
    },
  };
}

function evaluateCase(testCase, text, opts = {}) {
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
  const binaryQuestions = (testCase.binaryQuestions || []).map(question => {
    const passPattern = question.passPattern || question.pattern || '';
    const failPattern = question.failPattern || '';
    const matchedPass = passPattern ? compile(passPattern).test(text) : false;
    const matchedFail = failPattern ? compile(failPattern).test(text) : false;
    return {
      id: question.id,
      dimension: question.dimension || 'general',
      question: question.question,
      passed: matchedPass && !matchedFail,
      matchedPass,
      matchedFail,
      passPattern,
      failPattern,
      failureSignature: question.failureSignature,
      suggestedLesson: question.suggestedLesson,
    };
  });
  const dimensionScores = {};
  for (const question of binaryQuestions) {
    const bucket = dimensionScores[question.dimension] || { passed: 0, total: 0, score: 0 };
    bucket.total += 1;
    if (question.passed) bucket.passed += 1;
    bucket.score = Number((bucket.passed / bucket.total).toFixed(3));
    dimensionScores[question.dimension] = bucket;
  }
  const citationCount = countCitations(text);
  const citationPassed = citationCount >= (testCase.minCitationCount || 0);
  const checks = [
    ...required,
    ...forbidden,
    ...binaryQuestions.map(question => ({ name: `binary:${question.id}`, passed: question.passed })),
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
    binaryQuestions,
    dimensionScores,
    failedBinaryQuestions: binaryQuestions.filter(question => !question.passed).map(question => ({
      id: question.id,
      dimension: question.dimension,
      failureSignature: question.failureSignature,
      suggestedLesson: question.suggestedLesson,
    })),
    ...(opts.agentic ? { agenticEval: buildAgenticEval(testCase, text) } : {}),
    failedChecks: checks.filter(check => !check.passed).map(check => check.name),
  };
}

function renderText(results) {
  const lines = [];
  for (const result of results) {
    lines.push(`${result.passed ? 'PASS' : 'FAIL'} ${result.id}: ${result.score}/${result.minScore}`);
    if (result.agenticEval) {
      lines.push(`  agentic: ${result.agenticEval.generatedQuestions.length} advisory intent questions`);
    }
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
  node scripts/eval-brainstorm.mjs --case idea-validation --input answer.md --agentic --json
  cat answer.md | node scripts/eval-brainstorm.mjs --case idea-validation
  node scripts/eval-brainstorm.mjs --self-test

Options:
  --list          List eval cases
  --case <id>     Evaluate only one case
  --input, -i     Answer file. Omit to read stdin
  --json          Emit JSON result
  --agentic       Include advisory eval-agent question seeds derived from the case intent; does not affect score
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

function readFixture(relativePath) {
  return readFileSync(resolve(SKILL_DIR, relativePath), 'utf8');
}

function runSelfTest(cases) {
  const idea = cases.find(testCase => testCase.id === 'idea-validation');
  if (!idea) throw new Error('missing idea-validation case');
  const good = evaluateCase(idea, strongSample(), { agentic: true });
  const bad = evaluateCase(idea, weakSample(), { agentic: true });
  if (!good.passed) {
    throw new Error(`strong sample should pass: ${good.failedChecks.join(', ')}`);
  }
  if (good.failedBinaryQuestions.length) {
    throw new Error(`strong sample has failed binary questions: ${good.failedBinaryQuestions.map(q => q.id).join(', ')}`);
  }
  if (good.agenticEval.generatedQuestions.length < 3) {
    throw new Error('strong sample should emit advisory agentic questions');
  }
  if (bad.passed) {
    throw new Error('weak sample should fail');
  }
  const conflict = cases.find(testCase => testCase.id === 'conflicting-evidence');
  if (!conflict) throw new Error('missing conflicting-evidence case');
  const conflictGood = evaluateCase(conflict, readFixture(conflict.fixtures.passing), { agentic: true });
  const conflictBad = evaluateCase(conflict, readFixture(conflict.fixtures.failing), { agentic: true });
  if (!conflictGood.passed) {
    throw new Error(`conflict fixture should pass: ${conflictGood.failedChecks.join(', ')}`);
  }
  if (conflictGood.failedBinaryQuestions.length) {
    throw new Error(`conflict fixture has failed binary questions: ${conflictGood.failedBinaryQuestions.map(q => q.id).join(', ')}`);
  }
  if (conflictBad.passed) {
    throw new Error('conflict fixture without concession should fail');
  }
  if (!conflictBad.failedBinaryQuestions.some(question => question.id === 'concedes-unsupported-side')) {
    throw new Error('conflict failing fixture should mark the missing concession');
  }
  const resourceFirst = cases.find(testCase => testCase.id === 'resource-first-research');
  if (!resourceFirst) throw new Error('missing resource-first-research case');
  const resourceFirstGood = evaluateCase(resourceFirst, readFixture(resourceFirst.fixtures.passing), { agentic: true });
  const resourceFirstBad = evaluateCase(resourceFirst, readFixture(resourceFirst.fixtures.failing), { agentic: true });
  if (!resourceFirstGood.passed) {
    throw new Error(`resource-first fixture should pass: ${resourceFirstGood.failedChecks.join(', ')}`);
  }
  if (resourceFirstGood.failedBinaryQuestions.length) {
    throw new Error(`resource-first fixture has failed binary questions: ${resourceFirstGood.failedBinaryQuestions.map(q => q.id).join(', ')}`);
  }
  if (resourceFirstBad.passed) {
    throw new Error('resource-first fixture without top-resource loop should fail');
  }
  if (!resourceFirstBad.failedBinaryQuestions.some(question => question.id === 'starts-from-top-resources')) {
    throw new Error('resource-first failing fixture should mark the missing top-resource start');
  }
  return {
    ok: true,
    casesPath: CASES_PATH,
    strongSample: good,
    weakSample: bad,
    resourceFirst: {
      passingFixture: resourceFirstGood,
      failingFixture: resourceFirstBad,
    },
    conflictingEvidence: {
      passingFixture: conflictGood,
      failingFixture: conflictBad,
    },
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

  const results = selected.map(testCase => evaluateCase(testCase, answer, { agentic: opts.agentic }));
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
