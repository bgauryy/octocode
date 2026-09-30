#!/usr/bin/env node
// Aggregate a run into results/<run-id>/summary.json and markdown tables.
//
//   node report.mjs --run-id <id> [--write-docs]
//
// --write-docs regenerates docs/BENCHMARKS.md from BENCHMARKS.template.md.
// Every number on that page comes from summary.json.
import fs from 'node:fs';
import path from 'node:path';
import { EVAL_DIR, REPO_ROOT, RESULTS_DIR, loadQuestions, parseArgs, readJson, writeJson } from './lib.mjs';

const args = parseArgs(process.argv.slice(2));
if (!args['run-id']) throw new Error('--run-id is required');
const runId = String(args['run-id']);
const runDir = path.join(RESULTS_DIR, runId);
const manifest = readJson(path.join(runDir, 'manifest.json'));
const ARMS = manifest.arms; // [octocode, rg-gh]
const [A, B] = ARMS;
const ARM_LABEL = { octocode: 'With Octocode', 'rg-gh': 'Without (rg + gh)' };
const L = (a) => ARM_LABEL[a] ?? a;
const questions = loadQuestions().filter((q) => manifest.questionIds.includes(q.id));

const mean = (xs) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);
const median = (xs) => {
  if (!xs.length) return null;
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const sum = (xs) => xs.reduce((a, b) => a + b, 0);
const fmt = (x, d = 2) => (x === null || x === undefined || Number.isNaN(x) ? '–' : Number(x).toFixed(d));
const kfmt = (x) => (x === null || x === undefined ? '–' : Math.abs(x) >= 1e6 ? `${(x / 1e6).toFixed(2)}M` : Math.abs(x) >= 1e3 ? `${(x / 1e3).toFixed(1)}k` : String(Math.round(x)));
const pct = (n, d) => (d ? `${Math.round((100 * n) / d)}%` : '–');
const ratio = (a, b) => (a === null || b === null || !b ? null : a / b);
const rfmt = (r) => (r === null ? '–' : `${r.toFixed(2)}×`);

// ---- fixed per-call context, measured from each run's own first API call -----------
// The first request of a run carries Claude Code's system prompt, the tool definitions,
// any MCP server instructions and the question (identical across arms apart from one
// sentence naming rg and gh). Every later request re-reads that prefix.
function streamCalls(file) {
  const ids = [];
  let first = null;
  for (const line of fs.readFileSync(file, 'utf8').split('\n')) {
    if (!line.includes('"type":"assistant"')) continue;
    let e; try { e = JSON.parse(line); } catch { continue; }
    const id = e.message?.id;
    if (!id || ids.includes(id)) continue;
    ids.push(id);
    if (!first) { const u = e.message.usage ?? {}; first = (u.input_tokens ?? 0) + (u.cache_creation_input_tokens ?? 0) + (u.cache_read_input_tokens ?? 0); }
  }
  return { apiCalls: ids.length, firstCallInput: first };
}

// ---- collect -------------------------------------------------------------
const runs = [];
const judged = [];
for (const q of questions) {
  for (let pass = 1; pass <= manifest.passes; pass++) {
    const finalPath = path.join(runDir, q.id, 'judge', `pass${pass}`, 'final.json');
    const final = fs.existsSync(finalPath) ? readJson(finalPath) : null;
    if (final) judged.push(final);
    for (const arm of ARMS) {
      const p = path.join(runDir, q.id, arm, `pass${pass}`, 'run.json');
      if (!fs.existsSync(p)) { runs.push({ qid: q.id, arm, pass, surface: q.surface, category: q.category, missing: true }); continue; }
      const r = readJson(p);
      const u = r.usage;
      const total = u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens + u.output_tokens;
      const sc = streamCalls(path.join(runDir, q.id, arm, `pass${pass}`, 'stream.jsonl'));
      const overheadTokens = sc.firstCallInput === null ? null : Math.min(total, sc.firstCallInput * sc.apiCalls);
      runs.push({
        qid: q.id, arm, pass, surface: q.surface, category: q.category, status: r.status,
        verdict: final?.final?.[arm]?.verdict ?? 'unjudged', score: final?.final?.[arm]?.score ?? null,
        input_fresh: u.input_tokens, cache_write: u.cache_creation_input_tokens, cache_read: u.cache_read_input_tokens,
        output: u.output_tokens, input_total: u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens,
        total_tokens: total, overhead_tokens: overheadTokens, research_tokens: overheadTokens === null ? null : total - overheadTokens,
        first_call_input: sc.firstCallInput, api_calls: sc.apiCalls,
        cost: r.total_cost_usd, turns: r.num_turns, calls: r.toolCallCount, wall_s: r.wallMs / 1000,
        toolCounts: r.toolCounts, clasifyCalls: r.toolCounts['mcp__octocode__clasify'] ?? 0, denials: r.permission_denials.length,
        offAllowlist: (r.toolCalls ?? []).filter((c) => c.name === 'Bash' && !/^\s*(rg|gh)\s/.test(c.command ?? '')).length,
        toolErrors: r.toolErrorCount, isolationOk: r.isolation.ok, isolationProblems: r.isolation.problems,
      });
    }
  }
}

const TOKEN_KEYS = ['input_fresh', 'cache_write', 'cache_read', 'input_total', 'output', 'total_tokens', 'overhead_tokens', 'research_tokens'];
function aggregate(rs) {
  const done = rs.filter((r) => !r.missing);
  const resolved = done.filter((r) => ['correct', 'partial', 'wrong'].includes(r.verdict));
  const count = (v) => resolved.filter((r) => r.verdict === v).length;
  const passes = [...new Set(done.map((r) => r.pass))].sort();
  const perPassScore = passes.map((p) => mean(resolved.filter((r) => r.pass === p).map((r) => r.score))).filter((x) => x !== null);
  const perPassTokens = passes.map((p) => sum(done.filter((r) => r.pass === p).map((r) => r.total_tokens)));
  const perPassCorrect = passes.map((p) => resolved.filter((r) => r.pass === p && r.verdict === 'correct').length);
  const stat = (k) => {
    const xs = done.map((r) => r[k]).filter((x) => x !== null && x !== undefined);
    return { total: sum(xs), mean: mean(xs), median: median(xs) };
  };
  return {
    runs: done.length, missing: rs.length - done.length,
    statuses: Object.fromEntries([...new Set(done.map((r) => r.status))].map((s) => [s, done.filter((r) => r.status === s).length])),
    judged: resolved.length, unresolved: done.filter((r) => r.verdict === 'unresolved').length, unjudged: done.filter((r) => r.verdict === 'unjudged').length,
    correct: count('correct'), partial: count('partial'), wrong: count('wrong'),
    meanScore: mean(resolved.map((r) => r.score)),
    passMeanScores: perPassScore, passScoreSpread: perPassScore.length ? Math.max(...perPassScore) - Math.min(...perPassScore) : null,
    passCorrect: perPassCorrect,
    passTotalTokens: perPassTokens,
    tokens: Object.fromEntries(TOKEN_KEYS.map((k) => [k, stat(k)])),
    cost: stat('cost'), calls: stat('calls'), turns: stat('turns'), firstCallInput: stat('first_call_input'), apiCalls: stat('api_calls'), wall_s: stat('wall_s'),
    toolErrors: sum(done.map((r) => r.toolErrors)), denials: sum(done.map((r) => r.denials)),
    isolationFailures: done.filter((r) => !r.isolationOk).length,
  };
}

const byArm = Object.fromEntries(ARMS.map((a) => [a, aggregate(runs.filter((r) => r.arm === a))]));
const slices = {};
for (const key of ['surface', 'category']) {
  for (const v of [...new Set(questions.map((q) => q[key]))]) {
    slices[`${key}:${v}`] = { key, value: v, n: questions.filter((q) => q[key] === v).length, ...Object.fromEntries(ARMS.map((a) => [a, aggregate(runs.filter((r) => r.arm === a && r[key] === v))])) };
  }
}
const perQuestion = questions.map((q) => {
  const row = { qid: q.id, surface: q.surface, category: q.category, source: q.source.id };
  for (const a of ARMS) {
    const rs = runs.filter((r) => r.qid === q.id && r.arm === a && !r.missing);
    row[a] = {
      verdicts: rs.map((r) => r.verdict), meanScore: mean(rs.filter((r) => r.score !== null).map((r) => r.score)),
      meanTotalTokens: mean(rs.map((r) => r.total_tokens)), meanResearchTokens: mean(rs.map((r) => r.research_tokens).filter((x) => x !== null)),
      meanCost: mean(rs.map((r) => r.cost)), meanCalls: mean(rs.map((r) => r.calls)), meanWall: mean(rs.map((r) => r.wall_s)),
      clasifyCalls: sum(rs.map((r) => r.clasifyCalls ?? 0)),
    };
  }
  row.tokenRatio = ratio(row[A].meanTotalTokens, row[B].meanTotalTokens);
  row.researchTokenRatio = ratio(row[A].meanResearchTokens, row[B].meanResearchTokens);
  return row;
});
const ratios = perQuestion.map((q) => q.tokenRatio).filter((x) => x !== null);
const researchRatios = perQuestion.map((q) => q.researchTokenRatio).filter((x) => x !== null && Number.isFinite(x));
const tokenRatios = {
  questions: ratios.length, mean: mean(ratios), median: median(ratios),
  geomean: ratios.length ? Math.exp(mean(ratios.map(Math.log))) : null,
  ofSums: ratio(byArm[A].tokens.total_tokens.total, byArm[B].tokens.total_tokens.total),
  octocodeFewer: ratios.filter((r) => r < 1).length,
  researchMean: mean(researchRatios), researchMedian: median(researchRatios),
};

// clasify usage in the Octocode arm
const oc = runs.filter((r) => r.arm === 'octocode' && !r.missing);
const withClasify = oc.filter((r) => r.clasifyCalls > 0);
const clasify = {
  runs: oc.length, runsWithClasify: withClasify.length, totalCalls: sum(oc.map((r) => r.clasifyCalls)),
  questions: [...new Set(withClasify.map((r) => r.qid))],
  outcomeWith: aggregate(withClasify), outcomeWithout: aggregate(oc.filter((r) => r.clasifyCalls === 0)),
};

// judge agreement
const judgeCalls = judged.flatMap((j) => j.calls);
const judge = {
  pairs: judged.length,
  orderAgree: judged.filter((j) => j.orderAgree).length,
  firstPairScoreAgree: judged.filter((j) => j.firstPairScoreAgree).length,
  firstPairPreferredAgree: judged.filter((j) => j.firstPairPreferredAgree).length,
  tiebreaks: judged.filter((j) => j.usedTiebreak).length,
  unresolvedArmVerdicts: judged.reduce((s, j) => s + ARMS.filter((a) => !j.final[a].resolved).length, 0),
  calls: judgeCalls.length, parseFailures: judgeCalls.filter((c) => !c.parsed).length,
  cost: sum(judged.map((j) => j.judgeCost ?? 0)),
  preferred: Object.fromEntries([...ARMS, 'tie'].map((a) => [a, judgeCalls.filter((c) => c.preferredArm === a).length])),
};

// paired per-question score comparison (mean over passes)
const paired = perQuestion.filter((q) => ARMS.every((a) => q[a].meanScore !== null)).map((q) => ({ qid: q.qid, delta: q[A].meanScore - q[B].meanScore }));
const comparison = {
  questionsCompared: paired.length,
  octocodeBetter: paired.filter((p) => p.delta >= 0.5).map((p) => p.qid),
  rgBetter: paired.filter((p) => p.delta <= -0.5).map((p) => p.qid),
  similar: paired.filter((p) => Math.abs(p.delta) < 0.5).map((p) => p.qid),
  meanDelta: mean(paired.map((p) => p.delta)),
};

const isolation = {
  solverRuns: runs.filter((r) => !r.missing).length,
  failures: runs.filter((r) => !r.missing && !r.isolationOk).map((r) => ({ qid: r.qid, arm: r.arm, pass: r.pass, problems: r.isolationProblems })),
  deniedCalls: Object.fromEntries(ARMS.map((a) => [a, byArm[a].denials])),
  offAllowlistAttempts: sum(runs.filter((r) => r.arm === 'rg-gh' && !r.missing).map((r) => r.offAllowlist)),
};

const summary = {
  runId, generatedAt: new Date().toISOString(),
  setup: { model: manifest.model, maxTurns: manifest.maxTurns, timeoutMs: manifest.timeoutMs, passes: manifest.passes, concurrency: manifest.concurrency, claudeVersion: manifest.claudeVersion, corpus: manifest.corpus, hashes: manifest.hashes, createdAt: manifest.createdAt },
  overhead: { method: 'first API call input of each run × API calls in the run', byArm: Object.fromEntries(ARMS.map((a) => [a, { firstCallInputMedian: byArm[a].firstCallInput.median, apiCallsMean: byArm[a].apiCalls.mean }])) },
  byArm, slices, tokenRatios, perQuestion, comparison, clasify, judge, isolation, runs,
};
writeJson(path.join(runDir, 'summary.json'), summary);

// ---- markdown --------------------------------------------------------------
const mark = (v) => ({ correct: '✓', partial: '½', wrong: '✗', unresolved: '?', unjudged: '·' }[v] ?? '·');
const range = (xs, f = kfmt) => (xs.length ? `${f(Math.min(...xs))}–${f(Math.max(...xs))}` : '–');

function headlineTable(sel) {
  const a = sel[A]; const b = sel[B];
  const out = [`| Metric | ${L(A)} | ${L(B)} | Ratio (with ÷ without) |`, '|---|---:|---:|---:|'];
  const row = (name, fa, fb, ra) => out.push(`| ${name} | ${fa} | ${fb} | ${ra} |`);
  row('Correct answers', `${a.correct} / ${a.judged}`, `${b.correct} / ${b.judged}`, rfmt(ratio(a.correct, b.correct)));
  row('Partial / wrong', `${a.partial} / ${a.wrong}`, `${b.partial} / ${b.wrong}`, '');
  row('Mean score (0–3)', fmt(a.meanScore), fmt(b.meanScore), '');
  row('**Total tokens per run** (mean)', `**${kfmt(a.tokens.total_tokens.mean)}**`, `**${kfmt(b.tokens.total_tokens.mean)}**`, `**${rfmt(ratio(a.tokens.total_tokens.mean, b.tokens.total_tokens.mean))}**`);
  row('– input, fresh', kfmt(a.tokens.input_fresh.mean), kfmt(b.tokens.input_fresh.mean), rfmt(ratio(a.tokens.input_fresh.mean, b.tokens.input_fresh.mean)));
  row('– input, cache write', kfmt(a.tokens.cache_write.mean), kfmt(b.tokens.cache_write.mean), rfmt(ratio(a.tokens.cache_write.mean, b.tokens.cache_write.mean)));
  row('– input, cache read', kfmt(a.tokens.cache_read.mean), kfmt(b.tokens.cache_read.mean), rfmt(ratio(a.tokens.cache_read.mean, b.tokens.cache_read.mean)));
  row('– output', kfmt(a.tokens.output.mean), kfmt(b.tokens.output.mean), rfmt(ratio(a.tokens.output.mean, b.tokens.output.mean)));
  {
    row('Fixed overhead per run (est.)', kfmt(a.tokens.overhead_tokens.mean), kfmt(b.tokens.overhead_tokens.mean), rfmt(ratio(a.tokens.overhead_tokens.mean, b.tokens.overhead_tokens.mean)));
    row('Research tokens per run (est.)', kfmt(a.tokens.research_tokens.mean), kfmt(b.tokens.research_tokens.mean), rfmt(ratio(a.tokens.research_tokens.mean, b.tokens.research_tokens.mean)));
  }
  row('Total tokens, all runs', kfmt(a.tokens.total_tokens.total), kfmt(b.tokens.total_tokens.total), rfmt(ratio(a.tokens.total_tokens.total, b.tokens.total_tokens.total)));
  row('Total tokens per pass (min–max)', range(a.passTotalTokens), range(b.passTotalTokens), '');
  row('Cost per run, USD (mean)', `$${fmt(a.cost.mean, 3)}`, `$${fmt(b.cost.mean, 3)}`, rfmt(ratio(a.cost.mean, b.cost.mean)));
  row('Tool calls per run (mean)', fmt(a.calls.mean, 1), fmt(b.calls.mean, 1), rfmt(ratio(a.calls.mean, b.calls.mean)));
  row('Wall time per run, s (mean)', fmt(a.wall_s.mean, 0), fmt(b.wall_s.mean, 0), rfmt(ratio(a.wall_s.mean, b.wall_s.mean)));
  return out.join('\n');
}

function headlineMarkdown(s) {
  const out = [];
  out.push(`#### All ${questions.length} questions (${s.byArm[A].runs} runs per side)`, '', headlineTable(s.byArm), '');
  for (const surf of ['github', 'local']) {
    const sl = s.slices[`surface:${surf}`];
    if (!sl) continue;
    out.push(`#### ${surf === 'github' ? 'GitHub' : 'Local'} questions (${sl.n} questions, ${sl[A].runs} runs per side)`, '', headlineTable(sl), '');
  }
  return out.join('\n');
}

function overheadMarkdown(s) {
  const a = s.byArm[A]; const b = s.byArm[B];
  return [
    'The first model request of every run carries a fixed prefix: Claude Code\'s system prompt, the tool definitions and, for Octocode, the MCP server instructions, followed by the question. Every later request in the run re-reads that prefix, almost always from the prompt cache. The table measures the prefix from each run\'s own first request, so it reflects exactly what the solver saw.',
    '',
    `| | ${L(A)} | ${L(B)} | Difference |`, '|---|---:|---:|---:|',
    `| Input of the first request (median over runs) | ${kfmt(a.firstCallInput.median)} | ${kfmt(b.firstCallInput.median)} | ${kfmt(a.firstCallInput.median - b.firstCallInput.median)} |`,
    `| Model requests per run (mean) | ${fmt(a.apiCalls.mean, 1)} | ${fmt(b.apiCalls.mean, 1)} | |`,
    `| Fixed overhead per run (first-request input × requests) | ${kfmt(a.tokens.overhead_tokens.mean)} | ${kfmt(b.tokens.overhead_tokens.mean)} | ${rfmt(ratio(a.tokens.overhead_tokens.mean, b.tokens.overhead_tokens.mean))} |`,
    `| Research tokens per run (total − overhead) | ${kfmt(a.tokens.research_tokens.mean)} | ${kfmt(b.tokens.research_tokens.mean)} | ${rfmt(ratio(a.tokens.research_tokens.mean, b.tokens.research_tokens.mean))} |`,
    `| Share of total tokens that is fixed overhead | ${pct(a.tokens.overhead_tokens.total, a.tokens.total_tokens.total)} | ${pct(b.tokens.overhead_tokens.total, b.tokens.total_tokens.total)} | |`,
    '',
    'Research tokens are everything above the fixed prefix: tool results, and the model\'s own earlier turns re-read on each request. The prefix includes the question itself (a few hundred tokens, the same in both arms). Cached prefix tokens are billed at a fraction of the fresh-input price, so the overhead weighs less in cost than in token counts.',
  ].join('\n');
}

function perQuestionTokenMarkdown(s) {
  const out = [];
  out.push(`| Question | Type | ${L(A)} tokens | ${L(B)} tokens | Ratio | ${L(A)} correct | ${L(B)} correct |`, '|---|---|---:|---:|---:|:-:|:-:|');
  for (const q of s.perQuestion) {
    out.push(`| [${q.qid}](../packages/octocode-benchmark/eval/QUESTIONS.md#${q.qid.toLowerCase()}) | ${q.surface} · ${q.category} | ${kfmt(q[A].meanTotalTokens)} | ${kfmt(q[B].meanTotalTokens)} | ${rfmt(q.tokenRatio)} | ${q[A].verdicts.map(mark).join('')} | ${q[B].verdicts.map(mark).join('')} |`);
  }
  const t = s.tokenRatios;
  out.push('');
  out.push(`Tokens are the mean total per run over ${manifest.passes} passes. Marks show each pass: ✓ correct, ½ partial, ✗ wrong, ? unresolved. Ratio below 1 means Octocode used fewer tokens.`);
  out.push('');
  out.push(`| Token ratio across ${t.questions} questions | Value |`, '|---|---:|');
  out.push(`| Median of per-question ratios | ${rfmt(t.median)} |`);
  out.push(`| Mean of per-question ratios | ${rfmt(t.mean)} |`);
  out.push(`| Geometric mean of per-question ratios | ${rfmt(t.geomean)} |`);
  out.push(`| Ratio of summed tokens | ${rfmt(t.ofSums)} |`);
  out.push(`| Questions where Octocode used fewer tokens | ${t.octocodeFewer} of ${t.questions} |`);
  out.push(`| Median / mean of per-question research-token ratios | ${rfmt(t.researchMedian)} / ${rfmt(t.researchMean)} |`);
  return out.join('\n');
}

function correctnessMarkdown(s) {
  const out = [];
  out.push(`| Metric | ${L(A)} | ${L(B)} |`, '|---|---:|---:|');
  const row = (name, f) => out.push(`| ${name} | ${f(s.byArm[A])} | ${f(s.byArm[B])} |`);
  row('Correct / partial / wrong', (x) => `${x.correct} / ${x.partial} / ${x.wrong}`);
  row('Correct rate', (x) => pct(x.correct, x.judged));
  row('Mean score (0–3)', (x) => fmt(x.meanScore));
  row('Correct answers per pass', (x) => x.passCorrect.join(', '));
  row('Mean score per pass', (x) => x.passMeanScores.map((v) => fmt(v)).join(', '));
  row('Pass-to-pass score spread (max − min)', (x) => fmt(x.passScoreSpread));
  row('Unresolved or unjudged (excluded)', (x) => String(x.unresolved + x.unjudged));
  row('Turns per run (mean)', (x) => fmt(x.turns.mean, 1));
  row('Tool errors, all runs', (x) => String(x.toolErrors));
  row('Denied tool calls, all runs', (x) => String(x.denials));
  row('Run status', (x) => Object.entries(x.statuses).map(([k, v]) => `${k} ${v}`).join(', '));
  out.push('');
  out.push('By question type:', '');
  out.push(`| Type | Questions | ${L(A)} score | ${L(B)} score | ${L(A)} correct | ${L(B)} correct | Token ratio |`, '|---|---:|---:|---:|---:|---:|---:|');
  for (const sl of Object.values(s.slices).filter((x) => x.key === 'category')) {
    out.push(`| ${sl.value} | ${sl.n} | ${fmt(sl[A].meanScore)} | ${fmt(sl[B].meanScore)} | ${sl[A].correct}/${sl[A].judged} | ${sl[B].correct}/${sl[B].judged} | ${rfmt(ratio(sl[A].tokens.total_tokens.mean, sl[B].tokens.total_tokens.mean))} |`);
  }
  out.push('');
  out.push('Per question (mean over passes):', '');
  out.push(`| Question | ${L(A)} score | ${L(B)} score | ${L(A)} calls | ${L(B)} calls | ${L(A)} time (s) | ${L(B)} time (s) | ${L(A)} cost | ${L(B)} cost |`, '|---|---:|---:|---:|---:|---:|---:|---:|---:|');
  for (const q of s.perQuestion) {
    out.push(`| ${q.qid} | ${fmt(q[A].meanScore)} | ${fmt(q[B].meanScore)} | ${fmt(q[A].meanCalls, 1)} | ${fmt(q[B].meanCalls, 1)} | ${fmt(q[A].meanWall, 0)} | ${fmt(q[B].meanWall, 0)} | $${fmt(q[A].meanCost, 3)} | $${fmt(q[B].meanCost, 3)} |`);
  }
  return out.join('\n');
}

function judgeMarkdown(s) {
  const j = s.judge;
  return [
    `- Pairs judged: ${j.pairs} (${j.calls} judge calls, ${j.parseFailures} unparseable after one retry).`,
    `- Both orders gave the same verdict for both answers: ${j.orderAgree} of ${j.pairs} (${pct(j.orderAgree, j.pairs)}). Same scores: ${j.firstPairScoreAgree} (${pct(j.firstPairScoreAgree, j.pairs)}). Same preferred answer: ${j.firstPairPreferredAgree} (${pct(j.firstPairPreferredAgree, j.pairs)}).`,
    `- Tie-break calls: ${j.tiebreaks}. Verdicts still unresolved after the tie-break (excluded from correctness totals): ${j.unresolvedArmVerdicts}.`,
    `- Preferred answer across all judge calls: ${ARMS.map((a) => `${L(a)} ${j.preferred[a]}`).join(', ')}, tie ${j.preferred.tie}.`,
    `- Judge cost: $${fmt(j.cost)}.`,
  ].join('\n');
}

function findingsMarkdown(s) {
  const c = s.comparison;
  const cl = s.clasify;
  const list = (ids) => (ids.length ? ids.join(', ') : 'none');
  return [
    `- Score: across ${c.questionsCompared} questions, the mean per-question score difference (with minus without Octocode) is ${fmt(c.meanDelta)} on the 0–3 scale.`,
    `  - Octocode scored at least 0.5 higher on: ${list(c.octocodeBetter)}.`,
    `  - rg + gh scored at least 0.5 higher on: ${list(c.rgBetter)}.`,
    `  - Within 0.5 of each other: ${list(c.similar)}.`,
    `- Tokens: the median per-question ratio is ${rfmt(s.tokenRatios.median)}; Octocode used fewer tokens on ${s.tokenRatios.octocodeFewer} of ${s.tokenRatios.questions} questions.`,
    '',
    '**clasify usage.** ' + (cl.runsWithClasify
      ? `The Octocode agent called clasify in ${cl.runsWithClasify} of ${cl.runs} runs (${cl.totalCalls} calls in total), on ${cl.questions.join(', ')}. Those runs: ${cl.outcomeWith.correct} correct, ${cl.outcomeWith.partial} partial, ${cl.outcomeWith.wrong} wrong, mean score ${fmt(cl.outcomeWith.meanScore)}. Octocode runs without clasify: ${cl.outcomeWithout.correct} correct, ${cl.outcomeWithout.partial} partial, ${cl.outcomeWithout.wrong} wrong, mean score ${fmt(cl.outcomeWithout.meanScore)}. The agent chose when to call it, so the two groups differ in question mix; this is not a controlled comparison.`
      : `The Octocode agent had clasify available but did not call it in any of the ${cl.runs} runs.`),
  ].join('\n');
}

const sections = {
  HEADLINE: headlineMarkdown(summary),
  OVERHEAD: overheadMarkdown(summary),
  PER_QUESTION_TOKENS: perQuestionTokenMarkdown(summary),
  CORRECTNESS: correctnessMarkdown(summary),
  JUDGE: judgeMarkdown(summary),
  FINDINGS: findingsMarkdown(summary),
};
const report = `# Run ${runId}\n\n## Tokens with and without Octocode\n\n${sections.HEADLINE}\n## Fixed overhead\n\n${sections.OVERHEAD}\n\n## Per-question tokens\n\n${sections.PER_QUESTION_TOKENS}\n\n## Correctness\n\n${sections.CORRECTNESS}\n\n## Judge agreement\n\n${sections.JUDGE}\n\n## Findings\n\n${sections.FINDINGS}\n`;
fs.writeFileSync(path.join(runDir, 'report.md'), report);
console.log(report);

if (args['write-docs']) {
  const tpl = fs.readFileSync(path.join(EVAL_DIR, 'BENCHMARKS.template.md'), 'utf8');
  const done = runs.filter((r) => !r.missing);
  const fill = {
    ...sections,
    RUN_ID: runId,
    RUN_DATE: manifest.createdAt.slice(0, 10),
    QUESTION_COUNT: String(questions.length),
    GITHUB_COUNT: String(questions.filter((q) => q.surface === 'github').length),
    LOCAL_COUNT: String(questions.filter((q) => q.surface === 'local').length),
    PASSES: String(manifest.passes),
    SOLVER_RUNS: String(done.length),
    MAX_TURNS: String(manifest.maxTurns),
    TIMEOUT_MIN: String(Math.round(manifest.timeoutMs / 60000)),
    CLAUDE_VERSION: manifest.claudeVersion,
    MODEL: manifest.model,
    ISOLATION_FAILURES: String(isolation.failures.length),
    DENIED_RG: String(isolation.deniedCalls['rg-gh'] ?? 0),
    DENIED_OCTOCODE: String(isolation.deniedCalls.octocode ?? 0),
    CORPUS: manifest.corpus.map((c) => `\`${c.repo}\` at \`${c.sha.slice(0, 12)}\``).join(' and '),
    MEDIAN_RATIO: rfmt(tokenRatios.median),
    CORRECT_A: `${byArm[A].correct} of ${byArm[A].judged}`,
    CORRECT_B: `${byArm[B].correct} of ${byArm[B].judged}`,
    TOKENS_A: kfmt(byArm[A].tokens.total_tokens.mean),
    TOKENS_B: kfmt(byArm[B].tokens.total_tokens.mean),
    COST_A: `$${fmt(byArm[A].cost.mean, 3)}`,
    COST_B: `$${fmt(byArm[B].cost.mean, 3)}`,
    OVERHEAD_A: kfmt(byArm[A].firstCallInput.median),
    OVERHEAD_B: kfmt(byArm[B].firstCallInput.median),
    JUDGE_COST: `$${fmt(judge.cost)}`,
    SOLVER_COST: `$${fmt(byArm[A].cost.total + byArm[B].cost.total)}`,
  };
  const page = tpl.replace(/\{\{(\w+)\}\}/g, (m, k) => {
    if (!(k in fill)) throw new Error(`unknown placeholder ${k}`);
    return fill[k];
  });
  fs.writeFileSync(path.join(REPO_ROOT, 'docs/BENCHMARKS.md'), page);
  console.log('wrote docs/BENCHMARKS.md');
}
