#!/usr/bin/env node

import { existsSync, readFileSync } from 'node:fs';
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
    verifyLinks: false,
    linkTimeoutMs: 5000,
    help: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { opts.help = true; continue; }
    if (arg === '--list') { opts.list = true; continue; }
    if (arg === '--json') { opts.json = true; continue; }
    if (arg === '--self-test') { opts.selfTest = true; continue; }
    if (arg === '--agentic') { opts.agentic = true; continue; }
    if (arg === '--verify-links') { opts.verifyLinks = true; continue; }
    if (arg === '--link-timeout') { opts.linkTimeoutMs = Number(argv[++i]) || opts.linkTimeoutMs; continue; }
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

function extractCitations(text) {
  const urls = text.match(/https?:\/\/[^\s)]+/g) || [];
  const fileRefRaw = text.match(/\b[\w./-]+\.(?:md|mjs|js|ts|tsx|json|py|sh):\d+\b/g) || [];
  const fileRefs = fileRefRaw.map(raw => {
    const idx = raw.lastIndexOf(':');
    return { raw, file: raw.slice(0, idx), line: Number(raw.slice(idx + 1)) };
  });
  return { urls, fileRefs };
}

function countCitations(text) {
  const { urls, fileRefs } = extractCitations(text);
  return urls.length + fileRefs.length;
}

// A cited file:line can point anywhere depending on where the answer was produced,
// so try the caller's declared workspace first, then cwd, then this monorepo's root
// (this skill's own dev/self-test fallback) — first directory where the file exists wins.
function resolveBaseDirs() {
  const candidates = [process.env.WORKSPACE_ROOT, process.cwd(), resolve(SKILL_DIR, '..', '..')];
  const seen = new Set();
  const dirs = [];
  for (const candidate of candidates) {
    if (!candidate) continue;
    const abs = resolve(candidate);
    if (seen.has(abs)) continue;
    seen.add(abs);
    dirs.push(abs);
  }
  return dirs;
}

// Deterministic, local, no network: catches fabricated paths and stale line numbers
// (e.g. a citation surviving a later edit that shrank the target file).
function checkFileCitations(fileRefs) {
  const baseDirs = resolveBaseDirs();
  return fileRefs.map(ref => {
    let resolvedPath = null;
    let exists = false;
    for (const dir of baseDirs) {
      const candidate = resolve(dir, ref.file);
      if (existsSync(candidate)) {
        resolvedPath = candidate;
        exists = true;
        break;
      }
    }
    let lineCount = null;
    let lineInBounds = null;
    if (exists) {
      try {
        lineCount = readFileSync(resolvedPath, 'utf8').split('\n').length;
        lineInBounds = ref.line >= 1 && ref.line <= lineCount;
      } catch {
        lineInBounds = null;
      }
    }
    return { ...ref, exists, resolvedPath, lineCount, lineInBounds };
  });
}

// Opt-in (--verify-links) since it needs network access, which self-test and default
// CI runs must not depend on. Only a definitive 404 fails the check — timeouts, 403s,
// 5xx, and hosts that reject HEAD are "unverified", not "dead", so a flaky network or
// a bot-blocking host never fails an otherwise-good answer.
async function checkUrlLinks(urls, { timeoutMs = 5000 } = {}) {
  const unique = [...new Set(urls)];
  return Promise.all(unique.map(async url => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    try {
      const response = await fetch(url, { method: 'HEAD', redirect: 'follow', signal: controller.signal });
      if (response.status === 404) return { url, status: 'dead', httpStatus: response.status };
      if (response.ok) return { url, status: 'alive', httpStatus: response.status };
      return { url, status: 'unverified', httpStatus: response.status };
    } catch (err) {
      return { url, status: 'unverified', httpStatus: null, error: err?.message || String(err) };
    } finally {
      clearTimeout(timer);
    }
  }));
}

