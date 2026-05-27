#!/usr/bin/env node
// report-variance.mjs — Quantify agent variance across repeated runs.
//
// LLM agents are stochastic. The same question across multiple runs of the
// SAME agent will typically yield different research paths, different call
// counts, and different total chars. This script reports HOW MUCH variance
// there is — it does NOT pass/fail on it (that's what validate-pipeline.mjs is
// for, but only for measurement-code regressions).
//
// Per question, across N runs of the same agent, we report:
//   - calls:      min, max, median, CV (coefficient of variation = stdev/mean)
//   - in_chars:  same
//   - out_chars: same
//
// Usage:
//   node report-variance.mjs <run_dir...>         # ≥2 runs of the SAME agent
//   node report-variance.mjs --csv <run...>       # CSV output for plotting
import { readFileSync, existsSync, readdirSync } from 'fs';
import { join, basename } from 'path';

const args = process.argv.slice(2);
const asCsv = args.includes('--csv');
const runs = args.filter(a => !a.startsWith('--')).filter(existsSync);

if (runs.length < 2) {
  console.error('Usage: report-variance.mjs [--csv] <run_dir...>  (need ≥2 runs of the same agent)');
  process.exit(2);
}

const readAgent = (run) => {
  const summaryPath = join(run, 'summary.json');
  if (!existsSync(summaryPath)) return basename(run);
  try {
    return JSON.parse(readFileSync(summaryPath, 'utf8')).agent ?? basename(run);
  } catch {
    return basename(run);
  }
};

const agents = new Set(runs.map(readAgent));
if (agents.size > 1) {
  console.error(`report-variance: runs span multiple agents (${[...agents].join(', ')}) — compare runs of one agent at a time`);
  process.exit(2);
}
const agent = [...agents][0];

const data = runs.map(run => {
  const perQ = {};
  for (const f of readdirSync(run).filter(f => /^q\d+\.json$/.test(f))) {
    const q = +f.replace(/\D/g, '');
    const p = join(run, f);
    if (existsSync(p)) {
      const m = JSON.parse(readFileSync(p, 'utf8'));
      if (!Number.isFinite(m.in_chars) || !Number.isFinite(m.out_chars)) {
        console.error(`report-variance: ${p} lacks char fields — rerun with character-based metering`);
        process.exit(2);
      }
      m.tool_elapsed_ms ??= m.elapsed_ms ?? 0;
      m.q_elapsed_ms ??= 0;
      perQ[q] = m;
    }
  }
  return { slug: basename(run), perQ };
});

const allQs = [...new Set(data.flatMap(d => Object.keys(d.perQ).map(Number)))].sort((a, b) => a - b);

const stats = (xs) => {
  const n = xs.length;
  if (n === 0) return { n: 0, min: 0, max: 0, med: 0, mean: 0, stdev: 0, cv: 0 };
  const s = [...xs].sort((a, b) => a - b);
  const min = s[0], max = s[n - 1];
  const med = n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
  const mean = xs.reduce((a, b) => a + b, 0) / n;
  const variance = xs.reduce((a, b) => a + (b - mean) ** 2, 0) / n;
  const stdev = Math.sqrt(variance);
  const cv = mean > 0 ? stdev / mean : 0;
  return { n, min, max, med, mean, stdev, cv };
};

const rows = [];
for (const q of allQs) {
  const cells = data.map(d => d.perQ[q]).filter(Boolean);
  if (cells.length < 2) {
    rows.push({ q, n: cells.length, partial: true });
    continue;
  }
  rows.push({
    q,
    n: cells.length,
    calls: stats(cells.map(c => c.calls)),
    in: stats(cells.map(c => c.in_chars)),
    out: stats(cells.map(c => c.out_chars)),
    q_ms: stats(cells.map(c => c.q_elapsed_ms)),
  });
}

const fmt = (n) => Number(n).toLocaleString('en');
const cv = (v) => v >= 0.3 ? `${v.toFixed(2)} ⚠️` : v.toFixed(2);

if (asCsv) {
  console.log('q,n,calls_med,calls_cv,in_chars_med,in_chars_cv,out_chars_med,out_chars_cv,q_ms_med,q_ms_cv');
  for (const r of rows) {
    if (r.partial) { console.log(`${r.q},${r.n},,,,,,,,`); continue; }
    console.log(`${r.q},${r.n},${r.calls.med},${r.calls.cv.toFixed(3)},${r.in.med},${r.in.cv.toFixed(3)},${r.out.med},${r.out.cv.toFixed(3)},${r.q_ms.med},${r.q_ms.cv.toFixed(3)}`);
  }
  process.exit(0);
}

console.log(`# Agent variance — ${agent} — n=${runs.length}\n`);
console.log(`Runs:`);
for (const r of runs) console.log(`  - ${basename(r)}`);
console.log();
console.log(`CV = stdev/mean. ⚠️ marks CV ≥ 0.3 (unstable — prefer median + report CV, not a single-run number).\n`);
console.log(`| Q | n | calls (min/med/max, cv) | in_chars (med, cv) | out_chars (med, cv) | q_ms (med, cv) |`);
console.log(`|---|--:|------------------------|---------------------|----------------------|----------------|`);
let unstable = 0;
for (const r of rows) {
  if (r.partial) {
    console.log(`| Q${r.q} | ${r.n} | only ${r.n} run(s) | — | — | — |`);
    continue;
  }
  const u = (r.calls.cv >= 0.3 || r.in.cv >= 0.3 || r.out.cv >= 0.3 || r.q_ms.cv >= 0.3);
  if (u) unstable++;
  console.log(`| Q${r.q} | ${r.n} | ${r.calls.min}/${r.calls.med}/${r.calls.max}, ${cv(r.calls.cv)} | ${fmt(r.in.med)}, ${cv(r.in.cv)} | ${fmt(r.out.med)}, ${cv(r.out.cv)} | ${fmt(r.q_ms.med)}, ${cv(r.q_ms.cv)} |`);
}
console.log();
console.log(`Unstable questions (any metric CV ≥ 0.3): ${unstable}/${rows.filter(r => !r.partial).length}`);
console.log(`Recommendation: report MEDIAN, not mean, and disclose CV alongside any single-axis claim.`);
