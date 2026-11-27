import { test } from 'node:test';
import assert from 'node:assert/strict';
import { render, seriesFrom } from '../metrics-report.mjs';
const start = { event: 'start', pid: 123 };
const sample = (elapsedMs, user, gc, delay = null) => ({ event: 'sample', elapsedMs,
  cpuMs: { user, system: 0 }, memoryBytes: { rss: 2 ** 20, heapUsed: 2 ** 19, external: 0 },
  gc: { durationMs: gc }, eventLoop: { delayMs: delay } });
const jsonl = (...records) => records.map(JSON.stringify).join('\n');
test('CPU and GC use interval deltas and actual time; loop maximum stays cumulative', () => {
  const rows = seriesFrom(jsonl(start, sample(100, 50, 2), sample(300, 450, 5, { max: 40 })));
  assert.equal(rows[0].cpuPercent, 50);
  assert.equal(rows[1].cpuPercent, 200);
  assert.equal(rows[1].gcMs, 3);
  assert.equal(rows[0].loopDelayMaxMs, null);
  assert.equal(rows[1].loopDelayMaxMs, 40);
  assert.equal(rows[1].rssMiB, 1);
  const html = render(rows);
  assert.match(html, /cumulative/);
  assert.match(html, /All interval measurements/);
  assert.doesNotMatch(html, /<script|https?:/);
});
test('reject malformed, combined, empty and nonmonotonic captures', () => {
  for (const input of ['{', jsonl(start, start), jsonl(start),
    jsonl(start, sample(0, 1, 0)), jsonl(start, sample(100, 1, 0), sample(50, 2, 0))]) {
    assert.throws(() => seriesFrom(input));
  }
});
test('resource counters, GC count and loop utilization are differenced per interval; --json reports the exit record', async (t) => {
  const { mkdtempSync, rmSync, writeFileSync } = await import('node:fs');
  const { spawnSync } = await import('node:child_process');
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  const record = (event, elapsedMs, n) => ({ event, elapsedMs, cpuMs: { user: n, system: 0 }, memoryBytes: { rss: 2 ** 20, heapUsed: 0, external: 0 },
    resources: { fsRead: 10 * n, fsWrite: n, maxRSS: 1024 * n }, eventLoop: { idle: 3 * n, active: n, delayMs: null }, gc: { count: 2 * n, durationMs: n } });
  const text = jsonl(start, record('sample', 100, 1), record('exit', 300, 4));
  const rows = seriesFrom(text);
  assert.deepEqual(rows.map((r) => [r.fsRead, r.fsWrite, r.gcCount, r.loopUtilization, r.maxRssMiB]), [[10, 1, 2, 0.25, 1], [30, 3, 6, 0.25, 4]]);
  assert.equal(rows[1].intervalMs, 200);
  const dir = mkdtempSync(join(tmpdir(), 'metrics-report-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(join(dir, 'run.metrics.jsonl'), text);
  const script = new URL('../metrics-report.mjs', import.meta.url).pathname;
  const out = spawnSync(process.execPath, [script, join(dir, 'run.metrics.jsonl'), '--json'], { encoding: 'utf8' });
  assert.equal(out.status, 0, out.stdout + out.stderr);
  const report = JSON.parse(out.stdout);
  assert.equal(report.exitRecorded, true);
  assert.equal(report.intervals, rows.length);
  writeFileSync(join(dir, 'cut.metrics.jsonl'), jsonl(start, record('sample', 100, 1)));
  assert.equal(JSON.parse(spawnSync(process.execPath, [script, join(dir, 'cut.metrics.jsonl'), '--json'], { encoding: 'utf8' }).stdout).exitRecorded, false);
  const bad = spawnSync(process.execPath, [script, join(dir, 'missing.jsonl'), '--json'], { encoding: 'utf8' });
  assert.equal(bad.status, 2);
  assert.equal(JSON.parse(bad.stdout).ok, false);
});
