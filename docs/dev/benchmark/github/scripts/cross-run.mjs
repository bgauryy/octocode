#!/usr/bin/env node
// cross-run.mjs — Compute medians across multiple runs of the same agent.
// Usage: node cross-run.mjs <run_dir...>
//   e.g. node cross-run.mjs output/2026*-octocode
import { readFileSync, existsSync } from 'fs';
import { join, basename } from 'path';

const runs = process.argv.slice(2).filter(existsSync);
if (runs.length < 2) { console.error('Usage: cross-run.mjs <run_dir...> (need ≥2)'); process.exit(1); }

const med = (xs) => { const s = [...xs].sort((a, b) => a - b); const n = s.length; return n ? (n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2) : 0; };
const fmt = (n) => Math.round(n).toLocaleString('en');

const perQ = {};
for (const r of runs) {
  const out = join(r, 'output.md');
  if (!existsSync(out)) continue;
  const t = readFileSync(out, 'utf8');
  for (const m of t.matchAll(/\| Q(\d+) \| (\d+) \| ([\d,]+) \| ([\d,]+) \| ([\d,]+) \|/g)) {
    const q = +m[1];
    (perQ[q] ??= { calls: [], in: [], out: [], ms: [] });
    perQ[q].calls.push(+m[2]);
    perQ[q].in.push(+m[3].replaceAll(',', ''));
    perQ[q].out.push(+m[4].replaceAll(',', ''));
    perQ[q].ms.push(+m[5].replaceAll(',', ''));
  }
}

const slug = basename(runs[0]).split('-').pop();
const qs = Object.keys(perQ).map(Number).sort((a, b) => a - b);
const tot = { calls: 0, in: 0, out: 0, ms: 0 };

console.log(`# Cross-run median — ${slug} — n=${runs.length}\n`);
console.log('| Q | Calls (med) | In Chars (med) | Out Chars (med) | Elapsed ms (med) |');
console.log('|---|------------:|---------------:|----------------:|-----------------:|');
for (const q of qs) {
  const r = perQ[q];
  const c = med(r.calls), i = med(r.in), o = med(r.out), m = med(r.ms);
  tot.calls += c; tot.in += i; tot.out += o; tot.ms += m;
  console.log(`| Q${q} | ${c} | ${fmt(i)} | ${fmt(o)} | ${fmt(m)} |`);
}
console.log(`| **Σ** | **${tot.calls}** | **${fmt(tot.in)}** | **${fmt(tot.out)}** | **${fmt(tot.ms)}** |`);
