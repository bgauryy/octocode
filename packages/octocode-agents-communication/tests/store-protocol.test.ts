// Behavior of the store (src/runtime/store.ts and its capability modules) over real temporary databases: sessions,
// message kinds and their rules; store-protocol-rules.test.ts covers keys, check-backs, handoff leases and evidence pins. Reads of a database written by the previous release are in database-legacy-compat.test.ts.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { join } from 'node:path';
import type { JsonRecord } from './json-record.ts';
import { answer, decision, issue, restate, root, sbar, task } from './helpers.ts';
import { fixture, refuses, runtime, trio } from './store-support.ts';

// --- Sessions ----------------------------------------------------------------------------------------------

test('Given a workspace, when agents join, then names are unique among live peers and identity reads return the binding', async t => {
  const f = fixture(t, { git: true });
  const first = await f.call('', 'join', { name: 'agent', vendor: 'generic', vendorSession: 'native-1' });
  const second = await f.call('', 'join', { name: 'agent', vendor: 'generic' });
  assert.equal(first.name, 'agent');
  assert.equal(second.name, 'agent-2');
  assert.equal(first.vendorSession, 'native-1');
  assert.equal('vendorSession' in second, false, 'null members are stripped');
  const binding = await f.call(first.id, 'binding');
  assert.equal(binding.active, true);
  assert.equal(binding.coordinationScope, f.store.coordinationScope);
  assert.equal(binding.workspace, f.workspace);
  const peers = await f.call(first.id, 'peers');
  assert.deepEqual(peers.items.map((peer: JsonRecord) => peer.name).sort(), ['agent', 'agent-2']);
  assert.deepEqual(peers.items.map((peer: JsonRecord) => peer.id), [first.id, second.id].sort());
});

test('Given a session, when it heartbeats, updates status and leaves, then presence, status and the checkpoint follow', async t => {
  const f = fixture(t);
  const id = await f.join('agent');
  const beat = await f.call(id, 'heartbeat', { ttlMs: 120000, task: 'porting', status: 'busy' });
  assert.equal(beat.alive, true);
  assert.deepEqual(await f.call(id, 'set_status', { status: 'blocked' }), { id, task: 'porting', status: 'blocked' });
  assert.equal((await f.call(id, 'binding')).checkpoint, undefined);
  await f.call(id, 'record', { type: 'checkpoint', data: { goal: 'port', next: 'test' } });
  const latest = await f.call(id, 'record', { type: 'checkpoint', data: { goal: 'port', done: ['docs'], next: 'run tests' } });
  await assert.rejects(f.call(id, 'record', { type: 'checkpoint', data: { goal: 'No next' } }), /next/);
  assert.deepEqual((await f.call(id, 'binding')).checkpoint, { recordId: latest.recordId, timestamp: latest.timestamp, data: latest.data }, 'the latest checkpoint wins');
  assert.deepEqual(await f.call(id, 'leave'), { left: true });
  const binding = await f.call(id, 'binding');
  assert.equal(binding.active, false);
  await refuses(f.call(id, 'heartbeat'), /Unknown or expired session/);
  await refuses(f.call(id, 'resume', { vendor: 'other' }), /Vendor mismatch or session still active/);
  const resumed = await f.call(id, 'resume', { vendor: 'generic' });
  assert.equal(resumed.id, id);
  assert.deepEqual(resumed.checkpoint.data, latest.data, 'resume returns the latest checkpoint');
  assert.ok(resumed.expiresAt > Date.now());
});

test('Given a live peer name, when another session renames to it or an unknown peer is addressed, then both are refused', async t => {
  const f = await trio(t);
  await refuses(f.call(f.alice, 'heartbeat', { name: 'bob' }), /belongs to a live peer/);
  await refuses(f.call(f.alice, 'send_message', { to: 'nobody', body: 'hi' }), /Unknown peer nobody/);
  assert.equal((await f.call(f.alice, 'send_message', { to: 'bob', body: 'hi' })).recipients, 1);
  await f.call(f.alice, 'heartbeat', { name: 'alice-renamed', branch: 'topic' });
  assert.equal((await f.call(f.alice, 'binding')).name, 'alice-renamed');
});

