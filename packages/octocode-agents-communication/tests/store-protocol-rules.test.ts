// The message-protocol cross-reference rules of the store over real temporary databases: idempotency keys, informed
// peers, check-backs, handoff leases and evidence pins. protocol-rules.test.ts proves the CLI and MCP surface the same rules.
import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { JsonRecord } from './json-record.ts';
import { answer, decision, restate, task, tempWorkspace } from './helpers.ts';
import { callStore, refuses, Store, trio } from './store-support.ts';

type Trio = Awaited<ReturnType<typeof trio>>;
const handoffOf = (leases: number[]) => ({ state: 'Half done', next: 'Finish it', leases });
const owners = async (f: Trio, session: string): Promise<{ path: string; owner: string }[]> => (await f.call(session, 'locks')).items.map((item: JsonRecord) => ({ path: item.path.split('/').pop(), owner: item.owner }));

// --- Idempotency keys ------------------------------------------------------------------------------------------

test('Given a key, when the same content is sent again, then the stored message is returned and nothing is delivered twice', async t => {
  const f = await trio(t);
  const input = { to: 'bob', task: task({ informed: ['carol'] }), key: 'once', ttlMs: 600000, wake: 'action' };
  const first = await f.call(f.alice, 'send_message', input);
  assert.deepEqual(await f.call(f.alice, 'send_message', { ...input }), first);
  assert.deepEqual((await f.inbox(f.bob)).map(item => item.id), [first.id]);
});

test('Given a key, when any identity field of the content changes, then it is refused as a reused key and nothing is delivered', async t => {
  const f = await trio(t);
  const base = { to: 'bob', body: 'FYI', key: 'k', reasoning: 'Verify keyed delivery stays immutable', ttlMs: 600000 };
  await f.call(f.alice, 'send_message', base);
  const { body: _body, ...bodiless } = base;
  const changes = {
    target: { ...base, to: 'carol' }, body: { ...base, body: 'Different' }, kind: { ...bodiless, question: 'FYI' },
    reasoning: { ...base, reasoning: 'Verify a changed reason is refused' }, ttl: { ...base, ttlMs: 300000 },
    wake: { ...base, wake: 'action' }, conversation: { ...base, conversationId: 'thread' },
  };
  for (const [name, input] of Object.entries(changes)) await refuses(f.call(f.alice, 'send_message', input), /Message key reused with different content/, name);
  const structured = { to: 'bob', task: task({ inputs: ['a.ts'] }), key: 'k2' };
  await f.call(f.alice, 'send_message', structured);
  await refuses(f.call(f.alice, 'send_message', { ...structured, task: task({ inputs: ['b.ts'] }) }), /Message key reused with different content/, 'changed slot');
  await refuses(f.call(f.alice, 'send_message', { ...structured, task: task({ inputs: ['a.ts'], budget: '1h' }) }), /Message key reused with different content/, 'added slot');
  assert.equal((await f.inbox(f.bob)).length, 2, 'refused retries deliver nothing');
  assert.equal((await f.inbox(f.carol)).length, 0);
});

test('Given a key, when another sender uses it it is a separate message; reserved prefixes are refused', async t => {
  const f = await trio(t);
  const mine = await f.call(f.alice, 'send_message', { to: 'bob', body: 'from alice', key: 'shared' });
  const theirs = await f.call(f.carol, 'send_message', { to: 'bob', body: 'from carol', key: 'shared' });
  assert.notEqual(mine.id, theirs.id);
  for (const prefix of ['complete:', 'expired:', 'retry-exhausted:']) await refuses(f.call(f.alice, 'send_message', { to: 'bob', body: 'spoof', key: `${prefix}1` }), /reserved/, prefix);
});

// --- Informed (RACI) -------------------------------------------------------------------------------------------

test('Given an informed peer on a task or decision, when it is completed, then the peer gets the answer as an FYI exactly once', async t => {
  const f = await trio(t);
  const asked = await f.call(f.alice, 'send_message', { to: 'bob', task: task({ informed: ['carol', f.carol] }) });
  const decided = await f.call(f.alice, 'send_message', { to: 'bob', decision: decision({ informed: ['carol'] }) });
  const answers = [await f.call(f.bob, 'complete', answer(asked.id, 'Storage is fine')), await f.call(f.bob, 'complete', answer(decided.id, 'sqlite'))];
  const copies = await f.inbox(f.carol);
  assert.deepEqual(copies.map(item => item.id), answers.map(item => item.id), 'one copy per answer');
  for (const copy of copies) assert.deepEqual({ kind: copy.kind, replyRequired: copy.replyRequired }, { kind: 'answer', replyRequired: false });
  assert.deepEqual(await f.call(f.bob, 'complete', answer(asked.id, 'Storage is fine')), answers[0], 'an identical retry adds no copy');
  assert.equal((await f.inbox(f.carol)).length, 2);
  for (const copy of copies) assert.deepEqual(await f.call(f.carol, 'complete', { message: copy.id }), { completed: true, count: 1 });
  assert.equal((await f.inbox(f.carol)).length, 0);
});

