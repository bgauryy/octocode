import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyzePlan, loadPlan, validateCheckpoint } from '../dep-graph.mjs';

const script = fileURLToPath(new URL('../dep-graph.mjs', import.meta.url));
const fixtures = fileURLToPath(new URL('fixtures/', import.meta.url));
const fixture = name => JSON.parse(readFileSync(join(fixtures, name), 'utf8'));

function run(args, cwd = fixtures) {
  const result = spawnSync(process.execPath, [script, ...args], { cwd, encoding: 'utf8', env: { PATH: process.env.PATH } });
  return { status: result.status, json: JSON.parse(result.stdout) };
}

const analyze = (plan, changed) => analyzePlan(loadPlan(plan), changed);
const task = (id, fields = {}) => ({ id, owner: id, ...fields });

test('a circular wait is reported before dispatch and its tasks can never become ready; independent work stays ready', () => {
  const { status, json } = run(['--plan', 'circular-wait.json']);
  assert.equal(status, 1);
  assert.deepEqual(json.cycles, [['api', 'client']]);
  assert.deepEqual(json.neverReady.map(row => [row.id, row.cause]), [['api', 'cycle'], ['client', 'cycle']]);
  assert.deepEqual(json.ready, ['docs']);
});

test('an input with no producer keeps its consumer and everything after it pending, never ready', () => {
  const { status, json } = run(['--plan', 'missing-producer.json']);
  assert.equal(status, 1);
  assert.deepEqual(json.missingProducers, [{ task: 'client', input: 'openapi-spec' }]);
  assert.deepEqual(json.neverReady, [
    { id: 'client', cause: 'missing-producer', via: ['openapi-spec'] },
    { id: 'verify', cause: 'missing-producer', via: ['client-impl', 'openapi-spec'] },
  ]);
  assert.deepEqual(json.ready, ['research']);
});

test('overlapping scopes of unordered tasks with different owners conflict; a dependency between them serializes', () => {
  const { status, json } = run(['--plan', 'writer-conflict.json']);
  assert.equal(status, 1);
  assert.deepEqual(json.writerConflicts, [
    { tasks: ['api', 'client'], boundary: 'packages/schema/src', kind: 'scope' },
    { tasks: ['client', 'follow-up'], boundary: 'packages/schema', kind: 'scope' },
  ]);
  assert.deepEqual(json.ready, ['api', 'client']);
});

test('one owner serializes its own tasks, and two producers of one artifact conflict', () => {
  const sameOwner = analyze({ tasks: [{ id: 'a', owner: 'w', scope: ['src'] }, { id: 'b', owner: 'w', scope: ['src/x.ts'] }] });
  assert.deepEqual(sameOwner.writerConflicts, []);
  const twoProducers = analyze({ tasks: [task('a', { output: 'schema' }), task('b', { output: 'schema' })] });
  assert.deepEqual(twoProducers.writerConflicts, [{ tasks: ['a', 'b'], boundary: 'schema', kind: 'output' }]);
  const bothAccepted = analyze({ tasks: [task('a', { scope: ['src'], status: 'accepted' }), task('b', { scope: ['src'], status: 'accepted' })] });
  assert.deepEqual(bothAccepted.writerConflicts, []);
  const oneAccepted = analyze({ tasks: [task('a', { scope: ['src'], status: 'accepted' }), task('b', { scope: ['src'] })] });
  assert.equal(oneAccepted.writerConflicts.length, 1, 'a later writer can still change an accepted artifact');
});

test('a join waits on every producer that is not accepted; a failed producer can be repaired, a cancelled one cannot', () => {
  const plan = status => ({ tasks: [task('p', { output: 'p-out', status: 'accepted' }), task('c', { output: 'c-out', status }), task('join', { inputs: ['p-out', 'c-out'] })] });
  for (const status of ['reported', 'failed', 'running']) {
    const result = analyze(plan(status));
    assert.deepEqual(result.waiting, [{ id: 'join', on: ['c-out'], blocked: false }], status);
    assert.deepEqual(result.neverReady, [], status);
  }
  const cancelled = analyze(plan('cancelled'));
  assert.deepEqual(cancelled.neverReady, [{ id: 'join', cause: 'cancelled-producer', via: ['c-out', 'c'] }]);
});

