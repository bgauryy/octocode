#!/usr/bin/env node
// Aggregate a run into results/<run-id>/summary.json and results/<run-id>/REPORT.md.
//
//   node report.mjs --run-id <id>
import fs from 'node:fs';
import path from 'node:path';
import { RESULTS_DIR, UNIFIED_DIR, hashFile, loadQuestions, mean, median, parseArgs, readJson, writeJson } from './lib.mjs';

const args = parseArgs(process.argv.slice(2));
if (!args['run-id']) throw new Error('--run-id is required');
const runId = String(args['run-id']);
const runDir = path.join(RESULTS_DIR, runId);
const manifest = readJson(path.join(runDir, 'manifest.json'));
if (hashFile(path.join(UNIFIED_DIR, 'questions/questions.json')) !== manifest.hashes.questionsJson) throw new Error('frozen questions changed');
const workers = manifest.workers;
const questions = loadQuestions().filter((q) => manifest.questionIds.includes(q.id));
const maybe = (p) => (fs.existsSync(p) ? readJson(p) : null);

const pairs = [];
for (let i = 0; i < workers.length; i++) for (let j = i + 1; j < workers.length; j++) pairs.push([workers[i], workers[j]]);

const k = (n) => (n == null ? '—' : n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(Math.round(n)));
const f1 = (n) => (n == null ? '—' : n.toFixed(1));
const f2 = (n) => (n == null ? '—' : n.toFixed(2));
const usd = (n) => (n == null ? '—' : `$${n.toFixed(3)}`);
const weightedTokens = t => t?.weighted_tokens ?? null;
const surfaceOf = (q) => q.surface ?? ((q.repos ?? []).some((r) => r.dir) ? 'local' : 'github');
const catKey = (q) => q.category ?? surfaceOf(q);

// ---------------------------------------------------------------- per question
const rows = questions.map((q) => {
  const row = { qid: q.id, surface: surfaceOf(q), category: q.category, catKey: catKey(q), title: q.title, repo: (q.repos ?? []).map((r) => `${r.repo}@${String(r.sha).slice(0, 8)}`).join(', '), w: {} };
  const finals = pairs.map(([a, b]) => maybe(path.join(runDir, 'judge', q.id, `${a}__${b}`, 'final.json'))).filter(Boolean);
  for (const w of workers) {
    const r = maybe(path.join(runDir, 'runs', q.id, w, 'run.json'));
    if (!r || !r.valid || r.status !== 'ok' || !r.isolation?.ok || !r.tokens?.verified) throw new Error(`invalid/missing worker ${q.id}/${w}`);
    if (finals.length !== pairs.length || finals.some(f => !f.valid || f.graderErrors || !Number.isFinite(f.scores?.[w]?.quality))) throw new Error(`incomplete/invalid judgment ${q.id}/${w}`);
    const scores = finals.map((f) => f.scores?.[w]?.quality).filter((x) => x != null);
    const quality = scores.length ? mean(scores) : null;
    const t = r?.tokens;
    row.w[w] = r ? {
      status: r.status,
      quality,
      correctness: mean(finals.map((f) => f.scores?.[w]?.correctness).filter((x) => x != null)),
      completeness: mean(finals.map((f) => f.scores?.[w]?.completeness).filter((x) => x != null)),
      evidence: mean(finals.map((f) => f.scores?.[w]?.evidence).filter((x) => x != null)),
      wrongClaims: finals.flatMap((f) => f.scores?.[w]?.wrongClaims ?? []),
      totalTokens: t.total_tokens, weightedTokens: weightedTokens(t), contextTokens: t.context_tokens, overheadTokens: t.fixed_overhead_tokens,
      researchTokens: t.research_tokens, outputTokens: t.output_tokens, requests: t.requests,
      actualUsage: { input_tokens: t.input_tokens, cache_creation_input_tokens: t.cache_creation_input_tokens, cache_read_input_tokens: t.cache_read_input_tokens, output_tokens: t.output_tokens }, cacheCreationTTL: t.cache_creation ?? null, accountingSource: t.source,
      provisionalUsage: t.provisional_usage, usageReconciliation: { provisional: t.usage_gaps, modelUsage: t.model_usage_gaps },
      classificationProvider: r.classificationProvider ?? null, providerUsageStatus: r.providerUsageStatus,
      classificationAccounting: r.classificationAccounting ?? null,
      gatewayTraffic: r.gatewayTraffic ?? null, nativeGithubUsage: r.nativeGithubUsage ?? null, networkAccounting: r.networkAccounting ?? null,
      nativeCalls: r.nativeCalls ?? [], rowErrors: r.rowErrorCount ?? 0, nativeRowErrors: r.nativeRowErrorCount ?? 0,
      reflection: r.reflection ? { cost_usd: r.reflection.cost_usd, costVerified: r.reflection.costVerified, tokens: r.reflection.tokens } : null,
      firstRequestContext: t.first_request_context,
      cost: r.cost_usd, toolCalls: r.toolCallCount, toolCounts: r.toolCounts, counters: r.counters ?? {},
      wallMs: r.wallMs, numTurns: r.num_turns, toolErrors: r.toolErrorCount, denials: r.permission_denials?.length ?? 0,
      isolationOk: r.isolation?.ok ?? null, isolationProblems: r.isolation?.problems ?? [],
      efficiency: quality != null && t.total_tokens ? quality / (t.total_tokens / 10_000) : null,
      weightedEfficiency: quality != null && weightedTokens(t) ? quality / (weightedTokens(t) / 10_000) : null,
      researchEfficiency: quality != null && t.research_tokens > 0 ? quality / (t.research_tokens / 10_000) : null,
    } : null;
  }
  row.judge = finals.map((f) => ({ pair: f.pairKey, tiebreak: f.tiebreak, orderSpread: f.orderSpread, graderErrors: f.graderErrors, preferred: f.preferred, referenceIssues: f.referenceIssues, cost: f.judgeCost, model: f.model, totalTokens: f.judgeTokens, attempts: f.judgeAttempts }));
  row.ratios = Object.fromEntries(pairs.map(([a, b]) => {
    const A = row.w[a]; const B = row.w[b];
    return [`${a}/${b}`, A && B ? { total: A.totalTokens / B.totalTokens, weighted: A.weightedTokens && B.weightedTokens ? A.weightedTokens / B.weightedTokens : null, research: B.researchTokens > 0 ? A.researchTokens / B.researchTokens : null, cost: B.cost > 0 ? A.cost / B.cost : null } : null];
  }));
  return row;
});

