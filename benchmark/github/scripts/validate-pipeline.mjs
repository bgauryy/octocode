#!/usr/bin/env node
// validate-pipeline.mjs — Assert metering-pipeline integrity across runs.
//
// What this validates: given two or more runs whose underlying log.jsonl rows
// describe the SAME tool-call sequence, the metering pipeline (mcp-meas /
// gh-meas → log.jsonl → aggregate.mjs → q<N>.json) yields byte-identical
// char counts. This is a regression check on the MEASUREMENT CODE — NOT a
// claim that LLM agents behave deterministically. Real agents vary; that
// variance is what report-variance.mjs reports.
//
// Usage:
//   node validate-pipeline.mjs <run_dir...>            # ≥2 runs, same agent
//   node validate-pipeline.mjs --strict-cmds <run...>  # also assert per-call
//                                                       cmd sequence matches
//
// Reads each run's q<N>.json. Compares calls / in_chars / out_chars across
// runs, per question. Exits 0 iff every (q, metric) tuple matches.
// Prints a diff table otherwise.
//
// Excluded from comparison (wall-clock, non-deterministic by design):
//   - elapsed_ms, tool_elapsed_ms (per-call timing)
//   - q_elapsed_ms (Q-level wall time)
//   - reasoning_ms (q - tool, derived)
import { readFileSync, existsSync, readdirSync } from 'fs';
import { join, basename } from 'path';

const args = process.argv.slice(2);
const strict = args.includes('--strict-cmds');
const runs = args.filter(a => !a.startsWith('--')).filter(existsSync);

if (runs.length < 2) {
  console.error('Usage: validate-pipeline.mjs [--strict-cmds] <run_dir...> (need ≥2)');
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

const agents = new Set();
const data = runs.map(run => {
  const slug = basename(run);
  agents.add(readAgent(run));
  const qFiles = readdirSync(run).filter(f => /^q\d+\.json$/.test(f));
  const perQ = {};
  for (const f of qFiles) {
    const q = +f.replace(/\D/g, '');
    const p = join(run, f);
    try { perQ[q] = JSON.parse(readFileSync(p, 'utf8')); }
    catch { perQ[q] = null; }
  }
  return { slug, perQ };
});

if (agents.size > 1) {
  console.error(`validate-pipeline: runs span multiple agents (${[...agents].join(', ')}) — only compare runs of the SAME agent`);
  process.exit(2);
}

const allQs = [...new Set(data.flatMap(d => Object.keys(d.perQ).map(Number)))].sort((a, b) => a - b);

const mismatches = [];
let bothMissing = 0;
for (const q of allQs) {
  const cells = data.map(d => d.perQ[q] ?? null);
  const nullCount = cells.filter(c => c === null).length;
  if (nullCount === cells.length) {
    bothMissing++;
    continue;
  }
  if (nullCount > 0) {
    mismatches.push({ q, kind: 'missing', detail: cells.map((c, i) => c === null ? `${data[i].slug}=MISSING` : `${data[i].slug}=ok`).join('  ') });
    continue;
  }
  for (const [i, c] of cells.entries()) {
    if (!Number.isFinite(c.in_chars) || !Number.isFinite(c.out_chars)) {
      mismatches.push({ q, kind: 'char_fields', detail: `${data[i].slug}=missing char fields` });
    }
  }
  for (const key of ['calls', 'in_chars', 'out_chars']) {
    const vals = cells.map(c => c[key]);
    const uniq = new Set(vals);
    if (uniq.size > 1) {
      mismatches.push({ q, kind: key, detail: vals.map((v, i) => `${data[i].slug}=${v}`).join('  ') });
    }
  }
  if (strict) {
    const seqs = cells.map(c => (c.calls_detail || []).map(d => d.cmd).join('|'));
    if (new Set(seqs).size > 1) {
      mismatches.push({ q, kind: 'cmd_sequence', detail: seqs.map((s, i) => `${data[i].slug}:[${s}]`).join('  ') });
    }
  }
}

console.log(`# Determinism check — agent=${[...agents][0]} — n=${runs.length}\n`);
console.log(`Runs:`);
for (const r of runs) console.log(`  - ${basename(r)}`);
console.log();

const compared = allQs.length - bothMissing;
if (mismatches.length === 0) {
  console.log(`✅ ${compared}/${allQs.length} questions match on calls / in_chars / out_chars${strict ? ' / cmd_sequence' : ''} (${bothMissing} unanswered in every run, skipped)`);
  process.exit(0);
}

console.log(`❌ ${mismatches.length} mismatch(es):`);
console.log();
console.log('| Q | Metric | Detail |');
console.log('|---|--------|--------|');
for (const m of mismatches) {
  console.log(`| Q${m.q} | ${m.kind} | ${m.detail} |`);
}
process.exit(1);