test('Given a database file, when it is replaced or removed, then the bound store refuses to continue', async t => {
  const f = fixture(t);
  const id = await f.join('agent');
  const { renameSync, writeFileSync, rmSync } = await import('node:fs');
  renameSync(f.file, f.file + '.moved');
  writeFileSync(f.file, '');
  await refuses(f.call(id, 'binding'), /replaced|no longer a file/);
  rmSync(f.file);
  await refuses(f.call(id, 'binding'), /disappeared/);
  renameSync(f.file + '.moved', f.file);
});

test('Given an unknown operation or invalid input, when it is called, then it is refused before touching the database', async t => {
  const f = fixture(t);
  await assert.rejects(f.call('', 'nonsense'), /nonsense|Unknown/);
  await assert.rejects(f.call('', 'join', { name: 'agent' }), /vendor/);
});

// --- Messages ------------------------------------------------------------------------------------------------

test('Given each message kind, when it is sent, then replyRequired and wake follow the kind; zero or two kinds are refused', async t => {
  const f = await trio(t);
  await refuses(f.call(f.alice, 'send_message', { to: 'bob' }), /exactly one message kind/);
  await refuses(f.call(f.alice, 'send_message', { to: 'bob', body: 'x', question: 'y?' }), /exactly one message kind/);
  const expected: Record<string, [boolean, string]> = {
    note: [false, 'passive'], question: [true, 'action'], task: [true, 'action'], decision: [true, 'action'], handoff: [true, 'action'],
    blocker: [false, 'action'], delta: [false, 'passive'], issue: [false, 'passive'], checkback: [false, 'passive'],
  };
  assert.deepEqual(Object.keys(expected).sort(), runtime.catalog.messageKinds().map(marker => marker.kind).sort(), 'every catalog kind has an expected policy');
  const ids: Record<string, number> = {};
  for (const { kind, field } of runtime.catalog.messageKinds()) if (kind !== 'checkback') ids[kind] = (await f.call(f.alice, 'send_message', { to: 'bob', [field]: samples(0)[kind] })).id;
  ids.checkback = (await f.call(f.bob, 'send_message', { to: 'alice', checkback: restate(ids.task) })).id;
  for (const [kind, [replyRequired, wake]] of Object.entries(expected)) {
    const item = await f.find(kind === 'checkback' ? f.alice : f.bob, ids[kind]);
    assert.deepEqual({ kind: item.kind, replyRequired: item.replyRequired, wake: item.wake }, { kind, replyRequired, wake });
  }
});

/** A filled payload for every message kind; `request` is the task a check-back restates. */
const samples = (request: number): Record<string, unknown> => ({
  note: 'Plain FYI', question: 'Which store?',
  task: task({ inputs: ['a.ts'], constraints: ['no new deps'], budget: '1h', checkBack: true, informed: ['carol'] }),
  decision: decision({ tradeoffs: ['sqlite is simple'], default: 'postgres', informed: ['carol'] }),
  handoff: { state: 'Half done', evidence: [{ claim: 'Code', source: 'src/a.ts:3' }], blockers: ['review'], next: 'Finish it', constraints: ['read only', 'no new deps'] },
  blocker: sbar, delta: { phase: 'verify', changes: ['tests pass'], blockers: ['CI slow'], next: 'merge' }, checkback: restate(request), issue,
});

