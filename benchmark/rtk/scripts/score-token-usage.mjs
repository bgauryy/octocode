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
//   "rtk":      { "1": 3, "2": 1, ... },
//   "drift":   [5, 12],              // optional: excluded from wins/totals
//   "exclude": [9]                   // optional: excluded questions
// }
//
// Usage:
//   node benchmark/rtk/scripts/score-token-usage.mjs \
//     benchmark/rtk/output/octocode benchmark/rtk/output/rtk quality.json
import { readFileSync, existsSync } from 'fs';
import { join, basename } from 'path';

const [octoRun, rtkRun, qualityPath] = process.argv.slice(2);
if (!octoRun || !rtkRun || !qualityPath || !existsSync(octoRun) || !existsSync(rtkRun) || !existsSync(qualityPath)) {
  console.error('Usage: score-token-usage.mjs <octocode-run-dir> <rtk-run-dir> <quality-scores.json>');
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
const rtk = readSummary(rtkRun);
const quality = JSON.parse(readFileSync(qualityPath, 'utf8'));
const drift = new Set((quality.drift ?? []).map(Number));
const exclude = new Set((quality.exclude ?? []).map(Number));

const initChars = s => (s.mcp_init?.in_chars ?? 0) + (s.mcp_init?.out_chars ?? 0);
const comparableQs = octo.per_q
  .filter(q => !q.missing)
  .map(q => q.q)
  .filter(q => rtk.per_q.some(r => !r.missing && r.q === q))
  .filter(q => !drift.has(q) && !exclude.has(q));
const amortizedInit = comparableQs.length > 0 ? initChars(octo) / comparableQs.length : 0;
const fmt = n => Number(n).toLocaleString('en', { maximumFractionDigits: 3 });
const score = (q, chars) => q <= 0 ? 0 : q / (chars / 1000);
const winnerByScore = (octocodeScore, rtkScore) => {
  const max = Math.max(octocodeScore, rtkScore);
  if (max === 0 || Math.abs(octocodeScore - rtkScore) / max <= 0.05) return 'tie';
  return octocodeScore > rtkScore ? 'octocode' : 'rtk';
};
const qScore = (agent, q) => {
  const v = quality[agent]?.[String(q)] ?? quality[agent]?.[q];
  if (!Number.isFinite(v) || v < 0 || v > 3) {
    console.error(`score-token-usage: missing/invalid quality.${agent}.${q}`);
    process.exit(2);
  }
  return Number(v);
};

const rtkByQ = new Map(rtk.per_q.filter(q => !q.missing).map(q => [q.q, q]));
const rows = [];
const wins = { octocode: 0, rtk: 0, tie: 0 };
const sums = { octocodeQuality: 0, rtkQuality: 0, octocodeChars: 0, rtkChars: 0 };

for (const oq of octo.per_q.filter(q => !q.missing)) {
  const q = oq.q;
  const rq = rtkByQ.get(q);
  if (!rq) continue;
  const oqv = qScore('octocode', q);
  const rqv = qScore('rtk', q);
  const oc = oq.in_chars + oq.out_chars + amortizedInit;
  const rc = rq.in_chars + rq.out_chars;
  const os = score(oqv, oc);
  const rs = score(rqv, rc);
  const isDrift = drift.has(q);
  const isExcluded = exclude.has(q);
  let winner = '—';
  if (!isDrift && !isExcluded) {
    winner = winnerByScore(os, rs);
    wins[winner]++;
    sums.octocodeQuality += oqv;
    sums.rtkQuality += rqv;
    sums.octocodeChars += oc;
    sums.rtkChars += rc;
  }
  rows.push({ q, drift: isDrift, excluded: isExcluded, octocodeQuality: oqv, rtkQuality: rqv, octocodeChars: oc, rtkChars: rc, octocodeTokenScore: os, rtkTokenScore: rs, winner });
}

const totalScoreOcto = score(sums.octocodeQuality, sums.octocodeChars);
const totalScoreRtk = score(sums.rtkQuality, sums.rtkChars);
const totalWinner = winnerByScore(totalScoreOcto, totalScoreRtk);

console.log(JSON.stringify({
  runs: { octocode: basename(octoRun), rtk: basename(rtkRun) },
  amortized_mcp_init_chars: amortizedInit,
  rows,
  totals: {
    octocode_quality: sums.octocodeQuality,
    rtk_quality: sums.rtkQuality,
    octocode_effective_chars: sums.octocodeChars,
    rtk_effective_chars: sums.rtkChars,
    octocode_approx_tokens: sums.octocodeChars / 4,
    rtk_approx_tokens: sums.rtkChars / 4,
    octocode_quality_per_1k_chars: totalScoreOcto,
    rtk_quality_per_1k_chars: totalScoreRtk,
    winner: totalWinner,
    wins,
    chars_ratio_octo_over_rtk: sums.rtkChars ? sums.octocodeChars / sums.rtkChars : null,
  },
}, null, 2));

console.error(`winner=${totalWinner} octocode_q_per_1k=${fmt(totalScoreOcto)} rtk_q_per_1k=${fmt(totalScoreRtk)} chars_ratio=${fmt(sums.octocodeChars / sums.rtkChars)}`);
