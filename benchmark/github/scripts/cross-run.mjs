#!/usr/bin/env node
// cross-run.mjs — Median across multiple runs of the same agent.
//
// Reads <run>/summary.json (written by finalize.mjs).
//
// Usage: node cross-run.mjs <run_dir...>
//   e.g. node cross-run.mjs saved-runs/*/octocode
import { readFileSync, existsSync } from 'fs';
import { join, basename } from 'path';

const runs = process.argv.slice(2).filter(existsSync);
if (runs.length < 2) {
  console.error('Usage: cross-run.mjs <run_dir...> (need ≥2)');
  process.exit(1);
}

const med = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const n = s.length;
  if (!n) return 0;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
};
const fmt = (n) => Math.round(n).toLocaleString('en');

const summaries = runs.map(r => {
  const p = join(r, 'summary.json');
  if (!existsSync(p)) {
    console.error(`cross-run: missing ${p} — run finalize.mjs on ${r} first`);
    process.exit(1);
  }
  return JSON.parse(readFileSync(p, 'utf8'));
});

const agents = [...new Set(summaries.map((s, i) => s.agent ?? basename(runs[i])))];
if (agents.length > 1) {
  console.warn(`cross-run: WARNING — mixing agents in one comparison: ${agents.join(', ')}`);
}

const perQ = {};
for (const s of summaries) {
  for (const r of s.per_q) {
    if (r.missing) continue;
    if (!Number.isFinite(r.in_chars) || !Number.isFinite(r.out_chars)) {
      console.error('cross-run: summary lacks char fields — rerun finalize.mjs after character-based metering');
      process.exit(2);
    }
    (perQ[r.q] ??= { calls: [], in: [], out: [], tool_ms: [], q_ms: [], reason_ms: [] });
    perQ[r.q].calls.push(r.calls);
    perQ[r.q].in.push(r.in_chars);
    perQ[r.q].out.push(r.out_chars);
    perQ[r.q].tool_ms.push(r.tool_elapsed_ms ?? 0);
    perQ[r.q].q_ms.push(r.q_elapsed_ms ?? 0);
    perQ[r.q].reason_ms.push(r.reasoning_ms ?? 0);
  }
}

const qs = Object.keys(perQ).map(Number).sort((a, b) => a - b);
const tot = { calls: 0, in: 0, out: 0, tool_ms: 0, q_ms: 0, reason_ms: 0 };

console.log(`# Cross-run median — ${agents[0]} — n=${runs.length}\n`);
console.log('| Q | Calls | In Chars | Out Chars | Tool ms | Q wall ms | Reasoning ms |');
console.log('|---|------:|----------:|-----------:|--------:|----------:|-------------:|');
for (const q of qs) {
  const r = perQ[q];
  const c = med(r.calls), i = med(r.in), o = med(r.out);
  const tm = med(r.tool_ms), qm = med(r.q_ms), rm = med(r.reason_ms);
  tot.calls += c; tot.in += i; tot.out += o;
  tot.tool_ms += tm; tot.q_ms += qm; tot.reason_ms += rm;
  console.log(`| Q${q} | ${c} | ${fmt(i)} | ${fmt(o)} | ${fmt(tm)} | ${fmt(qm)} | ${fmt(rm)} |`);
}
console.log(`| **Σ med** | **${tot.calls}** | **${fmt(tot.in)}** | **${fmt(tot.out)}** | **${fmt(tot.tool_ms)}** | **${fmt(tot.q_ms)}** | **${fmt(tot.reason_ms)}** |`);
