#!/usr/bin/env node
// Render node-metrics JSONL without uploading captures or loading external assets.
import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function seriesFrom(text) {
  const records = text.trim().split('\n').map((line) => JSON.parse(line));
  const starts = records.filter((r) => r.event === 'start');
  if (starts.length !== 1) throw new Error('Use one node-metrics capture per report');
  const rows = records.filter((r) => r.event === 'sample' || r.event === 'exit');
  if (!rows.length) throw new Error('No measurement records');
  let previous = { elapsedMs: 0, cpuMs: { user: 0, system: 0 }, gc: { durationMs: 0 } };
  return rows.map((r) => {
    const dt = r.elapsedMs - previous.elapsedMs;
    if (!(dt > 0)) throw new Error('Measurement times must increase');
    const cpu = 100 * (r.cpuMs.user + r.cpuMs.system - previous.cpuMs.user - previous.cpuMs.system) / dt;
    const row = { seconds: r.elapsedMs / 1000, cpuPercent: cpu, rssMiB: r.memoryBytes.rss / 2 ** 20,
      heapMiB: r.memoryBytes.heapUsed / 2 ** 20, externalMiB: r.memoryBytes.external / 2 ** 20,
      gcMs: r.gc.durationMs - previous.gc.durationMs,
      loopDelayMaxMs: r.eventLoop.delayMs?.max ?? null };
    if (Object.values(row).some((v) => v !== null && !Number.isFinite(v))) throw new Error('Non-finite measurement');
    previous = r;
    return row;
  });
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
  if (process.argv[2] === '--help') console.log('Usage: node metrics-report.mjs <capture.metrics.jsonl> <new-report.html>');
  else {
    try {
      if (process.argv.length !== 4) throw new Error('Expected input JSONL and new output HTML');
      const rows = seriesFrom(readFileSync(process.argv[2], 'utf8'));
      writeFileSync(process.argv[3], render(rows), { flag: 'wx', mode: 0o600 });
      console.log(`Report: ${process.argv[3]} (${rows.length} intervals)`);
    } catch (error) { console.error(`metrics-report: ${error.message}`); process.exitCode = 2; }
  }
}
