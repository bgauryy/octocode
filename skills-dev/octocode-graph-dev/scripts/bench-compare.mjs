#!/usr/bin/env node
// Compare two graph-bench.mjs runs per grammar. Exit 1 when a row loses
// precision or starts failing to ingest; recall and timing changes are reported.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HELP = `Usage: bench-compare.mjs <before.json> <after.json> [--json]

Prints one Markdown row per grammar. Exit 0 no regression, 1 regression, 2 bad input.`;

const metric = (row) => ({
  ok: Boolean(row) && !row.ingest?.error && row.ingest?.exit === 0,
  recall: row?.recall?.recall ?? null,
  precision: row?.recall?.precision ?? null,
  link: row?.ingest?.callLinkRate ?? null,
  ms: row?.ingest?.ms ?? null,
  // Older runs kept 5-file samples; only complete lists can name newly missed files.
  missed: row?.recall?.missed ?? null,
  extra: row?.recall?.extra ?? null,
});

export function compare(before, after) {
  const langs = [...new Set([...before, ...after].map((row) => row.lang))].sort();
  return langs.map((lang) => {
    const b = metric(before.find((row) => row.lang === lang));
    const a = metric(after.find((row) => row.lang === lang));
    const notes = [];
    if (!before.some((row) => row.lang === lang)) notes.push('new row');
    if (!after.some((row) => row.lang === lang)) notes.push('not run');
    else if (b.ok && !a.ok) notes.push('ingest failed');
    if (b.precision !== null && a.precision !== null && a.precision < b.precision) notes.push('precision down');
    if (b.recall !== null && a.recall !== null && a.recall < b.recall) notes.push('recall down');
    const regression = notes.includes('ingest failed') || notes.includes('precision down');
    const added = (x, y) => (x && y ? y.filter((f) => !x.includes(f)) : []);
    const newMissed = added(b.missed, a.missed);
    const newExtra = added(b.extra, a.extra);
    return { lang, before: b, after: a, notes, regression, newMissed, newExtra };
  });
}

const fmt = (b, a) => (b === a ? String(a ?? '-') : `${b ?? '-'} → ${a ?? '-'}`);

export function markdown(rows) {
  const lines = [
    '| Language | Recall | Precision | Link rate | Ingest ms | Notes |',
    '|---|---|---|---|---|---|',
    ...rows.map((r) => `| ${r.lang} | ${fmt(r.before.recall, r.after.recall)} | ${fmt(r.before.precision, r.after.precision)} | ${fmt(r.before.link, r.after.link)} | ${fmt(r.before.ms, r.after.ms)} | ${[...r.notes, ...r.newMissed.map((f) => `newly missed ${f}`), ...r.newExtra.map((f) => `new extra ${f}`)].join('; ')} |`),
  ];
  return lines.join('\n');
}

function load(file) {
  const data = JSON.parse(readFileSync(file, 'utf8'));
  if (!Array.isArray(data)) throw new Error(`${file}: expected the graph-bench.mjs result array`);
  return data;
}

function main(argv) {
  if (argv.includes('--help') || argv.includes('-h')) return console.log(HELP), 0;
  const files = argv.filter((arg) => arg !== '--json');
  if (files.length !== 2) throw new Error(HELP);
  const rows = compare(load(resolve(files[0])), load(resolve(files[1])));
  console.log(argv.includes('--json') ? JSON.stringify(rows, null, 1) : markdown(rows));
  return rows.some((row) => row.regression) ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (e) {
    console.error(e.message);
    process.exitCode = 2;
  }
}
