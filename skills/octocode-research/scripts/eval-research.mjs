#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SKILL_DIR = resolve(__dirname, '..');
const CASES_PATH = resolve(SKILL_DIR, 'evals', 'cases.json');

function parseArgs(argv) {
  const opts = { caseId: '', input: '', json: false, list: false, selfTest: false, agentic: false, help: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { opts.help = true; continue; }
    if (arg === '--list') { opts.list = true; continue; }
    if (arg === '--json') { opts.json = true; continue; }
    if (arg === '--self-test') { opts.selfTest = true; continue; }
    if (arg === '--agentic') { opts.agentic = true; continue; }
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
    return 'Did the answer discover and cluster the landscape before judging what matters?';
  }
  if (testCase.mode === 'Validate') {
    return 'Did the answer test both the upside and the strongest reason not to proceed?';
  }
  if (testCase.mode === 'Plan') {
    return 'Did the answer turn evidence into a bounded, reversible next step?';
  }
  return 'Did the answer investigate competing explanations before settling on a finding?';
}

function buildAgenticEval(testCase, text) {
  const prompt = testCase.prompt || `${testCase.mode || 'Research'} answer`;
  const rubric = Array.isArray(testCase.rubric) ? testCase.rubric.join(' ') : '';
  const intentTerms = extractIntentTerms(`${prompt} ${rubric}`, 6);
  const answerSignals = extractIntentTerms(text, 6);
  const generatedQuestions = [
    {
      id: 'agentic-intent-fit',
      dimension: 'intent',
      question: `For the actual request "${prompt}", did the answer solve the user's intent rather than merely matching the output template?`,
    },
    {
      id: 'agentic-mode-fit',
      dimension: 'reasoning',
      question: modeQuestion(testCase),
    },
    {
      id: 'agentic-evidence-fit',
      dimension: 'evidence',
      question: 'Did the answer choose evidence strong enough for the claim it makes, and mark uncertainty where evidence is thin?',
    },
    {
      id: 'agentic-next-step-fit',
      dimension: 'decision',
      question: 'Did the final next step follow from the intent and evidence rather than from a canned workflow?',
    },
  ];
  if (intentTerms.length) {
    generatedQuestions.splice(1, 0, {
      id: 'agentic-intent-terms',
      dimension: 'intent',
      question: `Did the answer engage the salient intent terms (${intentTerms.join(', ')}) in context, without treating keyword mentions as proof?`,
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
      'You are an eval agent, not a fixed rubric. Create 3-5 binary questions from the user intent, the case mode, and the answer.',
      'Use the generatedQuestions as seeds only: rewrite, add, or drop questions if the intent demands it.',
      'Use answerSignals only to notice what the answer emphasized; do not require those terms.',
      'Each question must be answerable yes/no/uncertain from the answer text and should change guidance if it fails.',
      'Answer each question with verdict, evidence from the answer, and a suggested lesson. Do not fail the answer just because a seed was not relevant.',
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
    passed: runPattern(check.pattern, text),
    pattern: check.pattern,
  }));
  const forbidden = (testCase.forbidden || []).map(check => ({
    name: check.name,
    passed: !runPattern(check.pattern, text),
    pattern: check.pattern,
  }));
  const binaryQuestions = (testCase.binaryQuestions || []).map(question => {
    const passPattern = question.passPattern || question.pattern || '';
    const failPattern = question.failPattern || '';
    const matchedPass = passPattern ? runPattern(passPattern, text) : false;
    const matchedFail = failPattern ? runPattern(failPattern, text) : false;
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
  const citationCount = countEvidenceAnchors(text);
  const citationPassed = citationCount >= (testCase.minCitationCount || 0);
  const checks = [
    ...required,
    ...forbidden,
    ...binaryQuestions.map(question => ({ name: `binary:${question.id}`, passed: question.passed })),
    { name: `evidence anchors >= ${testCase.minCitationCount || 0}`, passed: citationPassed },
  ];
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
    binaryQuestions,
    dimensionScores,
    failedBinaryQuestions: binaryQuestions.filter(question => !question.passed).map(question => ({
      id: question.id,
      dimension: question.dimension,
      failureSignature: question.failureSignature,
      suggestedLesson: question.suggestedLesson,
    })),
    ...(opts.agentic ? { agenticEval: buildAgenticEval(testCase, text) } : {}),
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
    if (row.agenticEval) console.log(`  agentic: ${row.agenticEval.generatedQuestions.length} advisory intent questions`);
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
    'long-decision-brief': `Mode: Validate
Long research: read long-research.md because this is a durable decision brief with contested scope.
Campaign scope: question=adopt claim-level evidence ledger; surfaces=local docs active, web optional; budget=30 minutes and 3 source passes; stop gates=claims have proof or next pass will not change verdict; non-goals=do not build storage yet.
Evidence ledger: ev1 skills/octocode-research/references/long-research.md:47 says evidence rows stay standalone; ev2 https://example.com/evals says traceable claims improve review.
Claim ledger: cl1 supported by ev1; cl2 unverified because vendor cost is unknown.
Unsupported gaps: exact UI storage shape remains unverified.
Vendor adapters are optional; they enrich web/paper surfaces but do not replace local proof.
Verdict: Prototype First
Recommended next step: add the ledger only to long research runs.`,
    'github-landscape': `Mode: Map
GitHub Landscape
Repo clusters: active, partial, abandoned.
Ranked repo table / repo DB: fields include fit, activity, evidence, reuse, and risk.
Repositories and packages: ast-grep repository plus npm package; tree-sitter repository plus packages.
Exact reads: README and source were exact-read for top candidates: https://github.com/ast-grep/ast-grep and https://github.com/tree-sitter/tree-sitter.
Stars and downloads are tiebreakers, not proof.
Integration blueprint: reuse parser-facing ideas, avoid unverified service dependencies, proof still needed is a local prototype against our TypeScript files.
Confidence: likely
Next: run one prototype command before adopting.`,
    'change-mode': `Mode: Change
Scope: replace moment.js with Intl.DateTimeFormat inside the formatDate utility only.
Blast radius: LSP references and callers of formatDate show 3 consumers; exact read at src/utils/date.ts:14 and src/components/Header.tsx:27.
Patch: smallest scoped change — only the formatDate function body; the exported signature is unchanged.
Verification: yarn test src/utils/date.test.ts ran and passed; typecheck passed with exit 0.
Confidence: confirmed
Next: drop the moment dependency in a follow-up once no other imports remain.`,
    'pr-local-review': `Mode: Review
Scope: collected via git status and git diff --staged; 3 staged files, all in the auth area.
Risk: src/auth/login.ts is HIGH (auth logic changed); README.md is LOW (docs-only).
Sizing: Full pass, because one file is HIGH risk even though the file count is small; skipped the Quick surface-scan shortcut.
Blast radius: the changed login() signature was traced with LSP callers (incoming) at src/auth/login.ts:42; two callers found at src/api/session.ts:18 and src/api/session.ts:55.
Domains checked in order: Security, Bug, Flow Impact, Architecture, Performance, Error Handling, Quality.
[SEC-1] title: missing input validation on new token parameter
Severity: HIGH
Confidence: confirmed
Location: src/auth/login.ts:47
Evidence: exact read shows the parameter is passed to a SQL query unescaped; LSP callers confirm both call sites pass user input directly.
Impact: caller/user data path is exposed to injection.
Fix: validate/sanitize the token parameter before use, mirroring the existing pattern in src/auth/session.ts:20.
No existing PR comments to reconcile locally; findings deduped by root cause and capped to the highest-impact issue.
Next: run the project's auth test suite before opening the PR.`,
  };
  return base[caseId] || '';
}

function weakSample() {
  return `I know the answer. Empty result means absence. Confidence: confirmed. Safe to delete all candidates.`;
}

function selfTest() {
  const data = loadCases();
  const results = data.cases.map(testCase => {
    const strong = evaluateCase(testCase, strongSample(testCase.id), { agentic: true });
    const weak = evaluateCase(testCase, weakSample(), { agentic: true });
    return {
      id: testCase.id,
      strongPassed: strong.passed,
      strongBinaryClean: strong.failedBinaryQuestions.length === 0,
      strongAgenticQuestions: strong.agenticEval.generatedQuestions.length,
      weakPassed: weak.passed,
      strong,
      weak,
    };
  });
  const ok = results.every(r => r.strongPassed && r.strongBinaryClean && r.strongAgenticQuestions >= 3 && !r.weakPassed);
  return { ok, casesPath: CASES_PATH, results };
}

function usage() {
  return `Research answer evaluator

Usage:
  node scripts/eval-research.mjs --list
  node scripts/eval-research.mjs --case code-investigation --input answer.md --json
  node scripts/eval-research.mjs --case code-investigation --input answer.md --agentic --json
  cat answer.md | node scripts/eval-research.mjs --case prior-art-map
  node scripts/eval-research.mjs --self-test

Options:
  --list          List eval cases
  --case <id>     Evaluate one case
  --input, -i     Answer file. Omit to read stdin
  --json          Emit JSON result
  --agentic       Include advisory eval-agent question seeds derived from the case intent; does not affect score
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
  const results = selected.map(c => evaluateCase(c, answer, { agentic: opts.agentic }));
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