test('without a change the plan is clean: ready and waiting tasks only, exit 0', () => {
  const { status, json } = run(['--plan', 'changed-contract.json']);
  assert.equal(status, 0);
  assert.equal(json.ok, true);
  assert.deepEqual(json.ready, ['final-report']);
  assert.deepEqual(json.waiting, [{ id: 'integrate', on: ['consumer-impl'], blocked: false }]);
  assert.deepEqual(json.stale, []);
});

test('--changed marks downstream evidence and running work stale, keeps independent evidence and holds their consumers', () => {
  const { status, json } = run(['--plan', 'changed-contract.json', '--changed', 'contract']);
  assert.equal(status, 0);
  assert.deepEqual(json.stale, [
    { id: 'producer', status: 'accepted', via: ['contract', 'producer'] },
    { id: 'consumer', status: 'running', via: ['contract', 'consumer'] },
  ]);
  assert.ok(!json.stale.some(row => row.id === 'research'));
  assert.deepEqual(json.waiting, [{ id: 'integrate', on: ['producer-impl', 'consumer-impl'], blocked: false }]);
  assert.deepEqual(json.ready, ['final-report']);
});

test('invalid plans exit 2 with a stable code', () => {
  const dir = mkdtempSync(join(tmpdir(), 'dep-graph-'));
  try {
    const cases = {
      'duplicate-task': { tasks: [{ id: 'a' }, { id: 'a' }] },
      'unknown-status': { tasks: [{ id: 'a', status: 'done' }] },
      'invalid-plan': { tasks: [{ id: 'a', inputs: 'x', scope: [1] }] },
    };
    for (const [code, plan] of Object.entries(cases)) {
      writeFileSync(join(dir, `${code}.json`), JSON.stringify(plan));
      const { status, json } = run(['--plan', `${code}.json`], dir);
      assert.equal(status, 2, code);
      assert.equal(json.error.code, code);
    }
    const unknown = run(['--plan', join(fixtures, 'changed-contract.json'), '--changed', 'nothing'], dir);
    assert.equal(unknown.status, 2);
    assert.equal(unknown.json.error.code, 'unknown-changed');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a complete checkpoint validates and carries the plan analysis', () => {
  const { status, json } = run(['--validate-checkpoint', 'checkpoint.json']);
  assert.equal(status, 0);
  assert.deepEqual(json.errors, []);
  assert.deepEqual(json.plan.waiting, [{ id: 'integrate', on: ['consumer-impl'], blocked: false }]);
});

test('checkpoint errors: missing goal, a running task without a handle, an accepted task without evidence or downstream of an unknown effect', () => {
  const checkpoint = fixture('checkpoint.json');
  const errors = mutate => {
    const copy = structuredClone(checkpoint);
    mutate(copy);
    return validateCheckpoint(copy).errors.map(error => error.message);
  };
  assert.deepEqual(errors(copy => { delete copy.goal; }), ['required: the user outcome in one string']);
  assert.deepEqual(errors(copy => { copy.handles = []; }), ['consumer is running but has no handle to reconcile it']);
  assert.deepEqual(errors(copy => { copy.evidence[1].status = 'stale'; }), ['producer is accepted but has no current evidence']);
  assert.deepEqual(errors(copy => { copy.unresolvedEffects = [{ task: 'contract', operation: 'publish schema', check: 'registry lists v2' }]; }), [
    'contract is accepted while an effect it depends on is unknown; keep it unverified',
    'producer is accepted while an effect it depends on is unknown; keep it unverified',
  ]);
  assert.deepEqual(errors(copy => { copy.handles.push({ task: 'ghost', ref: 'x' }); }), ['ghost is not a task in the plan']);
  const invalid = validateCheckpoint({ ...checkpoint, plan: { tasks: 'none' } });
  assert.equal(invalid.ok, false);
  assert.equal(invalid.errors[0].field, 'plan');
});