test('Given each message kind, when it is stored, then the body is the kind title and every slot once, in catalog order', async t => {
  const f = await trio(t);
  const request = (await f.call(f.alice, 'send_message', { to: 'bob', task: task() })).id;
  const filled = samples(request);
  for (const { kind, field } of runtime.catalog.messageKinds()) {
    const value = filled[kind];
    assert.ok(value !== undefined, `${kind} has a sample`);
    const [from, to, reader] = kind === 'checkback' ? [f.bob, 'alice', f.alice] : [f.alice, 'bob', f.bob];
    // Slots arrive in reverse catalog order: the stored body must restore the catalog order.
    const input = typeof value === 'string' ? value : Object.fromEntries(Object.entries(value as JsonRecord).toReversed());
    const body = await f.body(reader, (await f.call(from, 'send_message', { to, [field]: input })).id);
    const spec: JsonRecord = runtime.protocols.schema(kind);
    if (spec.type === 'string') { assert.equal(body, value, kind); continue; }
    const slots = value as JsonRecord, lines: string[] = body.split('\n');
    assert.equal(lines[0], spec.title, kind);
    let at = 0, expectedLines = 1;
    const next = (label: string, matches: (line: string) => boolean) => {
      const index = lines.findIndex((line, position) => position > at && matches(line));
      assert.ok(index > at, `${kind}: ${label} follows line ${at} in\n${body}`);
      at = index;
    };
    for (const [key, schema] of Object.entries(spec.properties as JsonRecord)) {
      if (!(key in slots)) continue;
      const slot = slots[key];
      if (slot === true) { next(key, line => line === schema.title); expectedLines += 1; }
      else if (Array.isArray(slot)) {
        for (const item of slot) next(key, line => line === `- ${typeof item === 'object' ? `${item.claim} \u2014 ${item.source}` : item}`);
        expectedLines += 1 + slot.length;
      } else { next(key, line => line.endsWith(`: ${slot}`)); expectedLines += 1; }
    }
    assert.equal(lines.length, expectedLines, `${kind} renders nothing but its slots:\n${body}`);
  }
  // One exact rendering pins the label text and list layout.
  assert.equal(await f.body(f.bob, (await f.call(f.alice, 'send_message', { to: 'bob', task: filled.task })).id),
    'Task contract\nObjective: Review storage\nInputs:\n- a.ts\nConstraints:\n- no new deps\nOutput: Findings\nDefinition of Done: Given v7, when migrated, then tests pass\nBudget: 1h\nCheck back before completing: send checkback with this message id\nInformed (copied on the answer):\n- carol');
});

test('Given an FYI kind, when the recipient completes it, then it is acknowledged with no reply; a request refuses silent completion', async t => {
  const f = await trio(t);
  const send = async (input: Record<string, unknown>) => (await f.call(f.alice, 'send_message', { to: 'bob', ...input })).id;
  const fyis = [await send({ body: 'FYI' }), await send({ blocker: sbar }), await send({ delta: { changes: ['tests pass'] } }), await send({ issue })];
  const requests = [await send({ task: task() }), await send({ question: 'Why?' }), await send({ decision: decision() }), await send({ handoff: { state: 'x', next: 'y' } })];
  for (const id of requests) await refuses(f.call(f.bob, 'complete', { message: id }), /requires a final answer/, `request ${id}`);
  for (const id of fyis) {
    await refuses(f.call(f.bob, 'complete', answer(id, 'Unasked')), /informational and accepts no reply/, `FYI ${id}`);
    assert.deepEqual(await f.call(f.bob, 'complete', { message: id }), { completed: true, count: 1 });
  }
  assert.deepEqual(await f.inbox(f.alice), [], 'completing an FYI sends the sender nothing');
  assert.deepEqual((await f.inbox(f.bob)).map(item => item.id), requests);
  await refuses(f.call(f.carol, 'complete', { message: requests[0] }), /requires every ID|requires a final answer/);
});

test('Given a request, when it is answered, then the answer is a BLUF to the sender, idempotent, and the request leaves the inbox', async t => {
  const f = await trio(t);
  const asked = await f.call(f.alice, 'send_message', { to: 'bob', question: 'Why?', conversationId: 'thread-1' });
  const done = await f.call(f.bob, 'complete', answer(asked.id, 'Because', { reply: { conclusion: 'Because', confidence: 'high', unknowns: ['load'], next: 'measure' } }));
  assert.deepEqual({ completed: done.completed, count: done.count, recipients: done.recipients }, { completed: true, count: 1, recipients: 1 });
  const reply = await f.find(f.alice, done.id);
  assert.equal(reply.body, 'BLUF\nOutcome: done\nConclusion: Because\nConfidence: high\nUnknowns:\n- load\nNext: measure');
  assert.equal(reply.kind, 'answer');
  assert.equal(reply.replyTo, asked.id);
  assert.equal(reply.conversationId, 'thread-1');
  assert.equal(reply.reasoning, 'Outcome: done');
  assert.deepEqual((await f.inbox(f.bob)), []);
  const again = await f.call(f.bob, 'complete', answer(asked.id, 'Because', { reply: { conclusion: 'Because', confidence: 'high', unknowns: ['load'], next: 'measure' } }));
  assert.equal(again.id, done.id, 'the same reply is idempotent');
  await refuses(f.call(f.bob, 'complete', answer(asked.id, 'Different')), /Final reply already stored with different content|already completed/);
  await refuses(f.call(f.carol, 'complete', answer(asked.id, 'Hijack')), /received by this session|Unknown message/);
});

