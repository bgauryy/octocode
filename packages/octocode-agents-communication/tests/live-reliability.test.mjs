import { test } from 'node:test';
import assert from 'node:assert/strict';
import { gradeHandoff, gradePagedRead, toolStats } from '../src/live-reliability.mjs';

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
  const value = { messages: [{ id: 3, sender: worker, body: ' CODE-1 ', replyTo: request }], deliveries: base.deliveries, request, worker, expected: 'CODE-1' };
  assert.equal(gradePagedRead(value).passed, true);
  assert.equal(gradePagedRead({ ...value, messages: [...value.messages, { id: 5, sender: worker, body: 'CODE-1', replyTo: request }] }).passed, false);
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
