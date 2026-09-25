#!/usr/bin/env node
// Compare two probe-surfaces results.json files: per case and surface, report
// class/check changes and output-size deltas; flag regressions. Exit 1 on regression.
import { readFileSync } from 'node:fs';

const [basePath, candPath] = process.argv.slice(2).filter((a) => !a.startsWith('--'));
if (!basePath || !candPath || process.argv.includes('--help')) {
  console.log(`compare-runs — baseline vs candidate probe results

  node scripts/compare-runs.mjs <baseline/results.json> <candidate/results.json> [--size-threshold 0.1]

Regression = a check that passed now fails, a class that became FAIL_*, or output grew
beyond the threshold (default 10%) while check and class stayed the same. Exit 1 when any regression exists.`);
  process.exit(basePath && candPath ? 0 : 2);
}
const ti = process.argv.indexOf('--size-threshold');
const threshold = ti >= 0 ? Number(process.argv[ti + 1]) : 0.1;
const load = (p) => JSON.parse(readFileSync(p, 'utf8'));
const base = load(basePath);
const cand = load(candPath);
const byId = new Map(base.results.map((r) => [r.id, r]));

let regressions = 0;
const lines = [];
for (const c of cand.results) {
  const b = byId.get(c.id);
  if (!b) { lines.push(`+ ${c.id} (new case)`); continue; }
  for (const s of ['native', 'node', 'mcp']) {
    if (!b[s] || !c[s]) continue;
    const notes = [];
    let bad = false;
    if (b[s].check && !c[s].check) { notes.push('check pass→FAIL'); bad = true; }
    if (!b[s].check && c[s].check) notes.push('check FAIL→pass');
    if (b[s].cls !== c[s].cls) { notes.push(`${b[s].cls}→${c[s].cls}`); if (c[s].cls.startsWith('FAIL')) bad = true; }
    const delta = (c[s].chars - b[s].chars) / Math.max(1, b[s].chars);
    if (Math.abs(delta) >= 0.05) notes.push(`size ${b[s].chars}→${c[s].chars} (${delta > 0 ? '+' : ''}${Math.round(delta * 100)}%)`);
    // Growth counts only when the outcome is unchanged; a newly passing check or a
    // class change (e.g. an error that became a result) legitimately changes size.
    const sameOutcome = b[s].check === c[s].check && b[s].cls === c[s].cls;
    if (sameOutcome && delta > threshold) bad = true;
    if (notes.length) lines.push(`${bad ? '!' : ' '} ${c.id.padEnd(15)} ${s.padEnd(6)} ${notes.join('; ')}`);
    if (bad) regressions++;
  }
}
const cat = (x) => (x.catalog && !x.catalog.startError ? `${x.catalog.totalChars} chars / ${x.catalog.toolCount} tools` : `unavailable (${x.catalog?.startError?.slice(0, 80) ?? 'not run'})`);
console.log(`MCP catalog: ${cat(base)} → ${cat(cand)}`);
console.log(lines.join('\n') || 'no per-case changes');
console.log(`\nregressions: ${regressions}`);
process.exit(regressions ? 1 : 0);
