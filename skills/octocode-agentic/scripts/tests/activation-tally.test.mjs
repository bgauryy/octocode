import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

const script = new URL('../activation-tally.mjs', import.meta.url).pathname;
const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
const jsonl = (rows) => `${rows.map((row) => (typeof row === 'string' ? row : JSON.stringify(row))).join('\n')}\n`;

const init = { type: 'system', subtype: 'init', claude_code_version: '9.9.9', model: 'm1' };
const skill = (name, parent = null) => ({ type: 'assistant', parent_tool_use_id: parent, message: { content: [{ type: 'text', text: 'x' }, { type: 'tool_use', name: 'Skill', input: { skill: name } }] } });
const otherTool = { type: 'assistant', message: { content: [{ type: 'tool_use', name: 'Bash', input: { command: 'ls' } }] } };
const done = (cost) => ({ type: 'result', subtype: 'success', is_error: false, total_cost_usd: cost });

const REQUESTS = [
  { id: 'r1', set: 'tuning', expected: 'debugging' },
  { id: 'r2', set: 'tuning', expected: 'debugging' },
  { id: 'r3', set: 'tuning', expected: 'research' },
  { id: 'r4', set: 'heldout', expected: null },
  { id: 'r5', set: 'heldout', expected: null },
  { id: 'r6', set: 'heldout', expected: 'research' },
];

function fixture(dir, arms) {
  writeFileSync(join(dir, 'requests.jsonl'), jsonl(REQUESTS));
  for (const [arm, runs] of Object.entries(arms)) {
    mkdirSync(join(dir, 'runs', arm), { recursive: true });
    for (const [name, events] of Object.entries(runs)) writeFileSync(join(dir, 'runs', arm, `${name}.jsonl`), jsonl(events));
  }
}

test('tally classifies the first top-level Skill call and counts outcomes per set and arm', () => {
  const dir = mkdtempSync(join(tmpdir(), 'activation-tally-'));
  try {
    const runs = {
      r1: [init, skill('plugin:debugging'), done(0.1)],
      r2: [init, skill('nested', 'toolu_1'), skill('research'), done(0.2)],
      r3: [init, otherTool, done(0.3)],
      r4: [init, done(0.1)],
      r5: [init, skill('debugging'), done(0.1)],
      r6: [init, otherTool, 'truncated {'],
      'r6.2': [init, skill('research'), done(0.1)],
    };
    fixture(dir, { baseline: runs, candidate: runs });
    const result = run(join(dir, 'runs'), '--expected', join(dir, 'requests.jsonl'));
    assert.equal(result.status, 1, result.stdout);
    const body = JSON.parse(result.stdout);
    const outcomes = Object.fromEntries(body.runs.filter((r) => r.arm === 'baseline').map((r) => [`${r.id}${r.repeat ? `.${r.repeat}` : ''}`, r.outcome]));
    assert.deepEqual(outcomes, { r1: 'correct', r2: 'wrongRoute', r3: 'miss', r4: 'correctRejection', r5: 'falseTrigger', r6: 'runError', 'r6.2': 'correct' });
    const r6 = body.runs.find((r) => r.id === 'r6' && r.repeat === null);
    assert.equal(r6.badLines, 1);
    assert.deepEqual(body.arms, ['baseline', 'candidate']);
    for (const set of body.sets) {
      const counted = ['correct', 'miss', 'wrongRoute', 'correctRejection', 'falseTrigger', 'runError'].reduce((sum, key) => sum + set[key], 0);
      assert.equal(counted, set.runs);
    }
    const tuning = body.sets.find((s) => s.set === 'tuning' && s.arm === 'baseline');
    assert.deepEqual(tuning.winners, { research: 1 });
    assert.equal(tuning.costRuns, 3);
    assert.equal(body.runs[0].hostVersion, '9.9.9');
    assert.deepEqual(body.missing, []);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('tally exits 0 on full coverage, 1 on a missing run and 2 on bad input', () => {
  const dir = mkdtempSync(join(tmpdir(), 'activation-tally-'));
  try {
    const clean = Object.fromEntries(REQUESTS.map((r) => [r.id, [init, ...(r.expected ? [skill(r.expected)] : []), done(0.1)]]));
    fixture(dir, { a: clean });
    const expected = join(dir, 'requests.jsonl');
    const full = run(join(dir, 'runs'), '--expected', expected);
    assert.equal(full.status, 0, full.stdout);
    assert.ok(JSON.parse(full.stdout).sets.every((s) => s.correct + s.correctRejection === s.runs));

    const { r3: _dropped, ...partial } = clean;
    fixture(dir, { b: partial });
    const gap = run(join(dir, 'runs'), '--expected', expected);
    assert.equal(gap.status, 1);
    assert.deepEqual(JSON.parse(gap.stdout).missing, [{ arm: 'b', id: 'r3' }]);

    writeFileSync(join(dir, 'runs', 'a', 'unknown.jsonl'), jsonl([init, done(0)]));
    const unknown = run(join(dir, 'runs'), '--expected', expected);
    assert.equal(unknown.status, 2);
    assert.equal(JSON.parse(unknown.stdout).ok, false);
    writeFileSync(join(dir, 'bad.jsonl'), jsonl([{ id: 'x', set: 's' }]));
    assert.equal(run(join(dir, 'runs', 'b'), '--expected', join(dir, 'bad.jsonl')).status, 2);
    assert.equal(run('--expected', expected).status, 2);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