test('Given a non-request message, when a reply names it, then it is refused; replyTo is only for complete', async t => {
  const f = await trio(t);
  await refuses(f.call(f.alice, 'send_message', { to: 'bob', body: 'x', replyTo: 1 }), /Replies use complete/);
  await refuses(f.call(f.alice, 'send_message', { to: 'bob', topic: 'both', body: 'x' }), /exactly one recipient/);
});

test('Given topics and broadcasts, when messages are sent, then only live subscribers and live peers receive them', async t => {
  const f = await trio(t);
  assert.deepEqual(await f.call(f.bob, 'subscribe', { topics: ['build'] }), { subscribed: true });
  const topical = await f.call(f.alice, 'send_message', { topic: 'build', body: 'green' });
  assert.equal(topical.recipients, 1);
  assert.equal((await f.inbox(f.bob))[0].topic, 'build');
  assert.equal((await f.inbox(f.carol)).length, 0);
  const all = await f.call(f.alice, 'notify_all', { body: 'heads up' });
  assert.equal(all.recipients, 2);
  await f.call(f.carol, 'leave');
  assert.equal((await f.call(f.alice, 'notify_all', { body: 'again' })).recipients, 1);
  const offline = await f.call(f.alice, 'send_message', { to: 'carol', body: 'are you there' });
  assert.equal(offline.recipientOffline, true);
});

test('Given an inbox, when it is read repeatedly, then an unchanged empty inbox answers without a query and paging continues', async t => {
  const f = await trio(t);
  assert.deepEqual(await f.call(f.bob, 'inbox'), { items: [] });
  assert.deepEqual(await f.call(f.bob, 'inbox'), { items: [] });
  const sent = [];
  for (let n = 0; n < 3; n += 1) sent.push((await f.call(f.alice, 'send_message', { to: 'bob', body: `m${n}` })).id);
  assert.deepEqual((await f.inbox(f.bob)).map(item => item.id), sent, 'new mail invalidates the idle cache');
  assert.deepEqual((await f.call(f.bob, 'inbox', { after: sent[1] })).items.map((item: JsonRecord) => item.id), [sent[2]]);
  assert.deepEqual((await f.call(f.bob, 'inbox', { message: sent[0] })).items.map((item: JsonRecord) => item.body), ['m0']);
});

// --- Protocol rules: cross-references and approved fixes ------------------------------------------------------------

test('Given informed peers, when a task is sent, then names or IDs resolve, duplicates and the sender and target drop, and the body shows names', async t => {
  const f = await trio(t);
  const sent = await f.call(f.alice, 'send_message', { to: 'bob', task: task({ informed: ['carol', f.carol, 'alice', f.bob, 'bob'] }) });
  assert.match(await f.body(f.bob, sent.id), /\nInformed \(copied on the answer\):\n- carol$/);
  assert.equal((await f.inbox(f.carol)).length, 0, 'informed peers receive nothing until the answer');
  const only = await f.call(f.alice, 'send_message', { to: 'bob', task: task({ informed: ['alice', 'bob'] }) });
  assert.doesNotMatch(await f.body(f.bob, only.id), /Informed/);
  for (const input of [{ task: task({ informed: ['carol', 'nobody'] }) }, { decision: decision({ informed: ['nobody'] }) }]) await refuses(f.call(f.alice, 'send_message', { to: 'bob', ...input }), /Unknown peer nobody/);
  assert.equal((await f.inbox(f.bob)).length, 2, 'a refused message stores nothing');
});

test('Given an informed peer, when the request is answered, then the copy shows what was asked', async t => {
  const f = await trio(t);
  const sent = await f.call(f.alice, 'send_message', { to: 'bob', decision: decision({ informed: ['carol'] }) });
  await f.call(f.bob, 'complete', answer(sent.id, 'sqlite'));
  const copy = (await f.inbox(f.carol))[0];
  assert.equal(copy.body, `BLUF\nRe decision ${sent.id}: Which store?\nOutcome: done\nConclusion: sqlite`);
  assert.deepEqual(await f.call(f.carol, 'complete', { message: copy.id }), { completed: true, count: 1 });
  const plain = await f.call(f.alice, 'send_message', { to: 'bob', question: 'Why?' });
  const reply = await f.call(f.bob, 'complete', answer(plain.id, 'Because'));
  assert.equal(await f.body(f.alice, reply.id), 'BLUF\nOutcome: done\nConclusion: Because', 'a request without informed peers gets no context line');
});

