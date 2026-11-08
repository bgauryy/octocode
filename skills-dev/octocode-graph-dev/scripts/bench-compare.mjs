#!/usr/bin/env node
// Compare two graph-bench.mjs runs per grammar. Reject invalid measurements;
// gate precision and workflow regressions, and report recall and timing changes.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { isMainModule } from './lib/entrypoint.mjs';
import { batteryFailures, workflowStatuses } from './lib/battery-status.mjs';

const HELP = `Usage: bench-compare.mjs <before.json> <after.json> [--json]

Prints one Markdown row per grammar. Exit 0 no regression, 1 regression, 2 bad input or invalid measurement.`;

const metric = row => {
  const ingest = row?.ingest ?? {}, recall = row?.recall ?? {};
  return {
    ok: Boolean(row) && !ingest.error && ingest.exit === 0,
    recall: recall.recall ?? null, precision: recall.precision ?? null,
    link: ingest.callLinkRate ?? null, ms: ingest.ms ?? null,
    reused: row?.incremental ? row.incremental.reused === true : null, reuseMs: row?.incremental?.ms ?? null,
    // Only complete lists can name newly missed files.
    missed: recall.missed ?? null, extra: recall.extra ?? null,
    workflows: workflowStatuses(row),
  };
};
const added = (before, after) => before && after ? after.filter(file => !before.includes(file)) : [];

function metricNotes(beforeRow, afterRow, before, after) {
  const notes = [];
  if (!beforeRow) notes.push('new row');
  if (!afterRow) notes.push('not run');
  else if (before.ok && !after.ok) notes.push('ingest failed');
  if (before.precision !== null && after.precision !== null && after.precision < before.precision) notes.push('precision down');
  if (before.recall !== null && after.recall !== null && after.recall < before.recall) notes.push('recall down');
  if (before.reused === true && after.reused === false) notes.push('unchanged tree no longer reused');
  return notes;
}

function workflowNotes(before, after) {
  const notes = [];
  let failed = false;
  for (const stage of new Set([...Object.keys(before), ...Object.keys(after)])) {
    if (stage === 'ingest') continue;
    const prior = before[stage], current = after[stage];
    if (JSON.stringify(prior) !== JSON.stringify(current)) notes.push(`${stage}: ${prior?.exit ?? 'not run'} → ${current?.exit ?? 'not run'}`);
    if (prior && [0, 1].includes(prior.exit) && (!current || ![0, 1].includes(current.exit) || current.error)) failed = true;
  }
  if (failed) notes.push('workflow failed');
  return { notes, failed };
}

export function compare(before, after) {
  const langs = [...new Set([...before, ...after].map(row => row.lang))].toSorted((a, b) => a.localeCompare(b));
  return langs.map(lang => {
    const beforeRow = before.find(row => row.lang === lang), afterRow = after.find(row => row.lang === lang);
    const b = metric(beforeRow), a = metric(afterRow);
    const workflow = workflowNotes(b.workflows, a.workflows);
    const notes = [...metricNotes(beforeRow, afterRow, b, a), ...workflow.notes];
    const failures = {
      before: beforeRow ? batteryFailures([beforeRow]) : [],
      after: afterRow ? batteryFailures([afterRow]) : [],
    };
    for (const [side, entries] of Object.entries(failures)) {
      if (entries.length) notes.push(`${side} measurement invalid: ${entries.map(failure => failure.error || `${failure.tool ?? failure.operation} exit ${failure.exit}`).join('; ')}`);
    }
    const invalid = failures.before.length > 0 || failures.after.length > 0;
    const regression = notes.includes('ingest failed') || notes.includes('precision down') || notes.includes('unchanged tree no longer reused') || workflow.failed;
    return { lang, before: b, after: a, notes, regression, invalid, ...(invalid ? { failures } : {}), newMissed: added(b.missed, a.missed), newExtra: added(b.extra, a.extra) };
  });
}

const fmt = (b, a) => (b === a ? String(a ?? '-') : `${b ?? '-'} → ${a ?? '-'}`);

export function markdown(rows) {
  const lines = [
    '| Language | Recall | Precision | Link rate | Ingest ms | Reuse ms | Notes |',
    '|---|---|---|---|---|---|---|',
    ...rows.map((r) => `| ${r.lang} | ${fmt(r.before.recall, r.after.recall)} | ${fmt(r.before.precision, r.after.precision)} | ${fmt(r.before.link, r.after.link)} | ${fmt(r.before.ms, r.after.ms)} | ${fmt(r.before.reuseMs, r.after.reuseMs)} | ${[...r.notes, ...r.newMissed.map((f) => `newly missed ${f}`), ...r.newExtra.map((f) => `new extra ${f}`)].join('; ')} |`),
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
  if (rows.some(row => row.invalid)) return 2;
  return rows.some((row) => row.regression) ? 1 : 0;
}

if (isMainModule(import.meta.url)) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (e) {
    console.error(e.message);
    process.exitCode = 2;
  }
}
