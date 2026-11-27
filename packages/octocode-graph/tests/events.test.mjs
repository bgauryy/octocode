import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { summarizeEvents } from '../src/runtime/events.ts';
import { tempDir } from './test-tmp.mjs';

// A build (run … done) with three calls, then a refresh (… refreshed) with one, then a viewer call after both.
const log = () => {
  let seq = 0;
  const e = (t, event) => ({ seq: seq++, t, ...event });
  const tool = (t, id, state, extra = {}) => e(t, { type: 'tool', id, state, tool: extra.tool ?? 'graphQuery', page: extra.page ?? 'first', ...extra });
  return [
    e(0, { type: 'run', root: '/repo', options: {} }),
    e(5, { type: 'phase', id: 'list', state: 'start' }),
    tool(6, 'a-1', 'start', { tool: 'structureSearch' }),
    tool(16, 'a-1', 'done', { tool: 'structureSearch', ms: 10, bytes: 100, rows: 4 }),
    e(17, { type: 'phase', id: 'list', state: 'done' }),
    e(18, { type: 'phase', id: 'graph', state: 'start' }),
    tool(20, 'a-2', 'start'),
    tool(50, 'a-2', 'done', { ms: 30, bytes: 1000, rows: 7, error: 'scopeTooBroad' }),
    tool(51, 'a-3', 'start', { page: 'cursor …abc' }),
    tool(60, 'a-3', 'error', { page: 'cursor …abc', ms: 9, error: 'expired' }),
    e(61, { type: 'chunk', id: 'root', state: 'error', error: 'graphBuild failed' }),
    e(70, { type: 'phase', id: 'graph', state: 'done' }),
    e(80, { type: 'done', ms: 80 }),
    e(100, { type: 'phase', id: 'graph', state: 'start' }),
    tool(101, 'b-1', 'start'),
    tool(111, 'b-1', 'done', { ms: 10, bytes: 10, rows: 1 }),
    e(130, { type: 'refreshed', ms: 30 }),
    tool(200, 'c-1', 'start', { tool: 'lspSearch' }),
  ];
};

test('events summary: each id counts once per tool; runs split at done and refreshed', () => {
  const s = summarizeEvents(log());
  assert.equal(s.events, log().length);
  assert.deepEqual(
    { calls: s.tools.calls, cursorPages: s.tools.cursorPages, errors: s.tools.errors, errorRows: s.tools.errorRows, unfinished: s.tools.unfinished, rows: s.tools.rows, bytes: s.tools.bytes, ms: s.tools.ms },
    { calls: 5, cursorPages: 1, errors: 1, errorRows: 1, unfinished: 1, rows: 12, bytes: 1110, ms: 59 }
  );
  assert.equal(s.tools.byTool.graphQuery.calls, 3);
  assert.equal(s.tools.byTool.lspSearch.unfinished, 1);
  // Totals per tool add up to the total.
  for (const key of ['calls', 'rows', 'bytes', 'ms'])
    assert.equal(Object.values(s.tools.byTool).reduce((n, t) => n + t[key], 0), s.tools[key]);
  assert.deepEqual(s.starts, { ids: 5, startEvents: 5, repeated: [], withoutStart: [] });
  assert.deepEqual(s.runs.map((r) => [r.kind, r.seq, r.finished, r.wallMs, r.tools.calls]), [
    ['build', [0, 12], true, 80, 3],
    ['refresh', [13, 16], true, 30, 1],
    ['after', [17, 17], true, 0, 1],
  ]);
  assert.deepEqual(s.runs[0].phases, [{ id: 'list', state: 'done', ms: 12 }, { id: 'graph', state: 'done', ms: 52 }]);
  assert.deepEqual(s.runs[0].failures, ['chunk root: graphBuild failed']);
});

test('events summary reports ids started twice or never, and an unfinished build', () => {
  const events = log().slice(0, 8);
  events.push({ seq: 8, t: 90, type: 'tool', id: 'a-2', state: 'start', tool: 'graphQuery' });
  events.push({ seq: 9, t: 91, type: 'tool', id: 'z-9', state: 'done', tool: 'graphQuery', ms: 1 });
  const s = summarizeEvents(events);
  assert.deepEqual(s.starts.repeated, [{ id: 'a-2', starts: 2 }]);
  assert.deepEqual(s.starts.withoutStart, ['z-9']);
  assert.deepEqual(s.runs.map((r) => [r.kind, r.finished]), [['build', false]]);
  assert.deepEqual(s.runs[0].phases.at(-1), { id: 'graph', state: 'started', ms: null });
});

test('octocode-graph events prints the summary of a result directory, as text or JSON', () => {
  const dir = tempDir('graph-events-');
  writeFileSync(join(dir, 'events.jsonl'), `${log().map((e) => JSON.stringify(e)).join('\n')}\n`);
  const bin = new URL('../bin/octocode-graph.mjs', import.meta.url).pathname;
  const run = (...args) => spawnSync(process.execPath, [bin, 'events', ...args], { encoding: 'utf8' });
  const text = run(dir);
  assert.equal(text.status, 0, text.stderr);
  assert.match(text.stdout, /build seq 0–12: wall 80 ms, 3 tool calls \(1 cursor page, 1 failed, 1 error row, 0 unfinished\)/);
  assert.match(text.stdout, /^total\s+5\s+1\s+1\s+1\s+1\s+12\s+1110\s+59$/m);
  const json = JSON.parse(run(dir, '--json').stdout);
  assert.equal(json.dir, dir);
  assert.equal(json.tools.calls, 5);
  assert.equal(run().status, 2);
});