test('Given a long request headline, when its answer is copied, then the context line keeps 200 code points', async t => {
  const f = await trio(t);
  const question = '\u{1F600}'.repeat(250);
  const sent = await f.call(f.alice, 'send_message', { to: 'bob', decision: decision({ question, informed: ['carol'] }) });
  await f.call(f.bob, 'complete', answer(sent.id, 'sqlite'));
  const line = (await f.inbox(f.carol))[0].body.split('\n')[1];
  assert.equal(line, `Re decision ${sent.id}: ${'\u{1F600}'.repeat(200)}\u2026`);
});

test('Given a decision, when its default or recommendation is not an option, or options repeat or are too few, then it is refused', async t => {
  const f = await trio(t);
  const send = (input: Record<string, unknown>) => f.call(f.alice, 'send_message', { to: 'bob', decision: input });
  await refuses(send(decision({ default: 'mysql' })), /Decision default must be one of its options/);
  await refuses(send(decision({ recommendation: 'mysql' })), /Decision recommendation must be one of its options/);
  await refuses(send(decision({ options: ['sqlite', 'sqlite'] })), /distinct/);
  await assert.rejects(send(decision({ options: ['sqlite'] })), /options/);
  await assert.rejects(send(decision({ default: undefined })), /default/);
  assert.equal((await f.inbox(f.bob)).length, 0);
  assert.ok((await send(decision({ default: 'postgres' }))).id);
});

test('Given a request that is not a task, when a check-back names it, then it is refused', async t => {
  const f = await trio(t);
  const requests = { question: { question: 'Why?' }, decision: { decision: decision() }, handoff: { handoff: { state: 'x', next: 'y' } } };
  for (const [kind, input] of Object.entries(requests)) {
    const asked = await f.call(f.alice, 'send_message', { to: 'bob', ...input });
    await refuses(f.call(f.bob, 'send_message', { to: 'alice', checkback: restate(asked.id) }), /pending task you received/, kind);
  }
});

test('Given a check-back, when it is addressed wrongly or names another conversation, then it is refused', async t => {
  const f = await trio(t);
  const asked = await f.call(f.alice, 'send_message', { to: 'bob', task: task(), conversationId: 'thread-1' });
  await refuses(f.call(f.bob, 'send_message', { to: 'carol', checkback: restate(asked.id) }), /Send the checkback to the sender/);
  await refuses(f.call(f.bob, 'send_message', { topic: 'x', checkback: restate(asked.id) }), /Send the checkback to the sender/);
  await refuses(f.call(f.bob, 'send_message', { to: 'alice', checkback: restate(asked.id), conversationId: 'other' }), /conversationId must match/);
  await refuses(f.call(f.carol, 'send_message', { to: 'alice', checkback: restate(asked.id) }), /pending task you received/);
  const ok = await f.call(f.bob, 'send_message', { to: 'alice', checkback: restate(asked.id) });
  assert.equal((await f.find(f.alice, ok.id)).conversationId, 'thread-1', 'the check-back inherits the request conversation');
});

test('Given a task asking for check-back, when it is completed done, partial or failed, then it is refused until the check-back is sent; refused and canceled need none', async t => {
  const f = await trio(t);
  for (const outcome of ['done', 'partial', 'failed']) {
    const asked = await f.call(f.alice, 'send_message', { to: 'bob', task: task({ checkBack: true }) });
    await refuses(f.call(f.bob, 'complete', { message: asked.id, outcome, reply: { conclusion: 'Stopped' } }), /asks for a check-back/, outcome);
    const first = await f.call(f.bob, 'send_message', { to: 'alice', checkback: restate(asked.id) });
    assert.equal((await f.call(f.bob, 'send_message', { to: 'alice', checkback: restate(asked.id), key: `again-${outcome}` })).id !== first.id, true, 'a duplicate check-back stays accepted');
    assert.equal((await f.call(f.bob, 'complete', { message: asked.id, outcome, reply: { conclusion: `Work ${outcome}` } })).completed, true, outcome);
  }
  for (const outcome of ['refused', 'canceled']) {
    const asked = await f.call(f.alice, 'send_message', { to: 'bob', task: task({ checkBack: true }) });
    assert.equal((await f.call(f.bob, 'complete', { message: asked.id, outcome, reply: { conclusion: 'No' } })).completed, true, outcome);
  }
});

