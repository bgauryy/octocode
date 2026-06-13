#!/usr/bin/env node
// aggregate.mjs — Sum log.jsonl character entries for one question.
//
// Usage:
//   node aggregate.mjs <log> <q>                → "calls in_chars out_chars elapsed_ms"
//   node aggregate.mjs <log> <q> --allow-zero   → as above; 0 rows is not an error
//   node aggregate.mjs <log> <q> --json         → emit a JSON object with per-call breakdown
//
// By default this fails loudly if the log is missing OR contains zero rows for
// the question. Zero rows usually means the metering wrapper wasn't used —
// silently emitting 0/0/0/0 would corrupt the benchmark.
import { readFileSync, existsSync } from 'fs';

const args = process.argv.slice(2);
const allowZero = args.includes('--allow-zero');
const asJson = args.includes('--json');
const [log, qRaw] = args.filter(a => !a.startsWith('--'));

if (!log || !qRaw) {
  console.error('Usage: aggregate.mjs <log> <q> [--allow-zero] [--json]');
  process.exit(2);
}
const q = Number.parseInt(qRaw, 10);
if (!/^\d+$/.test(qRaw) || !Number.isFinite(q)) {
  console.error(`aggregate: invalid q: ${qRaw}`);
  process.exit(2);
}

if (!existsSync(log)) {
  console.error(`aggregate: log not found: ${log}`);
  process.exit(2);
}

const rows = readFileSync(log, 'utf8')
  .split('\n').filter(Boolean).map((l, i) => {
    try { return JSON.parse(l); }
    catch (e) { console.error(`aggregate: malformed jsonl at line ${i + 1}`); process.exit(2); }
  })
  .filter(r => r.q === q);

if (rows.length === 0 && !allowZero) {
  console.error(`aggregate: zero rows for q=${q} in ${log} — did the metering wrapper run? (use --allow-zero to silence)`);
  process.exit(3);
}

for (const [i, r] of rows.entries()) {
  if (!Number.isFinite(r.in_chars) || !Number.isFinite(r.out_chars)) {
    console.error(`aggregate: row ${i + 1} for q=${q} lacks character fields — rerun with character-based metering`);
    process.exit(4);
  }
}

const sum = (k) => rows.reduce((s, r) => s + (r[k] || 0), 0);
const out = {
  calls: rows.length,
  in_chars: sum('in_chars'),
  out_chars: sum('out_chars'),
  elapsed_ms: sum('elapsed_ms'),
  calls_detail: rows.map(r => ({ cmd: r.cmd, in_chars: r.in_chars, out_chars: r.out_chars, elapsed_ms: r.elapsed_ms, exit: r.exit })),
};

if (asJson) {
  process.stdout.write(JSON.stringify(out) + '\n');
} else {
  console.log(`${out.calls} ${out.in_chars} ${out.out_chars} ${out.elapsed_ms}`);
}
