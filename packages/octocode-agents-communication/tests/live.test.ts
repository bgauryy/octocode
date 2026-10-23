import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { DeliveryRow, MessageRow } from '../src/live/grading.ts';
import { gradeCollaboration, gradeConcurrentWave, gradeHandoff, gradePagedRead } from '../src/live/grading.ts';
import { parseArgs } from '../src/live/options.ts';
import { harnessSha256, liveSoak, liveTrials, runLive, toolStats } from '../src/live.ts';

// --- soak: concurrent waves ---
const state = {
  sender: 'controller',
  requests: [{ id: 1, recipient: 'pi', expected: '15' }, { id: 2, recipient: 'codex', expected: '20' }],
  messages: [
    { id: 1, sender: 'controller', target: 'pi', body: '7+8', replyTo: null },
    { id: 2, sender: 'controller', target: 'codex', body: '11+9', replyTo: null },
    { id: 3, sender: 'pi', target: 'controller', body: '15', replyTo: 1 },
    { id: 4, sender: 'codex', target: 'controller', body: '20', replyTo: 2 },
  ],
  deliveries: [{ message: 1, recipient: 'pi', acknowledgedAt: 10 }, { message: 2, recipient: 'codex', acknowledgedAt: 20 }],
  dispatches: [{ message: 1, recipient: 'pi', state: 'submitted', submittedAt: 5 }, { message: 2, recipient: 'codex', state: 'submitted', submittedAt: 15 }],
};

test('concurrent grading requires exact, correlated, acknowledged native submissions', () => {
  assert.equal(gradeConcurrentWave(state).passed, true);
  for (const invalid of [
    { ...state, messages: state.messages.filter(message => message.id !== 4) },
    { ...state, messages: state.messages.map(message => message.id === 4 ? { ...message, body: '21' } : message) },
    { ...state, messages: state.messages.map(message => message.id === 4 ? { ...message, sender: 'claude' } : message) },
    { ...state, messages: state.messages.map(message => message.id === 4 ? { ...message, target: 'other' } : message) },
    { ...state, messages: [...state.messages, { ...state.messages[3], id: 5 }] },
    { ...state, messages: [...state.messages, { id: 6, sender: 'pi', target: 'codex', replyTo: null, body: 'unexpected' }] },
    { ...state, deliveries: state.deliveries.map(delivery => ({ ...delivery, acknowledgedAt: null })) },
    { ...state, dispatches: state.dispatches.map(dispatch => ({ ...dispatch, state: 'uncertain' })) },
  ]) assert.equal(gradeConcurrentWave(invalid).passed, false);
});

test('importing or calling the soak cannot launch paid work without opt-in and valid configuration', async () => {
  await assert.rejects(liveSoak({}), /require --live/);
  await assert.rejects(liveSoak({ live: true, models: { pi: 'model' } }), /explicit/);
  const models = { pi: 'model', claude: 'model', codex: 'model' };
  await assert.rejects(liveSoak({ live: true, models, waves: 1 }), /waves/);
  await assert.rejects(liveSoak({ live: true, models, idleMs: 1 }), /presence interval/);
  await assert.rejects(liveSoak({ live: true, models, durationMs: 1000 }), /cover idle intervals/);
});

// --- qa: collaboration grading ---
type GradedMessage = Pick<MessageRow, 'id' | 'sender' | 'target' | 'body' | 'replyTo'>;
interface Collaboration { request: number; notice: number; sender: string; receiver: string; expected: string; passiveTurns: number; messages: GradedMessage[]; deliveries: DeliveryRow[]; initialMessages?: number[] }

function collaborationFixture(): Collaboration {
  return {
    request: 2, notice: 1, sender: 'sender', receiver: 'receiver', expected: '13', passiveTurns: 0,
    messages: [
      { id: 1, sender: 'sender', target: 'receiver', body: 'FYI', replyTo: null },
      { id: 2, sender: 'sender', target: 'receiver', body: 'Question', replyTo: null },
      { id: 3, sender: 'receiver', target: 'sender', body: '13', replyTo: 2 },
    ],
    deliveries: [
      { message: 1, recipient: 'receiver', acknowledgedAt: 100 },
      { message: 2, recipient: 'receiver', acknowledgedAt: 101 },
    ],
  };
}

