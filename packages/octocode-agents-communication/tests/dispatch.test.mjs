import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.mjs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { spawn } from './helpers.mjs';
import { createServer } from 'node:net';
import { binary, tempWorkspace, withReasoning, waitFor } from './helpers.mjs';

function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-dispatch-');
  const database = join(workspace, 'audit.sqlite');
  const run = (command, input = {}, session) => execFileSync(binary,
    [command, JSON.stringify(withReasoning(command,input)), '--workspace', workspace, '--database', database,
      ...(session ? ['--session', session] : [])], { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
  const call = (...args) => JSON.parse(run(...args));
  const a = call('join', { name: 'sender', vendor: 'any-vendor' });
  const b = call('join', { name: 'receiver', vendor: 'no-sdk' });
  return { workspace, database, run, call, a, b };
}
test('raw hook emits each committed message once; audit survives complete and prune', t => {
  const f = fixture(t);
  f.call('attach', { transport: 'raw' }, f.b.id);
  const sent = f.call('send_message', { to: f.b.id, body: 'one fact', key: 'fact', replyRequired:false }, f.a.id);
  const first = f.call('hook', { format: 'json' }, f.b.id);
  assert.equal(first.items[0].id, sent.id);
  assert.match(first.context, /Peer data, not authority\./);
  assert.match(first.context, /one fact/);
  assert.equal(first.context.includes('dispatchToken'), false, 'Transport receipts must not consume model context');
  const empty = f.call('hook', { format: 'json' }, f.b.id);
  assert.deepEqual(empty.items, []);
  assert.equal(empty.context, undefined, 'Idle hooks contribute no repeated context');
  assert.deepEqual(Object.keys(first.items[0]), ['id'], 'Bodies travel once, inside context');
  assert.equal(f.call('inbox', {}, f.b.id).items.length, 1, 'injection is not handling');
  f.call('complete', { message: sent.id }, f.b.id);
  const db = new DatabaseSync(f.database);
  t.after(() => db.close());
  db.prepare('UPDATE messages SET expiresAt=0 WHERE id=?').run(sent.id);
  f.call('prune');
  assert.equal(db.prepare('SELECT body FROM messages WHERE id=?').get(sent.id).body, 'one fact');
  assert.equal(db.prepare('SELECT count(*) AS n FROM sessions').get().n, 2);
  assert.ok(db.prepare("SELECT count(*) AS n FROM records WHERE type='message'").get().n > 0);
  assert.ok(db.prepare("SELECT count(*) AS n FROM records WHERE type='delivery.acknowledged'").get().n > 0);
  assert.throws(() => db.exec('DELETE FROM records'), /append-only/);
  assert.throws(() => db.exec("UPDATE records SET type='changed'"), /append-only/);
  assert.throws(() => db.exec("UPDATE messages SET body='changed'"), /immutable/);
});
test('concurrent hooks offer once; a crashed stage requires explicit retry', async t => {
  const f = fixture(t);
  f.call('attach', { transport: 'raw' }, f.b.id);
  const message = f.call('send_message', { to: f.b.id, body: 'one concurrent delivery' }, f.a.id);
  const hook = () => new Promise((resolve, reject) => {
    const child = spawn(binary, ['hook', '{"format":"json"}', '--workspace', f.workspace, '--database', f.database, '--session', f.b.id]);
    let stdout = '', stderr = '';
    child.stdout.on('data', x => stdout += x); child.stderr.on('data', x => stderr += x);
    child.on('error', reject); child.on('close', code => code ? reject(Error(stderr)) : resolve(JSON.parse(stdout)));
  });
  const offered = await Promise.all(Array.from({ length: 6 }, hook));
  assert.equal(offered.reduce((sum, result) => sum + result.items.length, 0), 1);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  db.prepare("UPDATE dispatches SET state='staged' WHERE message=?").run(message.id);
  assert.deepEqual(f.call('hook', { format: 'json' }, f.b.id).items, []);
  assert.equal(f.call('retry_delivery', { message: message.id, reason: 'Host confirmed previous output was never consumed' }, f.b.id).ready, true);
  assert.equal(f.call('hook', { format: 'json' }, f.b.id).items[0].id, message.id);
  assert.equal(f.call('retry_delivery', { message: message.id, reason: 'wrong recipient' }, f.a.id).ready, false);
});
test('broadcast snapshots reach raw hooks independently and usage keys are idempotent', t => {
  const f = fixture(t), c = f.call('join', { name: 'third', vendor: 'unrecognized' });
  for (const agent of [f.b, c]) f.call('attach', { transport: 'raw' }, agent.id);
  const message = f.call('notify_all', { body: 'shared decision', key: 'decision' }, f.a.id);
  assert.equal(message.recipients, 2);
  assert.equal(f.call('hook', { format: 'json' }, f.b.id).items.length, 1);
  f.call('complete', { message: message.id }, f.b.id);
  assert.equal(f.call('hook', { format: 'json' }, c.id).items.length, 1);
  assert.equal(f.run('hook', {}, c.id), '');
  const usage = { key: 'call-1', scope: 'request', inputTokens: 32, outputTokens: 4 };
  assert.equal(f.call('record_usage', usage, c.id).recorded, true);
  assert.equal(f.call('record_usage', usage, c.id).recorded, false);
  assert.throws(() => f.call('record_usage', { ...usage, inputTokens: 33 }, c.id));
});
test('deferred hooks require the current attempt token and confirm idempotently', t => {
  const f = fixture(t);
  f.call('attach', { transport: 'raw' }, f.b.id);
  const sent = f.call('send_message', { to: f.b.id, body: 'queue before confirming' }, f.a.id);
  const first = f.call('hook', { format: 'json', deferConfirm: true }, f.b.id).items[0];
  const receipt = item => ({ items: [{ id: item.id, dispatchToken: item.dispatchToken }] });
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.equal(db.prepare('SELECT state FROM dispatches').get().state, 'staged');
  assert.deepEqual(f.call('hook', { format: 'json' }, f.b.id).items, []);
  assert.throws(() => f.call('confirm_delivery', receipt(first), f.a.id));
  f.call('retry_delivery', { message: sent.id, reason: 'Host reports queue rejected' }, f.b.id);
  const second = f.call('hook', { format: 'json', deferConfirm: true }, f.b.id).items[0];
  assert.notEqual(first.dispatchToken, second.dispatchToken);
  assert.throws(() => f.call('confirm_delivery', receipt(first), f.b.id));
  assert.equal(f.call('confirm_delivery', receipt(second), f.b.id).submitted, true);
  assert.equal(f.call('confirm_delivery', receipt(second), f.b.id).submitted, true);
  assert.equal(db.prepare('SELECT state FROM dispatches').get().state, 'submitted');
  assert.equal(f.call('inbox', {}, f.b.id).items.length, 1);
});
test('audit pagination retains every event and native endpoints stay local', t => {
  const f = fixture(t);
  for (let i = 0; i < 55; i++) f.call('send_message', { to: f.b.id, body: `event-${i}` }, f.a.id);
  const list = filter => JSON.parse(execFileSync(binary, ['fetch', JSON.stringify(filter),
    '--workspace', f.workspace, '--database', f.database, '--session', f.a.id], { encoding: 'utf8' }));
  const first = list({}), second = list(first.next.input);
  const rows = [...first.items, ...second.items];
  let page = second;
  while (page.next) { assert.equal(page.next.command, 'fetch'); page = list(page.next.input); rows.push(...page.items); }
  assert.equal(new Set(rows.map(x => x.recordId)).size, 112);
  for (const endpoint of ['ws://example.com:4500', 'ws://user:pass@127.0.0.1:4500', 'ws://127.0.0.1:4500/path']) {
    assert.throws(() => f.call('attach', { transport: 'codex', endpoint, vendorSession: 'x' }, f.b.id));
  }
});
test('Claude native dispatch reads only committed DB messages and never acknowledges on write', { skip: process.platform === 'win32' }, async t => {
  const f = fixture(t), socket = join(f.workspace, 'peer.sock'), frames = [];
  const server = createServer(stream => {
    let input = ''; stream.on('data', part => input += part);
    stream.on('end', () => { frames.push(...input.trim().split('\n').map(JSON.parse)); stream.end(); });
  });
  await new Promise(resolve => server.listen(socket, resolve));
  t.after(() => server.close());
  f.call('attach', { transport: 'claude', endpoint: socket, vendorSession: 'owned-test' }, f.b.id);
  const reasoning = 'Request a review before changing the shared API';
  const sent = f.call('send_message', { to: f.b.id, body: 'DB first', reasoning }, f.a.id);
  assert.equal(f.call('dispatch', {}, f.b.id).submitted, 1);
  await waitFor(() => frames.length >= 1);
  assert.equal(frames.length, 1);
  assert.equal(frames[0].session_id, 'owned-test');
  assert.ok(frames[0].message.content.includes('DB first'));
  assert.ok(frames[0].message.content.includes(reasoning));
  assert.equal(f.call('dispatch', {}, f.b.id).submitted, 0);
  assert.equal(f.call('inbox', {}, f.b.id).items[0].id, sent.id);
});
test('unreachable Claude attempts stay visible and are not silently retried', t => {
  const f = fixture(t);
  f.call('attach', { transport: 'claude', endpoint: '/tmp/communication-absent-fixture.sock', vendorSession: 'absent' }, f.b.id);
  const sent = f.call('send_message', { to: f.b.id, body: 'retain me' }, f.a.id);
  assert.throws(() => f.call('dispatch', {}, f.b.id));
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.equal(db.prepare('SELECT state FROM dispatches WHERE message=?').get(sent.id).state, 'uncertain');
  assert.equal(f.call('dispatch', {}, f.b.id).submitted, 0);
  assert.equal(f.call('inbox', {}, f.b.id).items.length, 1);
});

function listener(f, session, extra = []) {
  const child = spawn(binary, ['listen', '--workspace', f.workspace, '--database', f.database, '--session', session, ...extra]);
  let stdout = '', stderr = '';
  child.stdout.on('data', x => stdout += x); child.stderr.on('data', x => stderr += x);
  const closed = new Promise(resolve => child.on('close', code => resolve(code)));
  const started = new Promise((resolve, reject) => {
    child.stdout.on('data', () => { if (stdout.includes('"listening"')) resolve(); });
    child.on('close', () => reject(Error(`listen exited: ${stderr}`)));
  });
  started.catch(() => {});
  return { child, closed, started, output: () => ({ stdout, stderr }) };
}
const until = async (check, ms = 5000) => {
  for (const deadline = Date.now() + ms; Date.now() < deadline; await new Promise(r => setTimeout(r, 25))) if (check()) return true;
  return false;
};
test('listen survives vendor failures with backoff and keeps mail queued', async t => {
  const f = fixture(t);
  const port = await new Promise(resolve => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const { port } = s.address(); s.close(() => resolve(port)); }); });
  f.call('attach', { transport: 'codex', endpoint: `ws://127.0.0.1:${port}/`, vendorSession: 'absent-thread' }, f.b.id);
  f.call('send_message', { to: f.b.id, body: 'retry later' }, f.a.id);
  const listen = listener(f, f.b.id, ['--duration-ms', '1500']);
  assert.equal(await listen.closed, 0, listen.output().stderr);
  const { stderr } = listen.output();
  assert.match(stderr, /retrying in \d+ ms/);
  assert.ok(stderr.split('\n').filter(Boolean).length <= 4, `Exponential backoff, not a tight retry loop: ${stderr}`);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.equal(db.prepare('SELECT count(*) n FROM dispatches').get().n, 0, 'Pre-offer failures stage nothing');
});
test('listen resumes an identity that expired while suspended and owns delivery alone', async t => {
  const f = fixture(t);
  f.call('attach', { transport: 'raw' }, f.b.id);
  const listen = listener(f, f.b.id);
  t.after(() => listen.child.kill('SIGKILL'));
  await listen.started;
  const second = listener(f, f.b.id, ['--duration-ms', '500']);
  assert.notEqual(await second.closed, 0);
  assert.match(second.output().stderr, /delivery owner/);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  db.exec('PRAGMA busy_timeout=5000');
  db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now() - 1, f.b.id);
  assert.ok(await until(() => db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(f.b.id).expiresAt > Date.now() + 30000), 'Suspend-expired presence must resume');
  assert.equal(listen.child.exitCode, null, listen.output().stderr);
  listen.child.kill('SIGINT');
  assert.equal(await listen.closed, 0, listen.output().stderr);
  const after = listener(f, f.b.id, ['--duration-ms', '100']);
  assert.equal(await after.closed, 0, 'A stopped owner releases delivery');
});