test('Given empty lists in any slot, when a message or reply is sent, then it is refused', async t => {
  const f = await trio(t);
  await refuses(f.call(f.alice, 'send_message', { to: 'bob', delta: { changes: [], blockers: [] } }), /empty list/);
  await refuses(f.call(f.alice, 'send_message', { to: 'bob', task: task({ inputs: [] }) }), /empty list/);
  const asked = await f.call(f.alice, 'send_message', { to: 'bob', question: 'Why?' });
  await refuses(f.call(f.bob, 'complete', answer(asked.id, 'x', { reply: { conclusion: 'x', unknowns: [] } })), /empty list/);
  await refuses(f.call(f.alice, 'send_message', { to: 'bob', delta: { changes: ['  '] } }), /blank/);
});

test('Given a handoff with leases, when it is sent and completed, then rules hold and done moves the leases', async t => {
  const f = await trio(t);
  const held = await f.call(f.alice, 'lock', { path: 'a.txt' });
  const other = await f.call(f.alice, 'lock', { path: 'b.txt' });
  await refuses(f.call(f.alice, 'send_message', { topic: 't', handoff: { state: 's', next: 'n', leases: [held.lease.id] } }), /one recipient/);
  await refuses(f.call(f.alice, 'send_message', { to: 'bob', handoff: { state: 's', next: 'n', leases: [held.lease.id, 424242] } }), /live leases you own/);
  await refuses(f.call(f.bob, 'send_message', { to: 'alice', handoff: { state: 's', next: 'n', leases: [held.lease.id] } }), /live leases you own/);
  const sent = await f.call(f.alice, 'send_message', { to: 'bob', handoff: { state: 's', next: 'n', leases: [held.lease.id, other.lease.id] } });
  assert.match(await f.body(f.bob, sent.id), /Leases:\n- \d+ a\.txt \(file\)\n- \d+ b\.txt \(file\)/);
  await f.call(f.alice, 'unlock', { leaseId: other.lease.id });
  const done = await f.call(f.bob, 'complete', { message: sent.id, outcome: 'done', reply: { conclusion: 'Taken' } });
  assert.deepEqual(done.leases.map((lease: JsonRecord) => lease.path), ['a.txt']);
  assert.match(done.leasesNotMoved, /no longer holds/);
  assert.equal((await f.call(f.bob, 'locks', { owner: f.bob })).items.length, 1);
  assert.equal((await f.call(f.alice, 'locks', { owner: f.alice })).items.length, 0);
  const refusedHandoff = await f.call(f.alice, 'send_message', { to: 'bob', handoff: { state: 's', next: 'n' } });
  await f.call(f.bob, 'complete', { message: refusedHandoff.id, outcome: 'refused', reply: { conclusion: 'No' } });
});

test('Given a request that expires unacknowledged, when its sender heartbeats, then it is told once; a decision names its default', async t => {
  const f = await trio(t);
  const asked = await f.call(f.alice, 'send_message', { to: 'bob', decision: decision({ default: 'postgres' }), ttlMs: 1000, key: 'expiring' });
  const db = new DatabaseSync(f.file);
  db.prepare('UPDATE messages SET expiresAt=1 WHERE id=?').run(asked.id);
  db.close();
  await f.call(f.alice, 'heartbeat');
  await f.call(f.alice, 'heartbeat');
  const notices = (await f.inbox(f.alice)).filter(item => item.kind === 'blocker');
  assert.equal(notices.length, 1);
  assert.equal(notices[0].body, `SBAR\nSituation: Request ${asked.id} expired before bob acknowledged it\nBackground: Expiry stops delivery; submitted context may already be in progress\nAssessment: Handling is unconfirmed; the decision default applies: postgres\nRecommendation: Inspect delivery and the recipient before sending a new request with a new key`);
  assert.equal(notices[0].sender, f.alice);
  assert.equal(notices[0].wake, 'action');
});