test('live-host grade requires the answer and exact message correlation', () => {
  assert.equal(gradeCollaboration(collaborationFixture()).passed, true);
  for (const change of [(row: GradedMessage) => row.body = '12', (row: GradedMessage) => row.replyTo = 1, (row: GradedMessage) => row.target = 'someone-else']) {
    const input = collaborationFixture(); change(input.messages[2]);
    assert.equal(gradeCollaboration(input).passed, false);
  }
});

test('host submission alone cannot pass without handling or with a reply loop', () => {
  const missing = collaborationFixture(); missing.deliveries[0].acknowledgedAt = null;
  assert.equal(gradeCollaboration(missing).passed, false);
  const loop = collaborationFixture(); loop.messages.push({ id: 4, sender: 'receiver', target: 'sender', body: 'FYI received', replyTo: 1 });
  assert.equal(gradeCollaboration(loop).passed, false);
  const passive = collaborationFixture(); passive.passiveTurns = 1;
  assert.equal(gradeCollaboration(passive).passed, false);
});

test('authorized startup reports are separate from duplicate answers to the request', () => {
  const startup = collaborationFixture();
  startup.initialMessages = [4];
  startup.messages.push({ id: 4, sender: 'receiver', target: 'sender', body: 'Ready', replyTo: null });
  assert.equal(gradeCollaboration(startup).passed, true);
  const duplicate = collaborationFixture();
  duplicate.messages.push({ ...duplicate.messages[2], id: 4 });
  assert.equal(gradeCollaboration(duplicate).passed, false);
});

// --- lock-handoff, paged-read and tool stats ---
const owner = 'owner', worker = 'worker', request = 1;
const base = {
  messages: [
    { id: 1, sender: 'sender', target: worker, body: 'lock it', replyTo: null },
    { id: 2, sender: worker, target: owner, body: 'Need access', replyTo: null },
    { id: 4, sender: worker, target: 'sender', body: 'LOCKED', replyTo: 1 },
  ],
  deliveries: [{ message: 1, recipient: worker, acknowledgedAt: 50 }],
  leases: [{ id: 9, path: '/w/src/api.ts', owner: worker, acquiredAt: 30 }],
  request, owner, worker, releasedAt: 20, expected: 'LOCKED',
};

test('handoff grading requires asking, no overlap, acquisition after release and an exact reply', () => {
  assert.equal(gradeHandoff(base).passed, true);
  assert.equal(gradeHandoff({ ...base, leases: [{ ...base.leases[0], acquiredAt: 10 }] }).checks.noOverlap, false);
  assert.equal(gradeHandoff({ ...base, messages: base.messages.filter(row => row.target !== owner) }).checks.askedOwner, false);
  assert.equal(gradeHandoff({ ...base, leases: [] }).checks.acquiredAfterRelease, false);
  assert.equal(gradeHandoff({ ...base, expected: 'OTHER' }).checks.correlatedAnswer, false);
});

test('paged read grading accepts only the exact final-page code once', () => {
  const value = { messages: [{ id: 3, sender: worker, target: 'sender', body: ' CODE-1 ', replyTo: request }], deliveries: base.deliveries, request, worker, expected: 'CODE-1' };
  assert.equal(gradePagedRead(value).passed, true);
  assert.equal(gradePagedRead({ ...value, messages: [...value.messages, { id: 5, sender: worker, target: 'sender', body: 'CODE-1', replyTo: request }] }).passed, false);
  assert.equal(gradePagedRead({ ...value, expected: 'CODE-2' }).passed, false);
});