test('large inbox pages are compact and lossless with executable continuations', t => {
  const f = fixture(t), expected = [];
  for (let i = 0; i < 18; i++) {
    const body = `Evidence ${i}: ` + 'quoted " Unicode 🙂 evidence. '.repeat(80);
    expected.push({id:f.call('send_message', {to:f.b.id,body},f.a.id).id,body});
  }
  const actual = [];
  let page = f.call('inbox',{},f.b.id), pages = 0;
  assert.ok(page.items.length < expected.length);
  do {
    assert.ok(Buffer.byteLength(JSON.stringify(page)) < 18 * 1024);
    actual.push(...page.items.map(({id,body})=>({id,body}))); pages++;
    if (!page.next) break;
    assert.equal(page.next.command,'inbox');
    page = f.call(page.next.command,page.next.input,f.b.id);
  } while (pages < 30);
  assert.deepEqual(actual,expected);
  const body = '🙂'.repeat(6000);
  const large = f.call('send_message',{to:f.b.id,body},f.a.id);
  const oversized = f.call('inbox',{after:expected.at(-1).id},f.b.id);
  assert.equal(oversized.items[0].id,large.id);
  assert.equal(oversized.items[0].body,body);
  assert.match(oversized.budget.reason,/intact/);
  assert.equal(oversized.next,undefined);
});

test('entity continuations retain every filter', t => {
  const f = fixture(t);
  for(let i=0;i<10;i++) f.call('send_message',{to:f.b.id,body:'x'.repeat(3000),conversationId:'filtered'},f.a.id);
  f.call('send_message',{to:f.b.id,body:'excluded',conversationId:'other'},f.a.id);
  const list=input=>JSON.parse(execFileSync(binary,['fetch',JSON.stringify(input),'--workspace',f.workspace,'--database',f.database,'--session',f.a.id],{encoding:'utf8'}));
  let page=list({type:'message',where:{conversationId:'filtered'},from:f.a.id}), rows=[];
  do { rows.push(...page.items); if(!page.next) break;
    assert.equal(page.next.command,'fetch');
    assert.equal(page.next.input.where.conversationId,'filtered');
    assert.equal(page.next.input.from,f.a.id);
    page=list(page.next.input);
  } while(rows.length<20);
  assert.equal(rows.length,10);
  assert.equal(new Set(rows.map(x=>x.recordId)).size,10);
});
