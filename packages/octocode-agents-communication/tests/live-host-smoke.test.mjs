import assert from 'node:assert/strict';
import test from 'node:test';
import { gradeCollaboration } from '../src/live-host-smoke.mjs';

function fixture() {
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
  assert.equal(gradeCollaboration(fixture()).passed, true);
  for (const change of [row => row.body = '12', row => row.replyTo = 1, row => row.target = 'someone-else']) {
    const input = fixture(); change(input.messages[2]);
    assert.equal(gradeCollaboration(input).passed, false);
  }
});

test('host submission alone cannot pass without handling or with a reply loop', () => {
  const missing = fixture(); missing.deliveries[0].acknowledgedAt = null;
  assert.equal(gradeCollaboration(missing).passed, false);
  const loop = fixture(); loop.messages.push({ id: 4, sender: 'receiver', target: 'sender', body: 'FYI received', replyTo: 1 });
  assert.equal(gradeCollaboration(loop).passed, false);
  const passive = fixture(); passive.passiveTurns = 1;
  assert.equal(gradeCollaboration(passive).passed, false);
});

test('authorized startup reports are separate from duplicate answers to the request', () => {
  const startup = fixture();
  startup.initialMessages = [4];
  startup.messages.push({ id: 4, sender: 'receiver', target: 'sender', body: 'Ready', replyTo: null });
  assert.equal(gradeCollaboration(startup).passed, true);
  const duplicate = fixture();
  duplicate.messages.push({ ...duplicate.messages[2], id: 4 });
  assert.equal(gradeCollaboration(duplicate).passed, false);
});
