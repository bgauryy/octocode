#!/usr/bin/env node
// Render node-metrics JSONL without uploading captures or loading external assets.
import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { errorMessage, fail } from './lib/fail.mjs';

const USAGE = `Usage: node metrics-report.mjs <capture.metrics.jsonl> <new-report.html>
       node metrics-report.mjs <capture.metrics.jsonl> --json
Reads one node-metrics.mjs capture (one start record, then sample and exit records) and differences consecutive
records into intervals: seconds (elapsed at the interval end), intervalMs, cpuPercent (user + system CPU ms over
elapsed ms; 100 = one core), rssMiB, heapMiB and externalMiB (at the interval end), gcCount and gcMs (collections and
their duration in the interval), loopUtilization (event-loop active share of the interval), loopDelayMaxMs (cumulative
since start), fsRead and fsWrite (block I/O operations in the interval), maxRssMiB (process high-water mark so far).
A field the capture lacks is null.
With an output path: writes a self-contained HTML report there (refuses an existing file; mode 0600) and prints its
path. With --json: prints {"ok":true,"pid","exitRecorded","exitCode","intervals","rows":[...]} on stdout, values
rounded to 3 decimals; exitRecorded false means the run was interrupted before its exit measurement.
Exit 0 on success; 2 for unreadable, combined, empty or non-monotonic captures (--json prints
{"ok":false,"error":{code,message}}).`;

const records = (text) => text.trim().split('\n').map((line) => JSON.parse(line));
const finite = (value) => (Number.isFinite(value) ? value : null);
const change = (now, before) => (Number.isFinite(now) && Number.isFinite(before) ? now - before : null);

export function seriesFrom(text) {
  const all = records(text);
  const starts = all.filter((r) => r.event === 'start');
  if (starts.length !== 1) throw new Error('Use one node-metrics capture per report');
  const rows = all.filter((r) => r.event === 'sample' || r.event === 'exit');
  if (!rows.length) throw new Error('No measurement records');
  let previous = { elapsedMs: 0, cpuMs: { user: 0, system: 0 }, gc: { count: 0, durationMs: 0 }, eventLoop: { idle: 0, active: 0 }, resources: { fsRead: 0, fsWrite: 0 } };
  return rows.map((r) => {
    const dt = r.elapsedMs - previous.elapsedMs;
    if (!(dt > 0)) throw new Error('Measurement times must increase');
    const cpu = 100 * (r.cpuMs.user + r.cpuMs.system - previous.cpuMs.user - previous.cpuMs.system) / dt;
    const active = change(r.eventLoop?.active, previous.eventLoop?.active);
    const idle = change(r.eventLoop?.idle, previous.eventLoop?.idle);
    const row = { seconds: r.elapsedMs / 1000, intervalMs: dt, cpuPercent: cpu, rssMiB: r.memoryBytes.rss / 2 ** 20,
      heapMiB: r.memoryBytes.heapUsed / 2 ** 20, externalMiB: r.memoryBytes.external / 2 ** 20,
      gcCount: change(r.gc.count, previous.gc?.count),
      gcMs: r.gc.durationMs - previous.gc.durationMs,
      loopUtilization: active !== null && idle !== null && active + idle > 0 ? active / (active + idle) : null,
      loopDelayMaxMs: r.eventLoop?.delayMs?.max ?? null,
      fsRead: change(r.resources?.fsRead, previous.resources?.fsRead),
      fsWrite: change(r.resources?.fsWrite, previous.resources?.fsWrite),
      maxRssMiB: Number.isFinite(r.resources?.maxRSS) ? r.resources.maxRSS / 1024 : null };
    if (Object.values(row).some((v) => v !== null && !Number.isFinite(v))) throw new Error('Non-finite measurement');
    previous = r;
    return row;
  });
}

/** The capture's process and whether its exit measurement was written. */
export function captureFacts(text) {
  const all = records(text);
  const exit = all.find((r) => r.event === 'exit');
  return { pid: all.find((r) => r.event === 'start')?.pid ?? null, exitRecorded: Boolean(exit), exitCode: finite(exit?.exitCode) };
}