// --- Check-back ------------------------------------------------------------------------------------------------

test('Given a check-back, when its request is not a pending task this session received, then it is refused', async t => {
  const f = await trio(t);
  const asked = await f.call(f.alice, 'send_message', { to: 'bob', task: task({ checkBack: true }) });
  const send = (session: string, request: number, to: string) => f.call(session, 'send_message', { to, checkback: restate(request) });
  const refused = /must be a pending task you received/;
  await refuses(send(f.bob, 999, 'alice'), refused, 'unknown id');
  await refuses(send(f.carol, asked.id, 'alice'), refused, 'received by someone else');
  await refuses(send(f.alice, asked.id, 'bob'), refused, 'sent, not received, by the caller');
  const note = await f.call(f.alice, 'send_message', { to: 'bob', body: 'FYI' });
  await refuses(send(f.bob, note.id, 'alice'), refused, 'an FYI is not a request');
  await f.call(f.bob, 'complete', { message: asked.id, outcome: 'refused', reply: { conclusion: 'Declined' } });
  await refuses(send(f.bob, asked.id, 'alice'), refused, 'already answered');
});

test('Given a conversation, when a check-back names it explicitly, then only the request conversation is accepted', async t => {
  const f = await trio(t);
  const threaded = await f.call(f.alice, 'send_message', { to: 'bob', task: task(), conversationId: 'thread-1' });
  const explicit = await f.call(f.bob, 'send_message', { to: 'alice', checkback: restate(threaded.id), conversationId: 'thread-1' });
  assert.equal((await f.find(f.alice, explicit.id)).conversationId, 'thread-1');
  const bare = await f.call(f.alice, 'send_message', { to: 'bob', task: task() });
  await refuses(f.call(f.bob, 'send_message', { to: 'alice', checkback: restate(bare.id), conversationId: 'invented' }), /conversationId must match its request/, 'a request without a conversation admits none');
});

test('Given a task asking for check-back, when the check-back names another task, then the gate stays closed; a task without checkBack needs none', async t => {
  const f = await trio(t);
  const asked = await f.call(f.alice, 'send_message', { to: 'bob', task: task({ checkBack: true }) });
  const other = await f.call(f.alice, 'send_message', { to: 'bob', task: task() });
  await f.call(f.bob, 'send_message', { to: 'alice', checkback: restate(other.id) });
  await refuses(f.call(f.bob, 'complete', answer(asked.id)), /asks for a check-back/);
  assert.equal((await f.call(f.bob, 'complete', answer(other.id))).completed, true);
});

// --- Handoff: leases -------------------------------------------------------------------------------------------

test('Given a handoff with leases, when it goes to a topic, then it is refused and the leases stay; without leases a topic is allowed', async t => {
  const f = await trio(t);
  await f.call(f.bob, 'subscribe', { topics: ['crew'] });
  const lease = (await f.call(f.alice, 'lock', { path: 'a.ts' })).lease;
  await refuses(f.call(f.alice, 'send_message', { topic: 'crew', handoff: handoffOf([lease.id]) }), /needs one recipient: use to/);
  assert.equal((await f.inbox(f.bob)).length, 0);
  assert.deepEqual(await owners(f, f.alice), [{ path: 'a.ts', owner: f.alice }]);
  assert.equal((await f.call(f.alice, 'send_message', { topic: 'crew', handoff: { state: 'x', next: 'y' } })).recipients, 1);
});

test('Given a handoff with leases, when they are released or partly foreign, then it is refused', async t => {
  const f = await trio(t);
  const mine = (await f.call(f.alice, 'lock', { path: 'a.ts' })).lease, theirs = (await f.call(f.bob, 'lock', { path: 'b.ts' })).lease;
  const released = (await f.call(f.alice, 'lock', { path: 'c.ts' })).lease;
  await f.call(f.alice, 'unlock', { leaseId: released.id });
  for (const [name, leases] of Object.entries({ released: [released.id], 'partly foreign': [mine.id, theirs.id] })) {
    await refuses(f.call(f.alice, 'send_message', { to: 'bob', handoff: handoffOf(leases) }), /live leases you own/, name);
  }
  assert.equal((await f.inbox(f.bob)).length, 0);
});

