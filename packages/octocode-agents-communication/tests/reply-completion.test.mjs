import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';

const root = fileURLToPath(new URL('../', import.meta.url));
const target = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];
const binary = join(root, 'skills/octocode-agents-communication/scripts/bin', target, `octocode-agents-communication${process.platform === 'win32' ? '.exe' : ''}`);

function fixture(t) {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'communication-completion-')));
  const database = join(workspace, 'mail.sqlite');
  t.after(() => rmSync(workspace, { recursive: true, force: true }));
  const cli = (command, input, session) => JSON.parse(execFileSync(binary,
    [command, JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])],
    { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], timeout: 10000 }));
  const sender = cli('join', { name: 'requester', vendor: 'raw' }).id;
  const receiver = cli('join', { name: 'worker', vendor: 'raw' }).id;
  const other = cli('join', { name: 'other', vendor: 'raw' }).id;
  const send = (input, session = receiver) => cli('send_message', input, session);
  const request = cli('send_message', { to: receiver, body: 'Review the change', reasoning: 'Need review before release', key: 'request' }, sender);
  const db = new DatabaseSync(database);
  t.after(() => db.close());
  const ack = () => db.prepare('SELECT acknowledgedAt FROM deliveries WHERE message=? AND recipient=?').get(request.id, receiver).acknowledgedAt;
  const reply = { replyTo: request.id, body: 'Reviewed; checks pass', reasoning: 'Return completed review', key: 'reply', ackReply: true };
  return { cli, send, db, sender, receiver, other, request, reply, ack, workspace, database };
}

test('final reply and handling ACK commit together, retry is idempotent', t => {
    const f = fixture(t);
    const sent = f.send(f.reply);
    assert.equal(sent.acknowledged, true);
    const stamp = f.ack();
    assert.equal(typeof stamp, 'number');
    assert.deepEqual(f.send(f.reply), sent);
    assert.equal(f.ack(), stamp);
    assert.equal(f.db.prepare('SELECT count(*) n FROM messages WHERE sender=?').get(f.receiver).n, 1);
    assert.equal(f.db.prepare("SELECT count(*) n FROM audit WHERE kind='delivery.acknowledged'").get().n, 1);
  });
test('clarification remains pending; failed reply cannot complete parent', t => {
    const f = fixture(t);
    const clarification = { ...f.reply, ackReply: false, body: 'Which version?' };
    f.send(clarification);
    assert.equal(f.ack(), null);
    assert.throws(() => f.send(f.reply)); // Same key with changed body must roll back.
    assert.equal(f.ack(), null);
    // An expired handler cannot publish a final reply or acknowledge the request.
    f.db.exec('BEGIN IMMEDIATE');
    f.db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(f.receiver);
    f.db.exec('COMMIT');
    assert.throws(() => f.send({ ...f.reply, key: 'final' }));
    assert.equal(f.ack(), null);
    assert.equal(f.db.prepare('SELECT count(*) n FROM messages WHERE sender=?').get(f.receiver).n, 1);
  });
test('completion requires an incoming direct reply to the parent sender', t => {
    const f = fixture(t);
    for (const input of [
      { ...f.reply, replyTo: undefined, to: f.sender },
      { ...f.reply, to: f.other },
      { ...f.reply, topic: 'review' },
      { ...f.reply, ackReply: 'true' },
    ]) assert.throws(() => f.send(input));
    assert.throws(() => f.send(f.reply, f.sender)); // Sender can see parent, but did not receive it.
    assert.equal(f.ack(), null);
    assert.equal(f.db.prepare('SELECT count(*) n FROM messages').get().n, 1);
  });
test('explicit completion on an identical retry changes handling, not content', t => {
    const f = fixture(t);
    const first = f.send({ ...f.reply, ackReply: false });
    assert.equal(f.ack(), null);
    const final = f.send(f.reply);
    assert.equal(final.id, first.id);
    assert.equal(final.acknowledged, true);
    assert.equal(f.db.prepare('SELECT count(*) n FROM messages').get().n, 2);
});

test('bound MCP exposes and executes the atomic completion contract', t => {
  const f = fixture(t);
  const frames = [
    { jsonrpc: '2.0', id: 1, method: 'tools/list' },
    { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'send_message', arguments: f.reply } },
  ].map(JSON.stringify).join('\n') + '\n';
  const rows = execFileSync(binary, ['mcp', '--tools', 'send_message', '--workspace', f.workspace,
    '--database', f.database, '--session', f.receiver], { input: frames, encoding: 'utf8', timeout: 10000 })
    .trim().split('\n').map(JSON.parse);
  assert.equal(rows[0].result.tools[0].inputSchema.properties.ackReply.type, 'boolean');
  assert.notEqual(rows[1].result.isError, true);
  assert.equal(JSON.parse(rows[1].result.content[0].text).acknowledged, true);
  assert.equal(typeof f.ack(), 'number');
});