export function render(rows) {
  const charts = [ ['cpuPercent', 'Process CPU (%) — interval average; 100% = one core'],
    ['rssMiB', 'Process RSS (MiB)'], ['heapMiB', 'Main isolate heap used (MiB)'],
    ['externalMiB', 'Main isolate external memory (MiB)'], ['gcMs', 'Observed GC duration per interval (ms)'],
    ['loopDelayMaxMs', 'Main loop delay maximum since start (ms; cumulative)'] ];
  const duration = rows.at(-1).seconds;
  const figures = charts.map(([key, label]) => {
    const valid = rows.filter((r) => r[key] !== null);
    const max = valid.reduce((peak, r) => Math.max(peak, r[key]), 0);
    const scale = max || 1;
    const points = valid.map((r) => `${(r.seconds / duration * 900).toFixed(2)},${(150 - r[key] / scale * 140).toFixed(2)}`).join(' ');
    return `<figure><figcaption>${label} · peak observed ${valid.length ? max.toFixed(2) : 'unavailable'}</figcaption>
<svg viewBox="0 0 900 175" role="img" aria-label="${label}"><path d="M0 150H900" stroke="#bbb"/>
<polyline points="${points}" fill="none" stroke="#146c94" stroke-width="2"/>
<text x="0" y="173">0 s</text><text x="800" y="173">${duration.toFixed(2)} s</text></svg></figure>`;
  }).join('\n');
  return `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Node runtime measurements</title><style>body{font:16px system-ui;max-width:1000px;margin:32px auto;padding:0 20px;color:#18232c}figure{margin:24px 0}svg{width:100%;background:#f5f8fa}table{border-collapse:collapse}td,th{padding:7px;border:1px solid #ccd;text-align:right}</style>
<h1>Node runtime measurements</h1><p>${rows.length} measurement intervals. All charts share elapsed time; vertical scales differ. Peaks are observed samples, not guaranteed instantaneous maxima. CPU and GC average or aggregate each interval; short bursts may be hidden. Straight lines interpolate between observations.</p>
<p>RSS covers the process; heap and loop metrics cover the main isolate. External memory overlaps array buffers. Loop maximum is cumulative, so it cannot locate later spikes. Delayed sampling during blocked JavaScript hides the spike shape; correlate native or CPU stacks. A single point cannot establish a trend.</p>
${figures}<details><summary>All interval measurements</summary><table><tr>${Object.keys(rows[0]).map((k) => `<th>${k}</th>`).join('')}</tr>${rows.map((r) => `<tr>${Object.values(r).map((v) => `<td>${v === null ? 'unavailable' : v.toFixed(3)}</td>`).join('')}</tr>`).join('')}</table></details>`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  if (args.includes('--help')) console.log(USAGE);
  else if (args.includes('--json')) {
    const inputs = args.filter((arg) => arg !== '--json');
    if (inputs.length !== 1) fail('invalid-input', 'Expected one input JSONL with --json', 2);
    let text;
    let rows;
    try {
      text = readFileSync(inputs[0], 'utf8');
      rows = seriesFrom(text);
    } catch (error) { fail('invalid-input', `${inputs[0]}: ${errorMessage(error)}`, 2); }
    const rounded = rows.map((row) => Object.fromEntries(Object.entries(row).map(([key, value]) => [key, value === null ? null : Number(value.toFixed(3))])));
    console.log(JSON.stringify({ ok: true, ...captureFacts(text), intervals: rows.length, rows: rounded }));
  } else {
    try {
      if (args.length !== 2) throw new Error('Expected input JSONL and new output HTML');
      const rows = seriesFrom(readFileSync(args[0], 'utf8'));
      writeFileSync(args[1], render(rows), { flag: 'wx', mode: 0o600 });
      console.log(`Report: ${args[1]} (${rows.length} intervals)`);
    } catch (error) { console.error(`metrics-report: ${error.message}`); process.exitCode = 2; }
  }
}