/** Two linked Git worktrees of one repository sharing a database, each with its own bound store. */
function linkedWorktrees(t: TestContext) {
  const directory = realpathSync(tempWorkspace(t, 'store-protocol-worktrees-', { real: true })), main = join(directory, 'main'), topic = join(directory, 'topic');
  mkdirSync(main); mkdirSync(topic);
  execFileSync('git', ['-c', 'init.defaultBranch=main', 'init', '--quiet', main], { stdio: 'pipe' });
  const metadata = join(main, '.git', 'worktrees', 'topic');
  mkdirSync(metadata, { recursive: true });
  writeFileSync(join(topic, '.git'), `gitdir: ${metadata}\n`);
  writeFileSync(join(metadata, 'HEAD'), 'ref: refs/heads/feature/topic\n');
  writeFileSync(join(metadata, 'commondir'), '../..\n');
  writeFileSync(join(metadata, 'gitdir'), `${join(topic, '.git')}\n`);
  const database = join(directory, 'shared.sqlite');
  const [mainStore, topicStore] = [main, topic].map(workspace => new Store(database, workspace, false, true));
  t.after(() => { mainStore.close(); topicStore.close(); });
  return { main: mainStore, topic: topicStore };
}

test('Given a handoff with leases, when the recipient works in another worktree, then it is refused; without leases it crosses', async t => {
  const { main, topic } = linkedWorktrees(t);
  const a = (await callStore(main, '', 'join', { name: 'main-author', vendor: 'codex' })).id;
  const b = (await callStore(topic, '', 'join', { name: 'topic-reviewer', vendor: 'claude' })).id;
  const lease = (await callStore(main, a, 'lock', { path: 'a.ts' })).lease;
  await refuses(callStore(main, a, 'send_message', { to: b, handoff: handoffOf([lease.id]) }), /one worktree/);
  assert.equal((await callStore(topic, b, 'inbox')).items.length, 0);
  assert.equal((await callStore(main, a, 'send_message', { to: b, handoff: { state: 'x', next: 'y' } })).recipients, 1);
});

test('Given a handoff with leases, when the recipient completes it done, then the leases move once each and keep their path and kind', async t => {
  const f = await trio(t);
  f.mkdir('src');
  const first = (await f.call(f.alice, 'lock', { path: 'a.ts' })).lease, second = (await f.call(f.alice, 'lock_many', { paths: [{ path: 'src', kind: 'tree' }] })).leases[0];
  const sent = await f.call(f.alice, 'send_message', { to: 'bob', handoff: handoffOf([second.id, first.id, first.id]) });
  assert.match(await f.body(f.bob, sent.id), /\nLeases:\n- \d+ a\.ts \(file\)\n- \d+ src \(tree\)$/, 'leases render once each by relative path in lease-ID order');
  assert.deepEqual(await owners(f, f.bob), [{ path: 'a.ts', owner: f.alice }, { path: 'src', owner: f.alice }], 'until completion the sender owns them');
  const done = await f.call(f.bob, 'complete', answer(sent.id, 'Taken over'));
  assert.deepEqual(done.leases.map((item: JsonRecord) => [item.path, item.kind]), [['a.ts', 'file'], ['src', 'tree']]);
  assert.equal(done.leasesNotMoved, undefined);
  assert.deepEqual(await owners(f, f.bob), [{ path: 'a.ts', owner: f.bob }, { path: 'src', owner: f.bob }]);
  assert.match((await f.call(f.bob, 'locks')).items[0].reasoning, new RegExp(`^Handoff ${sent.id}: `));
});

test('Given a handoff whose sender lost every lease before completion, then nothing moves and leasesNotMoved says why', async t => {
  const f = await trio(t);
  const lease = (await f.call(f.alice, 'lock', { path: 'c.ts' })).lease;
  const sent = await f.call(f.alice, 'send_message', { to: 'bob', handoff: handoffOf([lease.id]) });
  await f.call(f.alice, 'unlock', { leaseId: lease.id });
  const done = await f.call(f.bob, 'complete', answer(sent.id, 'Nothing left'));
  assert.deepEqual(done.leases, []);
  assert.match(done.leasesNotMoved, /no longer holds some handed-off leases/);
  assert.deepEqual(await owners(f, f.bob), []);
});

test('Given a handoff with leases, when it is completed with any outcome but done, then the leases stay with the sender', async t => {
  const f = await trio(t);
  for (const outcome of ['partial', 'failed', 'refused', 'canceled']) {
    const lease = (await f.call(f.alice, 'lock', { path: `${outcome}.ts` })).lease;
    const sent = await f.call(f.alice, 'send_message', { to: 'bob', handoff: handoffOf([lease.id]) });
    const result = await f.call(f.bob, 'complete', { message: sent.id, outcome, reply: { conclusion: `Handoff ${outcome}` } });
    assert.deepEqual([result.leases, result.leasesNotMoved], [undefined, undefined], outcome);
  }
  const held = await owners(f, f.alice);
  assert.deepEqual(held.map(item => item.path).toSorted(), ['canceled.ts', 'failed.ts', 'partial.ts', 'refused.ts']);
  assert.ok(held.every(item => item.owner === f.alice));
});