// ---------------------------------------------------------------- aggregates
function aggregate(sel) {
  return Object.fromEntries(workers.map((w) => {
    const xs = sel.map((r) => r.w[w]).filter(Boolean);
    const sum = (key) => xs.reduce((s, x) => s + (x[key] ?? 0), 0);
    const counters = {};
    for (const x of xs) for (const [c, n] of Object.entries(x.counters ?? {})) counters[c] = (counters[c] ?? 0) + n;
    const tools = {};
    for (const x of xs) for (const [c, n] of Object.entries(x.toolCounts ?? {})) tools[c] = (tools[c] ?? 0) + n;
    const qs = xs.map((x) => x.quality).filter((x) => x != null);
    return [w, {
      runs: xs.length, ok: xs.filter((x) => x.status === 'ok').length,
      meanQuality: mean(qs), medianQuality: median(qs),
      weightedKnown: false, providerCostIncluded: false,
      classificationProvider: xs.map(x => x.classificationProvider).filter(Boolean),
      totalTokens: sum('totalTokens'), weightedTokens: null, contextTokens: sum('contextTokens'), overheadTokens: sum('overheadTokens'),
      researchTokens: sum('researchTokens'), outputTokens: sum('outputTokens'), requests: sum('requests'),
      cost: sum('cost'), toolCalls: sum('toolCalls'), wallMs: sum('wallMs'), toolErrors: sum('toolErrors'), rowErrors: sum('rowErrors'), nativeRowErrors: sum('nativeRowErrors'), denials: sum('denials'),
      meanTotalTokens: xs.length ? sum('totalTokens') / xs.length : null,
      meanResearchTokens: xs.length ? sum('researchTokens') / xs.length : null,
      efficiency: qs.length && sum('totalTokens') ? (sum('quality') / (sum('totalTokens') / 10_000)) : null,
      weightedEfficiency: null,
      counters, tools,
      isolationFailures: xs.filter((x) => x.isolationOk === false).length,
    }];
  }));
}
const totals = aggregate(rows);
const categories = [...new Set(rows.map((r) => r.catKey))].sort();
const byCategory = Object.fromEntries(categories.map((c) => [c, { n: rows.filter((r) => r.catKey === c).length, ...aggregate(rows.filter((r) => r.catKey === c)) }]));
const bySurface = Object.fromEntries(['github', 'local'].map((s) => [s, { n: rows.filter((r) => r.surface === s).length, ...aggregate(rows.filter((r) => r.surface === s)) }]));