// SKILL.md, references/output.md, and references/brief-template.md all make the
// closing Sources (chat) / Resources (saved brief) section mandatory whenever any
// external evidence was cited. Any case with minCitationCount > 0 implies external
// evidence was expected, so it must also close with one of these headings.
const SOURCES_HEADING_PATTERN = /^##\s*(Sources|Resources)\b/m;

function hasSourcesSection(text) {
  return SOURCES_HEADING_PATTERN.test(text);
}

function compile(pattern) {
  return new RegExp(pattern, 'ims');
}

function checkPattern(text, check) {
  return compile(check.pattern).test(text);
}

function extractIntentTerms(text, limit = 8) {
  const counts = new Map();
  for (const raw of String(text || '').toLowerCase().match(/[a-z][a-z0-9-]{3,}/g) || []) {
    const term = raw.replace(/^-+|-+$/g, '');
    if (!term) continue;
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
      id: 'agentic-citation-faithfulness',
      dimension: 'evidence',
      question: "For each citation used above, does the linked source's actual content support the specific claim placed next to it — not just topical relevance, a loose paraphrase, or an unverified assertion?",
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
      'Prefer questions about user/problem/success signal, evidence quality, citation faithfulness (does the source actually back the claim, not just exist), differentiated wedge, scope, and decision usefulness.',
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
  const { urls, fileRefs } = extractCitations(text);
  const citationCount = urls.length + fileRefs.length;
  const citationPassed = citationCount >= (testCase.minCitationCount || 0);
  const sourcesSectionRequired = (testCase.minCitationCount || 0) > 0;
  const sourcesSectionPresent = hasSourcesSection(text);

  const fileCitations = fileRefs.length ? checkFileCitations(fileRefs) : [];
  const brokenFileCitations = fileCitations.filter(ref => !ref.exists || ref.lineInBounds === false);
  const fileCitationsRequired = fileRefs.length > 0;

  const urlCitations = opts.urlChecks || null;
  const deadUrlCitations = urlCitations ? urlCitations.filter(check => check.status === 'dead') : [];
  const urlCitationsRequired = Boolean(urlCitations && urlCitations.length);

  const checks = [
    ...required,
    ...forbidden,
    ...binaryQuestions.map(question => ({ name: `binary:${question.id}`, passed: question.passed })),
    {
      name: `citations >= ${testCase.minCitationCount || 0}`,
      passed: citationPassed,
      observed: citationCount,
    },
    {
      name: 'closes with Sources/Resources section',
      passed: !sourcesSectionRequired || sourcesSectionPresent,
      observed: sourcesSectionPresent,
      skipped: !sourcesSectionRequired,
    },
    {
      name: 'cited file:line references resolve',
      passed: !fileCitationsRequired || brokenFileCitations.length === 0,
      observed: brokenFileCitations.map(ref => ref.raw),
      skipped: !fileCitationsRequired,
    },
    {
      name: 'cited links are reachable (verified)',
      passed: !urlCitationsRequired || deadUrlCitations.length === 0,
      observed: deadUrlCitations.map(check => check.url),
      skipped: !urlCitationsRequired,
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
    fileCitations,
    urlCitations,
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
  node scripts/eval-brainstorm.mjs --case idea-validation --input answer.md --verify-links --json
  cat answer.md | node scripts/eval-brainstorm.mjs --case idea-validation
  node scripts/eval-brainstorm.mjs --self-test

Options:
  --list           List eval cases
  --case <id>      Evaluate only one case
  --input, -i      Answer file. Omit to read stdin
  --json           Emit JSON result
  --agentic        Include advisory eval-agent question seeds derived from the case intent; does not affect score
  --verify-links   Live-check cited URLs (HEAD, network required); only a definitive 404 fails the score, everything else is unverified. Cited file:line references are always checked locally (no flag needed).
  --link-timeout   Per-URL timeout in ms for --verify-links (default: 5000)
  --self-test      Run evaluator smoke checks

Cases file: ${CASES_PATH}`;
}

function strongSample() {
  return `Mode: Validate

## TL;DR
Issue-to-plan CLI has thin prior art; worth a bounded prototype. Research limits: none.

## Surface Plan
Local active; GitHub/packages active; Web active.

## Direction Check
User chose the issue-to-plan workflow; no broader automation path was researched.

## Framings Considered
Researched: issue-to-plan CLI.

## Landscape
- Example source. \`moderate\` https://example.com/source
- Local source. \`moderate\` skills/octocode-brainstorming/SKILL.md:18

## Perspective Review
- Critical Architect: held claim because integration risk is bounded; evidence https://example.com/source.
- Visionary Entrepreneur: held claim because urgent workflow exists; evidence skills/octocode-brainstorming/SKILL.md:18.
- Product: held claim because MVP can test one workflow; evidence https://example.com/product.
- Conceded: broad automation claim dropped.

Decision: Prototype First

## Recommended Next Step
Prototype the hardest unknown first.

## Sources
- https://example.com/source — backs the integration-risk and workflow-urgency claims above.
- skills/octocode-brainstorming/SKILL.md:18 — backs the MVP-scope claim above.`;
}

function weakSample() {
  return 'This is clearly proven. I implemented the code. Full transcript follows.';
}

// Corrupts exactly one of strongSample's two identical file:line citations (the first
// occurrence) so the sample still has a valid citation alongside a fabricated one —
// isolates whether the check catches a single broken ref, not just an all-broken sample.
function brokenFileCitationSample() {
  return strongSample().replace(
    'skills/octocode-brainstorming/SKILL.md:18',
    'skills/octocode-brainstorming/SKILL.md:9999',
  );
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
  if (good.fileCitations.some(ref => !ref.exists || ref.lineInBounds === false)) {
    throw new Error(`strong sample cites a file:line that does not resolve: ${JSON.stringify(good.fileCitations)}`);
  }
  const brokenCitation = evaluateCase(idea, brokenFileCitationSample(), { agentic: true });
  if (!brokenCitation.failedChecks.includes('cited file:line references resolve')) {
    throw new Error('a fabricated file:line citation should be caught by the citation-resolve check');
  }
  if (brokenCitation.score >= good.score) {
    throw new Error('a fabricated file:line citation should score strictly lower than the fully-valid strong sample');
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
  const trendMomentum = cases.find(testCase => testCase.id === 'trend-momentum-check');
  if (!trendMomentum) throw new Error('missing trend-momentum-check case');
  const trendMomentumGood = evaluateCase(trendMomentum, readFixture(trendMomentum.fixtures.passing), { agentic: true });
  const trendMomentumBad = evaluateCase(trendMomentum, readFixture(trendMomentum.fixtures.failing), { agentic: true });
  if (!trendMomentumGood.passed) {
    throw new Error(`trend-momentum fixture should pass: ${trendMomentumGood.failedChecks.join(', ')}`);
  }
  if (trendMomentumGood.failedBinaryQuestions.length) {
    throw new Error(`trend-momentum fixture has failed binary questions: ${trendMomentumGood.failedBinaryQuestions.map(q => q.id).join(', ')}`);
  }
  if (trendMomentumBad.passed) {
    throw new Error('trend-momentum fixture without a real signal should fail');
  }
  if (!trendMomentumBad.failedBinaryQuestions.some(question => question.id === 'dispatches-trend-source-scout')) {
    throw new Error('trend-momentum failing fixture should mark the missing Trend & Source Scout dispatch');
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
    brokenFileCitationSample: brokenCitation,
    resourceFirst: {
      passingFixture: resourceFirstGood,
      failingFixture: resourceFirstBad,
    },
    trendMomentum: {
      passingFixture: trendMomentumGood,
      failingFixture: trendMomentumBad,
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

  const urlChecks = opts.verifyLinks
    ? await checkUrlLinks(extractCitations(answer).urls, { timeoutMs: opts.linkTimeoutMs })
    : null;
  const results = selected.map(testCase => evaluateCase(testCase, answer, { agentic: opts.agentic, urlChecks }));
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