test('tool stats count rejected complete calls across host trace shapes', () => {
  const stats = toolStats([
    { type: 'tool-call', callId: 'a', tool: 'mcp__communication__complete' },
    { type: 'tool-result', callId: 'a', isError: true },
    { type: 'tool-call', callId: 'b', tool: 'complete' },
    { type: 'tool-result', callId: 'b', tool: 'complete', error: null },
    { type: 'tool-call', callId: 'c', tool: 'lock' },
    { type: 'tool-result', callId: 'c', tool: 'lock', error: 'conflict' },
  ]);
  assert.deepEqual({ calls: stats.calls, errors: stats.errors, completeErrors: stats.completeErrors }, { calls: 3, errors: 2, completeErrors: 1 });
  assert.deepEqual(stats.byTool.complete, { calls: 2, errors: 1 });
});

// --- entry point and preflight: nothing here starts a vendor process ---

test('live arguments reject unknown options and unparsable models; --live and --help parse', () => {
  assert.deepEqual(parseArgs(['--live', '--vendor', 'codex', '--runs', '2']), { live: true, vendor: 'codex', runs: '2' });
  assert.deepEqual(parseArgs(['--models', '{"pi":"p"}', '--idle-ms', '60000']), { models: { pi: 'p' }, idleMs: '60000' });
  assert.equal(parseArgs(['--help']).help, true);
  assert.throws(() => parseArgs(['--bogus', 'x']), /Unknown or incomplete option --bogus/);
  assert.throws(() => parseArgs(['--vendor']), /Unknown or incomplete option --vendor/);
  assert.throws(() => parseArgs(['--models', '{']), /--models must be a JSON object/);
});

test('trial and scenario selection fail before any vendor starts', async () => {
  await assert.rejects(liveTrials({}), /require --live/);
  await assert.rejects(liveTrials({ live: true, vendor: 'other' }), /codex, claude, or pi/);
  await assert.rejects(liveTrials({ live: true, flows: ['bogus'] }), /--scenarios must name/);
  await assert.rejects(liveTrials({ live: true, runs: 0 }), /--runs/);
  await assert.rejects(liveTrials({ live: true, timeout: 1 }), /--timeout/);
  await assert.rejects(runLive({ live: true, scenarios: 'qa,bogus' }), /Unknown scenario bogus/);
  await assert.rejects(runLive({ live: true, scenarios: 'qa,soak' }), /run it alone/);
});

test('the harness digest covers the entry and every module under its live/ directory', () => {
  // The default digest is the real entry's: src/live.ts with src/live/**.
  assert.equal(harnessSha256(), harnessSha256(fileURLToPath(new URL('../src/live.ts', import.meta.url))));
  const dir = mkdtempSync(join(tmpdir(), 'live-harness-'));
  try {
    const entry = join(dir, 'live.ts');
    writeFileSync(entry, 'entry');
    mkdirSync(join(dir, 'live', 'nested'), { recursive: true });
    writeFileSync(join(dir, 'live', 'grading.ts'), 'grading');
    writeFileSync(join(dir, 'live', 'nested', 'options.ts'), 'options');
    let previous = harnessSha256(entry);
    assert.equal(harnessSha256(entry), previous, 'the digest is deterministic');
    const changes: [string, () => void][] = [
      ['entry edit', () => writeFileSync(entry, 'entry 2')],
      ['module edit', () => writeFileSync(join(dir, 'live', 'grading.ts'), 'grading 2')],
      ['nested module edit', () => writeFileSync(join(dir, 'live', 'nested', 'options.ts'), 'options 2')],
      ['module added', () => writeFileSync(join(dir, 'live', 'mechanics.ts'), 'mechanics')],
      ['module renamed', () => renameSync(join(dir, 'live', 'mechanics.ts'), join(dir, 'live', 'workers.ts'))],
      ['module removed', () => rmSync(join(dir, 'live', 'workers.ts'))],
    ];
    const digests: string[] = [];
    for (const [change, apply] of changes) {
      apply();
      const digest = harnessSha256(entry);
      assert.notEqual(digest, previous, `${change} changes the digest`);
      digests.push((previous = digest));
    }
    // Content-addressed: removing the added module restores the digest from before it.
    assert.equal(digests.at(-1), digests[2]);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
