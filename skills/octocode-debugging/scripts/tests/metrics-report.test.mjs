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
