import { test } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { binary, jsonCall, tempWorkspace, withReasoning } from './helpers.mjs';

test('Claude, Grok, Codex and Pi share typed requests, topics, documents and queued lease handoffs through the real CLI', t => {
  const workspace = tempWorkspace(t, 'communication-cross-vendor-');
  const cli = jsonCall(binary, workspace, join(workspace, 'shared.sqlite'));
  const call = (command, input = {}, session) => cli(command, withReasoning(command, input), session);
  const vendors = ['claude', 'grok', 'codex', 'pi'];
  const agents = Object.fromEntries(vendors.map(vendor =>
    [vendor, call('join', { name: vendor, vendor, task: 'Verify collaboration', status: 'busy' }).id]));
  assert.equal(call('peers').items.length, vendors.length);
  for (const session of Object.values(agents)) call('attach', { transport: 'raw' }, session);

  const doc = call('share_document', { name: 'review-evidence', content: 'API review evidence', context: { summary: 'Review API boundary', path: 'src/api', kind: 'tree' } }, agents.codex).document.name;
  const notes = call('context', { path: 'src/api' }, agents.claude);
  assert.ok(notes.items.some(item => item.name === doc));
  assert.equal(call('read_document', { name: doc }, agents.claude).content, 'API review evidence');
  for (const [index, vendor] of vendors.entries()) {
    const sender = agents[vendor], recipient = agents[vendors[(index + 1) % vendors.length]];
    const request = call('send_message', { to: recipient, body: `Review ${doc}`, key: `review-api-${vendor}`, conversationId: 'api-review' }, sender);
    assert.equal(call('hook', { format: 'json' }, recipient).action, true);
    const received = call('fetch', { incoming: true, type: 'message' }, recipient).items[0];
    assert.equal(received.data.messageId, request.id);
    assert.ok(Number.isSafeInteger(received.recordId) && !Object.hasOwn(received, 'id'));
    assert.equal(received.data.replyRequired, true);
    const reply = call('complete', { message: received.data.messageId, reply: 'Reviewed API; evidence is complete' }, recipient);
    const answer = call('fetch', { incoming: true, type: 'message' }, sender).items[0];
    assert.equal(answer.data.messageId, reply.id);
    assert.equal(answer.data.replyTo, request.id);
    assert.equal(answer.data.conversationId, 'api-review');
    assert.equal(answer.data.replyRequired, false);
    call('complete', { message: answer.data.messageId }, sender);
  }

  call('subscribe', { topics: ['api'] }, agents.grok);
  const topic = call('send_message', { topic: 'api', body: `Evidence: ${doc}` }, agents.codex);
  assert.equal(topic.recipients, 1);
  assert.equal(call('inbox', {}, agents.grok).items[0].replyRequired, false);
  call('complete', { message: topic.id }, agents.grok);
  call('subscribe', { topics: [] }, agents.grok);
  assert.equal(call('send_message', { topic: 'api', body: 'No subscriber remains' }, agents.codex).recipients, 0);
  const notice = call('notify_all', { body: 'API review finished' }, agents.codex);
  assert.equal(notice.recipients, vendors.length - 1);
  for (const vendor of vendors.filter(vendor => vendor !== 'codex')) call('complete', { message: notice.id }, agents[vendor]);

  const leased = call('lock', { path: 'src/api', kind: 'tree' }, agents.codex);
  assert.equal(leased.ok, true);
  const conflict = call('lock', { path: 'src/api/new.ts' }, agents.grok);
  assert.equal(conflict.ok, false);
  assert.equal(call(conflict.next.command, conflict.next.input, agents.grok).queued, true);
  assert.equal(call('lock', { path: 'src/api/new.ts', wait: true }, agents.pi).queued, true);
  assert.equal(call('check_write', { paths: [{ path: 'src/api/new.ts' }] }, agents.codex).ok, true);
  const granted = session => call('inbox', {}, session).items.filter(item => item.body.startsWith('Lease granted'));
  call('unlock', { leaseId: leased.lease.id }, agents.codex);
  assert.equal(granted(agents.grok).length, 1, 'the oldest waiter (Grok) is granted on release');
  assert.equal(granted(agents.pi).length, 0);
  assert.equal(call('check_write', { paths: [{ path: 'src/api/new.ts' }] }, agents.grok).ok, true);
  call('unlock', { leaseId: call('locks', { owner: agents.grok }, agents.grok).items[0].id }, agents.grok);
  assert.equal(granted(agents.pi).length, 1, 'Pi follows when Grok releases');
  call('unlock', { leaseId: call('locks', { owner: agents.pi }, agents.pi).items[0].id }, agents.pi);
  for (const session of Object.values(agents)) {
    const notices = call('inbox', {}, session).items.map(item => item.id);
    if (notices.length) call('complete', { messages: notices }, session);
  }
  assert.equal(call('health').counts.unacknowledged, 0);
  for (const session of Object.values(agents)) call('leave', {}, session);
  assert.equal(call('peers').items.length, 0);
});
