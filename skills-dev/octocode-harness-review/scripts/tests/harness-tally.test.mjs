import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { METRICS, compare, inBandError, summarize, tallyTranscript, taskOf } from '../harness-tally.mjs';

const cli = resolve(import.meta.dirname, '../harness-tally.mjs');
const lines = (...records) => records.map((record) => (typeof record === 'string' ? record : JSON.stringify(record))).join('\n');
const usage = (input, cacheWrite, cacheRead, output) => ({ input_tokens: input, cache_creation_input_tokens: cacheWrite, cache_read_input_tokens: cacheRead, output_tokens: output });
const assistant = (id, content, u, timestamp) => ({ type: 'assistant', timestamp, message: { id, model: 'm', content, usage: u } });
const result = (id, content, isError) => ({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: id, content, ...(isError ? { is_error: true } : {}) }] } });

// A session transcript: the host splits one message into a line per block and repeats its usage on each.
const session = lines(
  { type: 'mode' },
  assistant('m1', [{ type: 'tool_use', id: 't1', name: 'localSearch', input: { q: 'é' } }], usage(3, 100, 0, 10), '2026-11-24T10:00:00.000Z'),
  result('t1', 'boom', true),
  assistant('m2', [{ type: 'tool_use', id: 't2', name: 'localSearch', input: {} }], usage(1, 5, 100, 20), '2026-11-24T10:00:01.000Z'),
  assistant('m2', [{ type: 'tool_use', id: 't3', name: 'localFetch', input: {} }], usage(1, 5, 100, 20), '2026-11-24T10:00:01.500Z'),
  result('t2', [{ type: 'text', text: '{"results":[{"status":"error"}]}' }]),
  result('t3', 'ok'),
  '{not json',
  assistant('m3', [{ type: 'text', text: 'answer 😀' }], usage(2, 0, 105, 30), '2026-11-24T10:00:04.000Z'),
);

test('a session transcript counts each message once and splits the four token fields', () => {
  const tally = tallyTranscript(session);
  assert.equal(tally.format, 'session');
  assert.equal(tally.requests, 3);
  assert.deepEqual(tally.tokens, { input: 6, cacheRead: 205, cacheWrite: 105, output: 60, source: 'messages' });
  assert.equal(tally.firstCacheWrite, 100);
  assert.equal(tally.calls, 3);
  assert.equal(tally.failed, 2, 'host is_error and an in-band error row both count');
  assert.equal(tally.retries, 1, 'the localSearch after a failed localSearch');
  assert.deepEqual(tally.tools, { localSearch: 2, localFetch: 1 });
  assert.deepEqual(tally.failedTools, { localSearch: 2 });
  assert.equal(tally.chars.answer, 8, 'code points, not UTF-16 units');
  assert.equal(tally.chars.total, tally.chars.calls + tally.chars.results + tally.chars.answer);
  assert.equal(tally.wallMs, 4000);
  assert.equal(tally.wallSource, 'timestamps');
  assert.equal(tally.malformedLines, 1);
});

test('a stream-json capture takes tokens, duration and the answer from its result record', () => {
  const tally = tallyTranscript(lines(
    { type: 'system', subtype: 'init' },
    assistant('m1', [{ type: 'text', text: 'partial' }], usage(2, 50, 0, 1)),
    { type: 'result', result: 'final', duration_ms: 1234, total_cost_usd: 0.5, usage: usage(2, 50, 0, 99) },
  ));
  assert.equal(tally.format, 'stream-json');
  assert.deepEqual(tally.tokens, { input: 2, cacheRead: 0, cacheWrite: 50, output: 99, source: 'result' });
  assert.equal(tally.wallMs, 1234);
  assert.equal(tally.chars.answer, 5);
  assert.equal(tally.costUsd, 0.5);
  assert.equal(tallyTranscript(lines({ type: 'user', message: { content: 'hi' } })), null);
});

test('in-band errors are Octocode JSON error replies only', () => {
  assert.equal(inBandError('{"errorCode":"invalidInput"}'), true);
  assert.equal(inBandError('{"results":[{"status":"ok"},{"status":"error"}]}'), true);
  assert.equal(inBandError('{"results":[{"status":"empty"}]}'), false);
  assert.equal(inBandError('status: error'), false);
});

test('runs group by task and compare against the spread of both sides', () => {
  assert.equal(taskOf('/x/runs/T4/transcript.jsonl'), 'T4');
  assert.equal(taskOf('/x/agent-abc.jsonl'), 'agent-abc');
  assert.equal(taskOf('/x/G2-base-p3/transcript.jsonl', /\/(G\d+)-/), 'G2');
  const run = (task, calls) => ({ task, calls });
  const before = summarize([run('A', 10), run('A', 12), run('B', 5)]);
  const after = summarize([run('A', 4), run('A', 5), run('C', 1)]);
  assert.deepEqual(before.map((task) => [task.task, task.runs]), [['A', 2], ['B', 1]]);
  assert.equal(before[0].median.calls, 11);
  assert.equal(before[0].spread.calls, 2);
  assert.equal(before[0].median.wallMs, null, 'an unmeasured metric stays null, never zero');
  const { tasks, onlyBefore, onlyAfter } = compare(before, after);
  assert.deepEqual(tasks[0].metrics.calls, { before: 11, after: 4.5, delta: -6.5, spread: 2, beyondSpread: true });
  assert.deepEqual(onlyBefore, ['B']);
  assert.deepEqual(onlyAfter, ['C']);
  const single = compare(summarize([run('A', 10)]), summarize([run('A', 1)]));
  assert.equal(single.tasks[0].metrics.calls.beyondSpread, null, 'one run has no spread to judge against');
  assert.ok(METRICS.every((metric) => metric in tasks[0].metrics));
});

test('the CLI tallies, compares with --before and rejects invalid input with exit 2', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'harness-tally-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const name of ['r1', 'r2']) {
    mkdirSync(join(dir, name, 'T1'), { recursive: true });
    writeFileSync(join(dir, name, 'T1', 'transcript.jsonl'), session);
  }
  const invoke = (...args) => spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8', timeout: 10_000 });
  const first = invoke(join(dir, 'r1/T1/transcript.jsonl'), join(dir, 'r2/T1/transcript.jsonl'));
  assert.equal(first.status, 0, first.stdout + first.stderr);
  const tally = JSON.parse(first.stdout);
  assert.deepEqual(tally.tasks.map((task) => [task.task, task.runs]), [['T1', 2]]);
  writeFileSync(join(dir, 'before.json'), first.stdout);
  const again = invoke(join(dir, 'r1/T1/transcript.jsonl'), join(dir, 'r2/T1/transcript.jsonl'), '--before', join(dir, 'before.json'));
  assert.equal(again.status, 0, again.stdout + again.stderr);
  assert.equal(JSON.parse(again.stdout).comparison.tasks[0].metrics['tokens.output'].beyondSpread, false);
  assert.equal(invoke('--help').status, 0);
  writeFileSync(join(dir, 'empty.jsonl'), '{"type":"mode"}\n');
  for (const args of [[], [join(dir, 'missing.jsonl')], [join(dir, 'empty.jsonl')], [join(dir, 'r1/T1/transcript.jsonl'), '--before', join(dir, 'empty.jsonl')], [join(dir, 'r1/T1/transcript.jsonl'), '--task', '(']]) {
    const failed = invoke(...args);
    assert.equal(failed.status, 2, args.join(' '));
    assert.equal(JSON.parse(failed.stdout).ok, false);
  }
});