test('Given a handoff without leases or a non-handoff request, when it is completed done, then the receipt has no lease fields', async t => {
  const f = await trio(t);
  await f.call(f.alice, 'lock', { path: 'a.ts' });
  const ids = [(await f.call(f.alice, 'send_message', { to: 'bob', handoff: { state: 'x', next: 'y' } })).id, (await f.call(f.alice, 'send_message', { to: 'bob', question: 'Why?' })).id];
  for (const id of ids) assert.deepEqual(Object.keys(await f.call(f.bob, 'complete', answer(id))).toSorted(), ['completed', 'count', 'id', 'recipients']);
  assert.deepEqual(await owners(f, f.alice), [{ path: 'a.ts', owner: f.alice }]);
});

// --- Evidence pins ---------------------------------------------------------------------------------------------

test('Given handoff evidence citing a document, when it is sent, then it is pinned to the revision; wrong pins and unknown documents are refused', async t => {
  const f = await trio(t);
  await f.call(f.alice, 'share_document', { name: 'draft.md', content: 'half done' });
  const send = (source: string) => f.call(f.alice, 'send_message', { to: 'bob', handoff: { state: 'x', next: 'y', evidence: [{ claim: 'Draft', source }] } });
  const bare = await send('document:draft.md');
  const sha = (await f.body(f.bob, bare.id)).match(/document:draft\.md@([0-9a-f]{64})/)?.[1];
  assert.match(sha ?? '', /^[0-9a-f]{64}$/, 'a bare citation is stored pinned');
  assert.equal(await f.body(f.bob, (await send(`document:draft.md@${sha}`)).id), await f.body(f.bob, bare.id), 'an explicit correct pin is accepted unchanged');
  await refuses(send(`document:draft.md@${'0'.repeat(64)}`), /another revision; cite document:draft\.md/);
  await refuses(send('document:missing.md'), /unknown document missing\.md/);
  await refuses(send(`document:missing.md@${sha}`), /unknown document missing\.md/);
  assert.match(await f.body(f.bob, (await send('src/a.ts:3')).id), /\n- Draft — src\/a\.ts:3\nNext: y$/, 'other sources are stored as written');
});

/** A reply citing `source` and a plain code location. */
const reply = (source: string) => ({ conclusion: 'Done', evidence: [{ claim: 'Report', source }, { claim: 'Code', source: 'src/a.ts:3' }] });

test('Given answer evidence citing a document, when the request is completed, then it is pinned the same way', async t => {
  const f = await trio(t);
  await f.call(f.bob, 'share_document', { name: 'report.md', content: 'findings' });
  const ask = async () => (await f.call(f.alice, 'send_message', { to: 'bob', question: 'Why?' })).id;
  const first = await ask();
  await refuses(f.call(f.bob, 'complete', { message: first, outcome: 'done', reply: reply('document:gone.md') }), /unknown document gone\.md/);
  await refuses(f.call(f.bob, 'complete', { message: first, outcome: 'done', reply: reply(`document:report.md@${'f'.repeat(64)}`) }), /another revision/);
  const done = await f.call(f.bob, 'complete', { message: first, outcome: 'done', reply: reply('document:report.md') });
  const text = await f.body(f.alice, done.id);
  assert.match(text, /^BLUF\nOutcome: done\nConclusion: Done\nEvidence:\n- Report — document:report\.md@[0-9a-f]{64}\n- Code — src\/a\.ts:3$/);
  const sha = text.match(/@([0-9a-f]{64})/)?.[1];
  const explicit = await f.call(f.bob, 'complete', { message: await ask(), outcome: 'done', reply: reply(`document:report.md@${sha}`) });
  assert.match(await f.body(f.alice, explicit.id), new RegExp(`document:report\\.md@${sha}\\n`));
});

test('Given evidence with a malformed document pin, when it is sent or completed, then it is refused instead of stored unpinned', async t => {
  const f = await trio(t);
  await f.call(f.alice, 'share_document', { name: 'd.md', content: 'x' });
  for (const source of ['document:d.md@abc', `document:d.md@${'G'.repeat(64)}`, 'document:', 'document: d.md']) {
    const evidence = [{ claim: 'c', source }];
    await assert.rejects(f.call(f.alice, 'send_message', { to: 'bob', handoff: { state: 'x', next: 'y', evidence } }), /revision|document/, `handoff ${source}`);
    const asked = await f.call(f.alice, 'send_message', { to: 'bob', question: 'Why?' });
    await assert.rejects(f.call(f.bob, 'complete', { message: asked.id, outcome: 'done', reply: { conclusion: 'Done', evidence } }), /revision|document/, `answer ${source}`);
  }
  assert.equal((await f.inbox(f.bob)).filter(item => item.kind === 'handoff').length, 0);
});