const ratioStats = Object.fromEntries(pairs.map(([a, b]) => {
  const key = `${a}/${b}`;
  const tot = rows.map((r) => r.ratios[key]?.total).filter(Number.isFinite);
  const wtd = rows.map((r) => r.ratios[key]?.weighted).filter(Number.isFinite);
  const res = rows.map((r) => r.ratios[key]?.research).filter(Number.isFinite);
  const cost = rows.map((r) => r.ratios[key]?.cost).filter(Number.isFinite);
  const qd = rows.map((r) => (r.w[a]?.quality != null && r.w[b]?.quality != null ? r.w[a].quality - r.w[b].quality : null)).filter(Number.isFinite);
  return [key, {
    totalTokens: { mean: mean(tot), median: median(tot), n: tot.length },
    weightedTokens: { mean: mean(wtd), median: median(wtd), n: wtd.length },
    researchTokens: { mean: mean(res), median: median(res), n: res.length },
    cost: { mean: mean(cost), median: median(cost), n: cost.length },
    qualityDelta: { mean: mean(qd), median: median(qd), wins: qd.filter((d) => d > 0.5).length, losses: qd.filter((d) => d < -0.5).length, ties: qd.filter((d) => Math.abs(d) <= 0.5).length },
  }];
}));

// Judge agreement: order-swap spread per worker, tie-breaks, preferred consistency.
const judgeRows = rows.flatMap((r) => r.judge);
const spreads = judgeRows.flatMap((j) => Object.values(j.orderSpread ?? {})).filter(Number.isFinite);
const consistentPref = judgeRows.filter((j) => j.preferred?.length >= 2 && j.preferred[0] === j.preferred[1]).length;
const judgeAgreement = {
  pairsJudged: judgeRows.length,
  tiebreaks: judgeRows.filter((j) => j.tiebreak).length,
  graderErrors: judgeRows.reduce((s, j) => s + (j.graderErrors ?? 0), 0),
  meanOrderSpread: mean(spreads), maxOrderSpread: spreads.length ? Math.max(...spreads) : null,
  withinOnePoint: spreads.filter((s) => s <= 1).length, withinTwoPoints: spreads.filter((s) => s <= 2).length, spreads: spreads.length,
  preferredConsistentAcrossOrders: consistentPref,
  referenceIssues: rows.flatMap((r) => r.judge.flatMap((j) => (j.referenceIssues ?? []).map((x) => ({ qid: r.qid, issue: String(x).slice(0, 300) })))),
  cost: judgeRows.reduce((s, j) => s + (j.cost ?? 0), 0),
  totalTokens: judgeRows.reduce((s, j) => s + (j.totalTokens ?? 0), 0),
};

// Probes: fixed overhead ("reply OK") and isolation.
const probes = Object.fromEntries(workers.map((w) => {
  const o = maybe(path.join(runDir, 'probes', w, 'overhead', 'probe.json'));
  const i = maybe(path.join(runDir, 'probes', w, 'isolation', 'probe.json'));
  return [w, {
    fixedOverheadTokens: o?.tokens?.first_request_context ?? null, overheadCost: o?.cost_usd ?? null,
    offeredTools: o?.isolation?.offeredTools?.length ?? null, mcpServers: o?.isolation?.mcpServers ?? null,
    isolationOk: i?.isolation?.ok ?? null, isolationProblems: i?.isolation?.problems ?? null, leak: i?.leak ?? null,
    isolationAnswer: i?.answer ? String(i.answer).slice(0, 1200) : null, isolationDenials: i?.permission_denials ?? null,
    cost: (o?.cost_usd ?? 0) + (i?.cost_usd ?? 0), tokens: (o?.tokens?.total_tokens ?? 0) + (i?.tokens?.total_tokens ?? 0),
  }];
}));

const workerCost = Object.values(totals).reduce((s, t) => s + t.cost, 0);
const reflectionCost = rows.reduce((sum, r) => sum + Object.values(r.w).reduce((s, w) => s + (w.reflection?.cost_usd ?? 0), 0), 0);
const probeCost = Object.values(probes).reduce((s, p) => s + p.cost, 0);
const synthesis = workers.map(w => maybe(path.join(runDir, 'reflections', w, 'synthesis.json'))).filter(Boolean);
const synthesisCost = synthesis.reduce((s, r) => s + (r.cost_usd ?? 0), 0);
const summary = {
  runId, generatedAt: new Date().toISOString(), manifest: { createdAt: manifest.createdAt, model: manifest.model, claudeVersion: manifest.claudeVersion, build: manifest.build, hashes: { harness: manifest.hashes.harness, questionsJson: manifest.hashes.questionsJson, workers: manifest.hashes.workers } },
  workers, questionCount: questions.length, totals, bySurface, byCategory, ratioStats, judgeAgreement, probes,
  cost: { workers: workerCost, judge: judgeAgreement.cost, researchAndJudge: workerCost + judgeAgreement.cost, reflection: reflectionCost, probes: probeCost, reflectionSynthesis: synthesisCost, total: workerCost + judgeAgreement.cost + reflectionCost + probeCost + synthesisCost, classifierProvider: null, systemTotalVerified: false },
  verdict: 'DIAGNOSTIC: complete valid pairs; judge calibration and release thresholds require a separate acceptance decision',
  rows,
};
writeJson(path.join(runDir, 'summary.json'), summary);

