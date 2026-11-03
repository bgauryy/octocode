// Completion invariants over real databases: exact multipart replies, per-recipient handling, idempotency and atomicity.
import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { createInterface } from 'node:readline';
import { join } from 'node:path';
import { spawn, execFileSync } from './helpers.ts';
import { DatabaseSync } from 'node:sqlite';
import type { JsonRecord } from './json-record.ts';
import { binary, only, tempWorkspace, jsonCall } from './helpers.ts';

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-completion-', { real: true });
  const database = join(workspace, 'mail.sqlite');
  const cli = jsonCall(binary, workspace, database, { stdio: 'pipe', timeout: 10000 });
  const sender = cli('join', { name: 'requester', vendor: 'raw' }).id;
  const receiver = cli('join', { name: 'worker', vendor: 'raw' }).id;
  const other = cli('join', { name: 'other', vendor: 'raw' }).id;
  const send = (input: Record<string, unknown>, session = receiver) => cli('send_message', input, session);
  const complete = (input: Record<string, unknown>, session = receiver) => cli('complete', input, session);
  const request = cli('send_message', { to: receiver, parts: [{ text: 'Review the change' }], replyRequired: true, contextId: 'review:1', reasoning: 'Need review before release', key: 'request' }, sender);
  const db = new DatabaseSync(database);
  t.after(() => db.close());
  const stamp = () => only(db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=? AND recipient=?').get(request.id, receiver)).acknowledgedAt;
  const reply = { message: request.id, outcome: 'completed', reply: [{ text: 'Reviewed; checks pass' }], reasoning: 'Return completed review' };
  return { cli, send, complete, db, sender, receiver, other, request, reply, stamp, workspace, database };
}

test('a final multipart reply and handling commit atomically; identical retries return the same result', t => {
  const f = fixture(t), sent = f.complete(f.reply);
  assert.deepEqual({ completed: sent.completed, count: sent.count, recipients: sent.recipients }, { completed: true, count: 1, recipients: 1 });
  const at = f.stamp(); assert.equal(typeof at, 'number');
  assert.deepEqual(f.complete(f.reply), sent); assert.equal(f.stamp(), at);
  const inbox = f.cli('inbox', {}, f.sender).items;
  const answer = inbox.find((item: JsonRecord) => item.replyTo === f.request.id);
  assert.ok(answer); assert.deepEqual(answer.parts, f.reply.reply);
  assert.equal(answer.contextId, 'review:1'); assert.equal(answer.replyRequired, false);
  assert.equal(only(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(answer.id)).acknowledgedAt, null, 'the sender still handles the reply');
});

test('a completion outcome is restricted to final states and replies preserve arbitrary parts exactly', t => {
  const f = fixture(t);
  const rich = [{ data: { conclusion: 'Reviewed', evidence: [{ source: 'src/api.ts:4' }] } }, { url: 'https://example.invalid/review', mediaType: 'text/html', filename: 'review.html' }];
  for (const outcome of ['needs-input', 'Done', 'partial', 1]) assert.throws(() => f.complete({ message: f.request.id, outcome, reply: rich }), JSON.stringify(outcome));
  assert.throws(() => f.complete({ message: f.request.id, outcome: 'completed' }));
  assert.throws(() => f.complete({ messages: [f.request.id], outcome: 'completed', reply: rich }));
  assert.equal(f.stamp(), null);
  const done = f.complete({ message: f.request.id, outcome: 'rejected', reply: rich });
  const result = f.cli('inbox', {}, f.sender).items.find((item: JsonRecord) => item.id === done.id);
  assert.deepEqual(result.parts, rich);
  assert.equal(result.replyRequired, false);
});

test('all four terminal outcomes complete once; repeated final replies are idempotent and conflicting outcomes fail', t => {
  for (const outcome of ['completed', 'failed', 'rejected', 'canceled']) {
    const f = fixture(t);
    const input = { message: f.request.id, outcome, reply: [{ text: `Final: ${outcome}` }] };
    const first = f.complete(input);
    assert.deepEqual(f.complete(input), first, `${outcome} retry returns the original final answer`);
    assert.throws(() => f.complete({ ...input, outcome: outcome === 'completed' ? 'failed' : 'completed' }), /different|conflict|already/i);
    assert.equal(typeof f.stamp(), 'number');
  }
});

test('progress messages leave a reply-required request pending until final completion', t => {
  const f = fixture(t);
  const progress = f.send({ to: f.sender, parts: [{ text: 'Still checking' }], contextId: 'review:1', reasoning: 'Report interim progress without completing the request' }, f.receiver);
  assert.equal(progress.replyTo, undefined);
  assert.equal(f.stamp(), null);
  assert.equal(f.cli('inbox', {}, f.receiver).items.some((item: JsonRecord) => item.id === f.request.id), true);
  const final = f.complete(f.reply);
  assert.equal(f.stamp() !== null, true);
  assert.equal(f.cli('inbox', {}, f.sender).items.some((item: JsonRecord) => item.id === progress.id), true);
  assert.ok(final.id > progress.id);
});