// --- Kind taxonomy ---------------------------------------------------------------------------------------------

test('Given the catalog markers, when kinds are derived, then taxonomy, policy, send_message fields and the schema CHECK agree', () => {
  const markers = runtime.catalog.messageKinds();
  const kinds = runtime.protocols.messageKinds();
  assert.deepEqual(kinds, [...markers.map(marker => marker.kind), 'answer']);
  // The send_message schema lists exactly the marked fields as its kind group, and each is a property.
  const send: JsonRecord = runtime.catalog.definition('send_message').inputSchema ?? {};
  const group = send.allOf.map((member: JsonRecord) => member.oneOf.map((option: JsonRecord) => option.required[0])).find((names: string[]) => !names.includes('to'));
  assert.deepEqual(group, markers.map(marker => marker.field));
  for (const marker of markers) assert.ok(marker.field in send.properties, marker.field);
  assert.deepEqual(markers.map(marker => marker.field === 'body' ? 'note' : marker.field), markers.map(marker => marker.kind), 'only the plain note is stored under another field');
  // schema.sql: the kind list and the reply-required policy are the markers.
  const schemaSql = readFileSync(join(root, 'assets/schema.sql'), 'utf8');
  const check = /kind IN \(([^)]*)\)\s*\n?\s*AND \(kind IN \(([^)]*)\)\)=\(replyRequired=1\)/.exec(schemaSql);
  assert.ok(check, 'schema.sql carries the kind CHECK');
  const names = (list: string) => list.split(',').map(name => name.trim().slice(1, -1));
  assert.deepEqual(names(check[1]).sort(), [...kinds].sort(), 'the SQL kind CHECK lists every kind');
  assert.deepEqual(names(check[2]).sort(), markers.filter(marker => marker.request).map(marker => marker.kind).sort(), 'the SQL request set is the request marker');
  for (const marker of markers) {
    assert.equal(typeof marker.request, 'boolean', marker.kind);
    assert.equal(typeof marker.wake, 'boolean', marker.kind);
    if (marker.request) assert.ok(marker.wake, marker.kind + ': a request always wakes');
    assert.equal(runtime.protocols.isRequest(marker.kind), marker.request, marker.kind);
    assert.equal(runtime.protocols.wakesByDefault(marker.kind), marker.wake, marker.kind);
  }
  assert.equal(runtime.protocols.isRequest('answer'), false);
  assert.equal(runtime.protocols.wakesByDefault('answer'), false);
  for (const kind of kinds) assert.ok(runtime.protocols.schema(kind).title || runtime.protocols.schema(kind).type === 'string', kind);
});

test('Given the new slots, when they are blank or empty, then the message is refused; absent they change nothing', async t => {
  const f = await trio(t);
  const send = (input: Record<string, unknown>) => f.call(f.alice, 'send_message', { to: 'bob', ...input });
  await refuses(send({ handoff: { state: 'x', next: 'y', constraints: [] } }), /empty list/);
  await refuses(send({ handoff: { state: 'x', next: 'y', constraints: ['  '] } }), /blank/);
  await refuses(send({ delta: { changes: ['x'], phase: ' ' } }), /blank/);
  await refuses(send({ delta: { changes: ['x'], phase: 3 } }), /phase/);
  assert.equal(await f.body(f.bob, (await send({ handoff: { state: 'x', next: 'y' } })).id), 'Handoff packet\nState: x\nNext: y');
});

test('Given the catalog, when slots render, then titles carry the protocol meaning and the check-back text matches enforcement', () => {
  const slots = (kind: string) => runtime.protocols.schema(kind).properties as JsonRecord;
  assert.equal(slots('task').checkBack.title, 'Check back before completing: send checkback with this message id');
  assert.equal('description' in slots('task').checkBack, false, 'the title states the rule; no stale "before work" text remains');
  assert.equal(slots('task').informed.title, 'Informed (copied on the answer)');
  assert.equal(slots('decision').informed.title, slots('task').informed.title, 'one definition serves both kinds');
  assert.equal(slots('decision').default.title, 'Default if unanswered');
  assert.match(slots('issue').where.description, /path:line/);
  assert.match(slots('issue').who.description, /not an assignee/, 'Who names the affected party, not an owner');
});