// ---------------------------------------------------------------- REPORT.md
const L = [];
L.push(`# Unified benchmark — run \`${runId}\``, '');
L.push(`Workers: ${workers.map((w) => `\`${w}\``).join(', ')} · model ${manifest.model} · ${questions.length} questions × 1 pass · judge ${[...new Set(rows.flatMap(r => r.judge.map(j => j.model)).filter(Boolean))].join(', ')} (blinded X/Y, both orders, tie-break when spread > 2).`);
L.push(`Build: MCP dist sha256 \`${String(manifest.build?.mcpServerDist).slice(0, 12)}\` · Claude Code ${manifest.claudeVersion}.`, '');
L.push('Quality 0–10 = correctness 0–5 + completeness 0–3 + evidence 0–2. Tokens: total = all input kinds + output; research is an estimate: total − (first-request context × requests). Weighted tokens are unknown without a frozen model tariff and verified cache TTL. Cost and tokens cover the worker Claude session; classifier provider totals are separate/unknown. Efficiency = quality per 10k total tokens; weighted efficiency = quality per 10k weighted tokens.', '');

L.push('## Totals', '');
L.push('| worker | mean quality | median quality | total tokens | weighted tokens | research tokens | output tokens | requests | tool calls | cost | time | efficiency | weighted efficiency |', '|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|');
for (const w of workers) { const t = totals[w]; L.push(`| ${w} | ${f2(t.meanQuality)} | ${f1(t.medianQuality)} | ${k(t.totalTokens)} | ${k(t.weightedTokens)} | ${k(t.researchTokens)} | ${k(t.outputTokens)} | ${t.requests} | ${t.toolCalls} | $${t.cost.toFixed(2)} | ${(t.wallMs / 60000).toFixed(1)} min | ${f2(t.efficiency)} | ${f2(t.weightedEfficiency)} |`); }
L.push('');
for (const [key, s] of Object.entries(ratioStats)) {
  L.push(`Per-question ratio ${key}: total tokens mean ${f2(s.totalTokens.mean)}× / median ${f2(s.totalTokens.median)}×; weighted tokens mean ${f2(s.weightedTokens.mean)}× / median ${f2(s.weightedTokens.median)}×; research tokens mean ${f2(s.researchTokens.mean)}× / median ${f2(s.researchTokens.median)}×; cost mean ${f2(s.cost.mean)}× / median ${f2(s.cost.median)}×. Quality delta (${key.split('/')[0]} − ${key.split('/')[1]}): mean ${f2(s.qualityDelta.mean)}, wins/ties/losses ${s.qualityDelta.wins}/${s.qualityDelta.ties}/${s.qualityDelta.losses}.`);
}
L.push('');

L.push('## Fixed overhead ("reply OK" probe)', '');
L.push('| worker | first-request context | tools offered | isolation probe |', '|---|--:|--:|---|');
for (const w of workers) { const p = probes[w]; L.push(`| ${w} | ${k(p.fixedOverheadTokens)} | ${p.offeredTools ?? '—'} | ${p.isolationOk == null ? 'not run' : p.isolationOk ? 'pass' : 'FAIL: ' + (p.isolationProblems ?? []).join('; ') + (p.leak ? ' ' + JSON.stringify(p.leak) : '')} |`); }
L.push('');

L.push('## By category', '');
L.push(`| category | n | ${workers.map((w) => `${w} quality | ${w} tokens | ${w} research`).join(' | ')} |`, `|---|--:|${workers.map(() => '--:|--:|--:').join('|')}|`);
for (const [c, v] of [...Object.entries(bySurface), ...Object.entries(byCategory)]) {
  L.push(`| ${c} | ${v.n} | ${workers.map((w) => `${f2(v[w].meanQuality)} | ${k(v[w].meanTotalTokens)} | ${k(v[w].meanResearchTokens)}`).join(' | ')} |`);
}
L.push('', 'Token columns are per-question means.', '');