test('messages without replyRequired can be silently completed, while mixed batches handle each delivery independently', t => {
  const f = fixture(t);
  const notice = f.cli('send_message', { to: f.receiver, parts: [{ text: 'FYI' }], reasoning: 'Send an informational note' }, f.sender);
  assert.deepEqual(f.complete({ message: notice.id }), { completed: true, count: 1 });
  assert.deepEqual(f.complete({ message: notice.id }), { completed: true, count: 1 });
  assert.equal(f.cli('inbox', {}, f.sender).items.length, 0, 'silent handling sends no answer');
  const second = f.cli('send_message', { to: f.receiver, parts: [{ text: 'Another FYI' }], reasoning: 'Send another informational note' }, f.sender);
  assert.deepEqual(f.complete({ messages: [notice.id, second.id] }), { completed: true, count: 2 });
});

test('completion rejects unknown or foreign identities and removed message-specific fields', t => {
  const f = fixture(t);
  for (const input of [
    {}, { message: 999999 }, { message: f.request.id, outcome: 'completed' },
    { ...f.reply, to: f.other }, { ...f.reply, replyTo: f.request.id }, { ...f.reply, body: 'old' },
    { ...f.reply, key: 'caller-key' }, { ...f.reply, wake: 'passive' }, { ...f.reply, ttlMs: 1000 },
    { ...f.reply, ackReply: true }, { ...f.reply, session: f.other }, { messages: [f.request.id], outcome: 'completed', reply: f.reply.reply },
  ]) assert.throws(() => f.complete(input), JSON.stringify(input));
  assert.throws(() => f.complete(f.reply, f.sender));
  assert.throws(() => f.complete(f.reply, f.other));
  assert.throws(() => f.send({ to: f.sender, parts: [{ text: 'Unasked answer' }], replyTo: f.request.id }));
  assert.equal(f.stamp(), null);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n, 1);
});

test('a database failure while storing the reply rolls back both the answer and handling', async t => {
  const f = fixture(t), initial = only(f.db.prepare('SELECT count(*) n FROM records').get()).n;
  const child = spawn(binary, ['mcp', '--tools', 'messages', '--workspace', f.workspace, '--database', f.database, '--session', f.receiver]);
  const lines = createInterface({ input: child.stdout });
  let sequence = 0;
  t.after(() => { lines.close(); child.kill('SIGKILL'); });
  const request = (method: string, params: JsonRecord) => new Promise<JsonRecord>((resolve, reject) => {
    const timer = setTimeout(() => { lines.off('line', onLine); reject(new Error('MCP response timeout')); }, 10000);
    const onLine = (line: string) => { clearTimeout(timer); resolve(JSON.parse(line)); };
    lines.once('line', onLine);
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: ++sequence, method, params }) + '\n');
  });
  const initialized = await request('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'rollback-test', version: '1' } });
  assert.deepEqual(initialized.result.capabilities, { tools: {} });
  const complete = () => request('tools/call', { name: 'messages', arguments: { type: 'complete', input: f.reply } });
  f.db.exec(`CREATE TRIGGER fail_reply BEFORE INSERT ON messages WHEN NEW.replyTo=${f.request.id} BEGIN SELECT RAISE(ABORT,'Injected reply failure'); END`);
  const failed = await complete(); assert.equal(failed.result.isError, true);
  assert.match(failed.result.content[0].text, /Injected reply failure/);
  assert.equal(f.stamp(), null);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n, 1);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM records').get()).n, initial);
  f.db.exec('DROP TRIGGER fail_reply');
  f.db.exec("CREATE TRIGGER fail_completion BEFORE UPDATE OF acknowledgedAt ON deliveries BEGIN SELECT RAISE(ABORT,'Injected completion failure'); END");
  const handlingFailure = await complete(); assert.equal(handlingFailure.result.isError, true);
  assert.match(handlingFailure.result.content[0].text, /Injected completion failure/);
  assert.equal(f.stamp(), null);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n, 1);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM records').get()).n, initial);
  f.db.exec('DROP TRIGGER fail_completion');
  assert.equal(JSON.parse((await complete()).result.content[0].text).completed, true);
  child.stdin.end();
});

