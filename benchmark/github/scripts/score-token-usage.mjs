#!/usr/bin/env node
// score-token-usage.mjs — Deterministic token/character usage scorer.
//
// This script does NOT judge semantic answer quality. The judge supplies quality
// scores (0..3) in a JSON file, and this script combines them with metered
// character usage from <run>/summary.json. Winner axis: quality per measured
// character budget, with Octocode MCP init/context amortized per question.
//
// Quality file shape:
// {
//   "octocode": { "1": 3, "2": 2, ... },
//   "gh":       { "1": 3, "2": 1, ... },
//   "drift":   [5, 12],              // optional: excluded from wins/totals
//   "exclude": [9]                   // optional: bad questions excluded
// }
//
// Usage:
//   node benchmark/github/scripts/score-token-usage.mjs \
//     benchmark/github/output/octocode benchmark/github/output/gh quality.json
import { readFileSync, existsSync } from 'fs';
import { join, basename } from 'path';

const [octoRun, ghRun, qualityPath] = process.argv.slice(2);
if (!octoRun || !ghRun || !qualityPath || !existsSync(octoRun) || !existsSync(ghRun) || !existsSync(qualityPath)) {
  console.error('Usage: score-token-usage.mjs <octocode-run-dir> <gh-run-dir> <quality-scores.json>');
  process.exit(1);
}

const readSummary = run => {
  const p = join(run, 'summary.json');
  if (!existsSync(p)) {
    console.error(`score-token-usage: missing ${p}; run finalize.mjs first`);
    process.exit(1);
  }
  return JSON.parse(readFileSync(p, 'utf8'));
};

const octo = readSummary(octoRun);
const gh = readSummary(ghRun);
const quality = JSON.parse(readFileSync(qualityPath, 'utf8'));
const drift = new Set((quality.drift ?? []).map(Number));
const exclude = new Set((quality.exclude ?? []).map(Number));

const initChars = s => (s.mcp_init?.in_chars ?? 0) + (s.mcp_init?.out_chars ?? 0);
const comparableQs = octo.per_q
  .filter(q => !q.missing)
  .map(q => q.q)
  .filter(q => gh.per_q.some(g => !g.missing && g.q === q))
  .filter(q => !drift.has(q) && !exclude.has(q));
const amortizedInit = comparableQs.length > 0 ? initChars(octo) / comparableQs.length : 0;
const fmt = n => Number(n).toLocaleString('en', { maximumFractionDigits: 3 });
const score = (q, chars) => q <= 0 ? 0 : q / (chars / 1000);
const qScore = (agent, q) => {
  const v = quality[agent]?.[String(q)] ?? quality[agent]?.[q];
  if (!Number.isFinite(v) || v < 0 || v > 3) {
    console.error(`score-token-usage: missing/invalid quality.${agent}.${q}`);
    process.exit(2);
  }
  return Number(v);
};

const ghByQ = new Map(gh.per_q.filter(q => !q.missing).map(q => [q.q, q]));
const rows = [];
let wins = { octocode: 0, gh: 0, tie: 0 };
let sums = { octocodeQuality: 0, ghQuality: 0, octocodeChars: 0, ghChars: 0 };

for (const oq of octo.per_q.filter(q => !q.missing)) {
  const q = oq.q;
  const gq = ghByQ.get(q);
  if (!gq) continue;
  const oqv = qScore('octocode', q);
  const gqv = qScore('gh', q);
  const oc = oq.in_chars + oq.out_chars + amortizedInit;
  const gc = gq.in_chars + gq.out_chars;
  const os = score(oqv, oc);
  const gs = score(gqv, gc);
  const isDrift = drift.has(q);
  const isExcluded = exclude.has(q);
  let winner = '—';
  if (!isDrift && !isExcluded) {
    const max = Math.max(os, gs);
    winner = max > 0 && Math.abs(os - gs) / max <= 0.05 ? 'tie' : (os > gs ? 'octocode' : 'gh');
    wins[winner]++;
    sums.octocodeQuality += oqv;
    sums.ghQuality += gqv;
    sums.octocodeChars += oc;
    sums.ghChars += gc;
  }
  rows.push({ q, drift: isDrift, excluded: isExcluded, octocodeQuality: oqv, ghQuality: gqv, octocodeChars: oc, ghChars: gc, octocodeTokenScore: os, ghTokenScore: gs, winner });
}

const totalScoreOcto = score(sums.octocodeQuality, sums.octocodeChars);
const totalScoreGh = score(sums.ghQuality, sums.ghChars);

console.log(JSON.stringify({
  runs: { octocode: basename(octoRun), gh: basename(ghRun) },
  amortized_mcp_init_chars: amortizedInit,
  rows,
  totals: {
    octocode_quality: sums.octocodeQuality,
    gh_quality: sums.ghQuality,
    octocode_effective_chars: sums.octocodeChars,
    gh_effective_chars: sums.ghChars,
    octocode_approx_tokens: sums.octocodeChars / 4,
    gh_approx_tokens: sums.ghChars / 4,
    octocode_quality_per_1k_chars: totalScoreOcto,
    gh_quality_per_1k_chars: totalScoreGh,
    winner: Math.abs(totalScoreOcto - totalScoreGh) / Math.max(totalScoreOcto, totalScoreGh, 1) <= 0.05 ? 'tie' : (totalScoreOcto > totalScoreGh ? 'octocode' : 'gh'),
    wins,
    chars_ratio_octo_over_gh: sums.ghChars ? sums.octocodeChars / sums.ghChars : null,
  },
}, null, 2));

console.error(`winner=${totalScoreOcto > totalScoreGh ? 'octocode' : totalScoreGh > totalScoreOcto ? 'gh' : 'tie'} octocode_q_per_1k=${fmt(totalScoreOcto)} gh_q_per_1k=${fmt(totalScoreGh)} chars_ratio=${fmt(sums.octocodeChars / sums.ghChars)}`);
