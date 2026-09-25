import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { readFileSync } from 'node:fs';

const root = fileURLToPath(new URL('../', import.meta.url));
const target = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];
const binary = join(root, 'skills/octocode-agents-communication/scripts/bin', target, 'octocode-agents-communication');
function fixture(t) {
  const workspace = mkdtempSync(join(tmpdir(), 'communication-dispatch-'));
  const database = join(workspace, 'audit.sqlite');
  t.after(() => rmSync(workspace, { recursive: true, force: true }));
  const run = (command, input = {}, session) => execFileSync(binary,
    [command, JSON.stringify(input), '--workspace', workspace, '--database', database,
      ...(session ? ['--session', session] : [])], { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
  const call = (...args) => JSON.parse(run(...args));
  const a = call('join', { name: 'sender', vendor: 'any-vendor' });
  const b = call('join', { name: 'receiver', vendor: 'no-sdk' });
  return { workspace, database, run, call, a, b };
}
test('raw hook emits each committed message once; audit survives ack and prune', t => {
  const f = fixture(t);
  f.call('attach', { transport: 'raw' }, f.b.id);
  const sent = f.call('send_message', { to: f.b.id, body: 'one fact', key: 'fact' }, f.a.id);
  const first = f.call('hook', { format: 'json' }, f.b.id);
  assert.equal(first.items[0].id, sent.id);
  assert.deepEqual(f.call('hook', { format: 'json' }, f.b.id).items, []);
  assert.equal(f.call('inbox', {}, f.b.id).items.length, 1, 'injection is not handling');
  f.call('ack', { message: sent.id }, f.b.id);
  const db = new DatabaseSync(f.database);
  t.after(() => db.close());
  db.prepare('UPDATE messages SET expiresAt=0 WHERE id=?').run(sent.id);
  f.call('prune');
  assert.equal(db.prepare('SELECT body FROM messages WHERE id=?').get(sent.id).body, 'one fact');
  assert.equal(db.prepare('SELECT count(*) AS n FROM sessions').get().n, 2);
  assert.ok(db.prepare("SELECT count(*) AS n FROM audit WHERE kind='message.created'").get().n > 0);
  assert.ok(db.prepare("SELECT count(*) AS n FROM audit WHERE kind='delivery.acknowledged'").get().n > 0);
  assert.throws(() => db.exec('DELETE FROM audit'), /append-only/);
  assert.throws(() => db.exec("UPDATE audit SET kind='changed'"), /append-only/);
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
  f.call('ack', { message: message.id }, f.b.id);
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
  const list = filter => JSON.parse(execFileSync(binary, ['entity', 'list', 'audit', JSON.stringify(filter),
    '--workspace', f.workspace, '--database', f.database, '--session', f.a.id], { encoding: 'utf8' }));
  const first = list({}), second = list({ after: first.next });
  assert.equal(first.items.length, 100);
  assert.equal(second.items.length, 12);
  assert.equal(second.next, null);
  assert.equal(new Set([...first.items, ...second.items].map(x => x.id)).size, 112);
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
  const sent = f.call('send_message', { to: f.b.id, body: 'DB first' }, f.a.id);
  assert.equal(f.call('dispatch', {}, f.b.id).submitted, 1);
  await new Promise(resolve => setTimeout(resolve, 30));
  assert.equal(frames.length, 1);
  assert.equal(frames[0].session_id, 'owned-test');
  assert.ok(frames[0].message.content.includes('DB first'));
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
test('explicit v1 migration preserves messages and refuses active workers', t => {
  const workspace = mkdtempSync(join(tmpdir(), 'communication-migrate-'));
  t.after(() => rmSync(workspace, { recursive: true, force: true }));
  const database = join(workspace, 'v1.sqlite'), db = new DatabaseSync(database);
  db.exec(readFileSync(join(root, 'rust/schema-v1.sql'), 'utf8'));
  db.exec('PRAGMA application_id=1329678147; PRAGMA user_version=1');
  db.prepare('INSERT INTO sessions VALUES(?,?,?,?,?,?)').run('old', workspace, 'old', 'generic', null, Date.now()+60000);
  db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt) VALUES(?,?,?,?,?)').run('old', 'old', 'historical', 'old-key', 0);
  const migrate = () => JSON.parse(execFileSync(binary, ['db', 'migrate', '--database', database], { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
  assert.throws(migrate);
  assert.equal(db.prepare('PRAGMA user_version').get().user_version, 1);
  db.exec('UPDATE sessions SET expiresAt=0');
  assert.equal(migrate().migrated, true);
  assert.equal(db.prepare('SELECT body FROM messages').get().body, 'historical');
  assert.equal(db.prepare('SELECT kind FROM audit').get().kind, 'session.imported');
  assert.equal(migrate().migrated, false);
  db.close();
});