test('MCP exposes the messages family and preserves completion correlation', t => {
  const f = fixture(t);
  const frames = [
    { id: 1, method: 'tools/list' },
    { id: 2, method: 'tools/call', params: { name: 'ack', arguments: { message: f.request.id } } },
    { id: 3, method: 'tools/call', params: { name: 'messages', arguments: { type: 'complete', input: f.reply } } },
  ].map(row => JSON.stringify({ jsonrpc: '2.0', ...row })).join('\n') + '\n';
  const rows = execFileSync(binary, ['mcp', '--tools', 'messages', '--workspace', f.workspace, '--database', f.database, '--session', f.receiver], { input: frames, encoding: 'utf8', timeout: 10000 }).trim().split('\n').map((line): JsonRecord => JSON.parse(line));
  assert.deepEqual(rows[0].result.tools.map((tool: JsonRecord) => tool.name), ['messages']);
  assert.ok(rows[1].error || rows[1].result?.isError, 'the former ack alias is not exposed');
  assert.equal(JSON.parse(rows[2].result.content[0].text).completed, true);
  assert.equal(typeof f.stamp(), 'number');
});


test('required answers cannot be omitted individually or through a mixed completion batch',t=>{
  const f=fixture(t);
  const notice=f.cli('send_message',{to:f.receiver,parts:[{text:'FYI'}],reasoning:'Context only'},f.sender);
  assert.throws(()=>f.complete({message:f.request.id}));
  assert.throws(()=>f.complete({messages:[notice.id,f.request.id]}));
  assert.equal(f.stamp(),null);
  assert.equal(only(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(notice.id)).acknowledgedAt,null);
  assert.equal(only(f.db.prepare("SELECT count(*) n FROM records WHERE type='delivery.acknowledged'").get()).n,0);
  f.send({to:f.sender,parts:[{text:'Still reviewing'}],reasoning:'Partial work'});
  assert.throws(()=>f.complete({message:f.request.id}),'Partial reply is not a final answer');
  f.complete(f.reply);
  assert.deepEqual(f.complete({messages:[notice.id,f.request.id]}),{completed:true,count:2});
});


test('answers and notices cannot start reply loops; new work requires a new root request',t=>{
  const f=fixture(t),answer=f.complete(f.reply);
  assert.throws(()=>f.cli('send_message',{replyTo:answer.id,parts:[{text:'Thanks; more details?'}],reasoning:'Reply loop'},f.sender));
  assert.throws(()=>f.cli('complete',{message:answer.id,outcome:'completed',reply:[{text:'Thanks'}]},f.sender));
  assert.deepEqual(f.cli('complete',{message:answer.id},f.sender),{completed:true,count:1});
  const notice=f.cli('send_message',{to:f.receiver,parts:[{text:'Notice'}],reasoning:'FYI'},f.sender);
  assert.throws(()=>f.send({replyTo:notice.id,parts:[{text:'Unasked answer'}],reasoning:'Loop'}));
  assert.throws(()=>f.complete({message:notice.id,outcome:'completed',reply:[{text:'Unasked answer'}]}));
  assert.deepEqual(f.complete({message:notice.id}),{completed:true,count:1});
  const next=f.cli('send_message',{to:f.receiver,parts:[{text:'Please investigate a new issue'}],replyRequired:true,reasoning:'New work'},f.sender);
  assert.equal(f.complete({message:next.id,outcome:'completed',reply:[{text:'Investigated'}]}).completed,true);
});


test('SQL cannot bypass required answers, reply policy or immutable sender intent',t=>{
  const f=fixture(t);
  const partial=f.send({to:f.sender,parts:[{text:'Still reviewing'}],reasoning:'Partial progress'});
  const before=only(f.db.prepare('SELECT count(*) n FROM records').get()).n;
  assert.throws(()=>f.db.prepare('UPDATE deliveries SET acknowledgedAt=? WHERE message=? AND recipient=?').run(Date.now(),f.request.id,f.receiver));
  assert.equal(f.stamp(),null);assert.equal(only(f.db.prepare('SELECT count(*) n FROM records').get()).n,before);
  const notice=f.cli('send_message',{to:f.receiver,parts:[{text:'FYI only'}],reasoning:'Inform'},f.sender);
  const count=only(f.db.prepare('SELECT count(*) n FROM messages').get()).n;
  const audits=only(f.db.prepare('SELECT count(*) n FROM records').get()).n;
  const insert=f.db.prepare("INSERT INTO messages(sender,target,parts,key,expiresAt,reasoning,replyTo,replyRequired,outcome) VALUES(?,?,?,?,?,?,?,?,'completed')");
  assert.throws(()=>insert.run(f.receiver,f.sender,JSON.stringify([{text:'Reply to FYI'}]),'sql-notice',Date.now()+60000,'Bypass',notice.id,0));
  assert.throws(()=>insert.run(f.receiver,f.sender,JSON.stringify([{text:'Reply requesting reply'}]),'sql-loop',Date.now()+60000,'Bypass',f.request.id,1));
  assert.throws(()=>f.db.prepare('UPDATE messages SET replyRequired=0 WHERE id=?').run(f.request.id));
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n,count);
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM records').get()).n,audits);
  assert.equal(only(f.db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=?').get(notice.id)).acknowledgedAt,null);
  assert.equal(f.cli('inbox',{message:partial.id},f.sender).items[0].replyRequired,false);
});