L.push('## Per question', '');
const pair = (r, fn) => workers.map((w) => (r.w[w] ? fn(r.w[w]) : '—')).join(' / ');
L.push(`Each cell shows ${workers.join(' / ')}.`, '');
L.push('| q | category | quality | total tokens | weighted tokens | research tokens | tool calls | time | cost | efficiency |', '|---|---|--:|--:|--:|--:|--:|--:|--:|--:|');
for (const r of rows) {
  const q = pair(r, (x) => `${f1(x.quality)}${x.status === 'ok' ? '' : ` (${x.status})`}`);
  L.push(`| ${r.qid} | ${r.catKey} | ${q} | ${pair(r, (x) => k(x.totalTokens))} | ${pair(r, (x) => k(x.weightedTokens))} | ${pair(r, (x) => k(x.researchTokens))} | ${pair(r, (x) => String(x.toolCalls))} | ${pair(r, (x) => `${(x.wallMs / 1000).toFixed(0)}s`)} | ${pair(r, (x) => usd(x.cost))} | ${pair(r, (x) => f2(x.efficiency))} |`);
}
L.push('');

L.push('## Tool usage', '');
for (const w of workers) {
  const t = totals[w];
  const tools = Object.entries(t.tools).sort((a, b) => b[1] - a[1]).map(([n, c]) => `${n.replace(/^mcp__\w+__/, '')} ${c}`).join(', ');
  L.push(`- **${w}**: ${tools || 'none'}${Object.keys(t.counters).length ? ` · counters: ${Object.entries(t.counters).map(([n, c]) => `${n} ${c}`).join(', ')}` : ''} · tool errors ${t.toolErrors} · permission denials ${t.denials}`);
}
L.push('');
const perQCounters = rows.map((r) => ({ q: r.qid, c: Object.fromEntries(workers.map((w) => [w, r.w[w]?.counters ?? {}])) })).filter((x) => Object.values(x.c).some((c) => Object.values(c).some((n) => n > 0)));
if (perQCounters.length) {
  L.push('Per-question counters (non-zero):', '');
  for (const x of perQCounters) L.push(`- ${x.q}: ${Object.entries(x.c).map(([w, c]) => Object.entries(c).filter(([, n]) => n > 0).map(([n, v]) => `${w}.${n}=${v}`).join(', ')).filter(Boolean).join('; ')}`);
  L.push('');
}

L.push('## Judge agreement', '');
const ja = judgeAgreement;
L.push(`${ja.pairsJudged} question-pairs judged; ${ja.tiebreaks} needed a tie-break; ${ja.graderErrors} grader errors. Order-swap spread per worker score: mean ${f2(ja.meanOrderSpread)}, max ${ja.maxOrderSpread ?? '—'}; ${ja.withinOnePoint}/${ja.spreads} within 1 point, ${ja.withinTwoPoints}/${ja.spreads} within 2. Preferred answer consistent across both orders: ${ja.preferredConsistentAcrossOrders}/${ja.pairsJudged}.`);
if (ja.referenceIssues.length) { L.push('', 'Reference issues raised by the judge:', ''); for (const x of ja.referenceIssues) L.push(`- ${x.qid}: ${x.issue.replace(/\n/g, ' ')}`); }
L.push('');

L.push('## Wrong claims flagged', '');
for (const r of rows) for (const w of workers) { const wc = r.w[w]?.wrongClaims ?? []; if (wc.length) L.push(`- ${r.qid} ${w}: ${wc.slice(0, 3).map((s) => String(s).replace(/\n/g, ' ').slice(0, 200)).join(' | ')}`); }
L.push('');

L.push('## Cost', '');
L.push(`Worker research $${workerCost.toFixed(2)} (${workers.map((w) => `${w} $${totals[w].cost.toFixed(2)}`).join(', ')}) · judge $${ja.cost.toFixed(2)} · reflections $${reflectionCost.toFixed(2)} · probes $${probeCost.toFixed(2)} · reflection synthesis $${synthesisCost.toFixed(2)} · reported Claude total $${summary.cost.total.toFixed(2)}. Classification provider cost and total system cost remain unknown.`, '');

const iso = rows.flatMap((r) => workers.filter((w) => r.w[w]?.isolationOk === false).map((w) => `${r.qid} ${w}: ${r.w[w].isolationProblems.join('; ')}`));
L.push('## Isolation', '', iso.length ? iso.map((x) => `- ${x}`).join('\n') : 'Every run passed its profile isolation check (offered/called tools, MCP servers, no skills, no benchmark-file access).', '');

fs.writeFileSync(path.join(runDir, 'REPORT.md'), L.join('\n'));
console.log(`wrote ${path.join(runDir, 'summary.json')} and REPORT.md`);
