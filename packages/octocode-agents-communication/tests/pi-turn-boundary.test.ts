import assert from 'node:assert/strict';
import { test } from 'node:test';
import { registerPiInbox } from '../dist/hosts/pi-inbox.js';
import { binary, boundSession } from './helpers.ts';
import { fixture } from './pi-inbox-support.ts';

test('Pi leaves submitted requests pending without forcing a completion turn', async t => {
  const f = fixture(t);
  const controller = registerPiInbox(f.pi, { binary, database: f.database });
  await f.fire('session_start');
  try {
    const session = boundSession(controller);
    const sender = f.run('join', { name: 'sender', vendor: 'independent-provider' }).id;
    const request = f.run('send_message', { to: session, parts: [{ data: { job: 'agent-owned', steps: ['decide', 'act'] } }],
      replyRequired: true, wake: 'action', reasoning: 'Let the recipient decide when to answer' }, sender);
    await controller.drain();
    assert.equal(f.sent.filter(item => item.options.triggerTurn).length, 1, 'New action mail may wake an idle host');
    const count = f.sent.length;
    for (let turn = 0; turn < 2; turn++) {
      await f.fire('agent_start');
      await f.fire('agent_end');
      await new Promise(resolve => setImmediate(resolve));
      await controller.drain();
    }
    assert.equal(f.sent.length, count, 'Unanswered mail cannot create additional model turns');
    assert.equal(f.run('inbox', { message: request.id }, session).items.length, 1);
    assert.equal(f.run('fetch', { type: 'dispatch.submitted', current: true }, session).items.length, 1);
    f.run('complete', { message: request.id, outcome: 'rejected', reply: [{ text: 'Outside my scope' }] }, session);
    assert.equal(f.run('inbox', {}, session).items.length, 0, 'Only the agent decides the request outcome');
  } finally {
    await f.fire('session_shutdown');
  }
});
