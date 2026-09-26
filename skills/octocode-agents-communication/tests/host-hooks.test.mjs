import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { createServer } from 'node:net';
import { randomUUID } from 'node:crypto';
import { binary, tempWorkspace, withReasoning } from './helpers.mjs';

function fixture(t, vendor) {
  const workspace = tempWorkspace(t, 'communication-hook-');
  const database = join(workspace, 'audit.sqlite');
  const flags = ['--workspace', workspace, '--database', database];
  const cli = (command, input = {}, session) => JSON.parse(execFileSync(binary,
    [command, JSON.stringify(withReasoning(command,input)), ...flags, ...(session ? ['--session', session] : [])], { encoding: 'utf8' }));
  const input = event => vendor === 'cursor'
    ? { hook_event_name: event, conversation_id: 'host-fixture', workspace_roots: [workspace] }
    : { hook_event_name: event, hookEventName: event, sessionId: 'host-fixture', workspaceRoot: workspace };
  const args = ['host-hook', '--vendor', vendor, ...flags];
  const hook = (event, override = {}) => JSON.parse(execFileSync(binary, args,
    { input: JSON.stringify({ ...input(event), ...override }), encoding: 'utf8', stdio: ['pipe','pipe','pipe'] }));
  return { workspace, database, flags, cli, input, args, hook };
}
for (const vendor of ['cursor', 'grok']) {
  test(`${vendor}: identity reuse, event-specific context, one-time delivery and teardown`, t => {
    const f = fixture(t, vendor);
    const initial = f.hook('sessionStart');
    const db = new DatabaseSync(f.database); t.after(() => db.close());
    const recipient = db.prepare('SELECT * FROM sessions').get();
    assert.equal(recipient.vendor, vendor);
    f.hook('sessionStart');
    assert.equal(db.prepare('SELECT count(*) n FROM sessions').get().n, 1);
    assert.equal(vendor === 'cursor' ? initial.additional_context.includes(recipient.id) : Object.keys(initial).length === 0, true);
    const sender = f.cli('join', { vendor: 'raw', name: 'sender' });
    const message = f.cli('send_message', { to: recipient.id, body: 'ONE DELIVERY' }, sender.id);
    const ignored = f.hook(vendor === 'cursor' ? 'beforeSubmitPrompt' : 'UserPromptSubmit');
    assert.deepEqual(ignored, vendor === 'cursor' ? { continue: true } : {});
    assert.equal(db.prepare('SELECT count(*) n FROM dispatches').get().n, 0);
    const delivered = f.hook('postToolUse');
    const content = vendor === 'cursor' ? delivered.additional_context : delivered.hookSpecificOutput.additionalContext;
    assert.ok(content.includes('ONE DELIVERY'));
    assert.equal(content.includes('Communication identity:'), vendor !== 'cursor');
    assert.deepEqual(f.hook('postToolUseFailure'), {});
    assert.equal(f.cli('inbox', {}, recipient.id).items[0].id, message.id);
    f.cli('ack', { message: message.id }, recipient.id);
    f.hook('sessionEnd');
    assert.ok(db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(recipient.id).expiresAt <= Date.now());
    f.hook('postToolUse');
    assert.equal(db.prepare('SELECT count(*) n FROM sessions WHERE vendor=?').get(vendor).n, 1);
  });
}
test('Grok context cap uses explicit references without clipping stored bodies', t => {
  const f = fixture(t, 'grok'); f.hook('SessionStart');
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const recipient = db.prepare('SELECT id FROM sessions').get().id;
  const sender = f.cli('join', { name: 'sender', vendor: 'raw' });
  const body = 'x'.repeat(16000);
  const sent = f.cli('send_message', { to: recipient, body }, sender.id);
  const content = f.hook('PostToolUse').hookSpecificOutput.additionalContext;
  assert.ok(content.length < 10000);
  assert.ok(content.includes('bodyOmitted'));
  assert.ok(content.includes('entity get message ID'));
  assert.equal(db.prepare('SELECT body FROM messages WHERE id=?').get(sent.id).body, body);
  assert.equal(db.prepare('SELECT acknowledgedAt FROM deliveries').get().acknowledgedAt, null);
});
test('oversized batches never strand staged rows', t => {
  const f = fixture(t, 'grok'); f.hook('SessionStart');
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const recipient = db.prepare('SELECT id FROM sessions').get().id;
  const sender = f.cli('join', { name: 'sender', vendor: 'raw' });
  for (let i = 0; i < 16; i++) f.cli('send_message', { to: recipient, body: `short ${i}`, reasoning: `${i} ${'r'.repeat(500)}`.slice(0, 500) }, sender.id);
  const content = f.hook('PostToolUse').hookSpecificOutput.additionalContext;
  assert.ok(content.length <= 9000);
  assert.equal(content.includes('r'.repeat(100)), false, 'References omit repeated reasoning');
  assert.equal(db.prepare("SELECT count(*) n FROM dispatches WHERE state='staged'").get().n, 0);
  assert.equal(db.prepare("SELECT count(*) n FROM dispatches WHERE state='submitted'").get().n, 16);
});
test('concurrent host hooks register one identity and offer one delivery', async t => {
  const f = fixture(t, 'cursor');
  const hook = () => new Promise((resolve, reject) => {
    const child = spawn(binary, f.args); let out='', err='';
    child.stdout.on('data', b => out += b); child.stderr.on('data', b => err += b);
    child.on('error', reject); child.on('close', code => code ? reject(Error(err)) : resolve(JSON.parse(out)));
    child.stdin.end(JSON.stringify(f.input('postToolUse')));
  });
  await Promise.all(Array.from({ length: 8 }, hook));
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.equal(db.prepare('SELECT count(*) n FROM sessions').get().n, 1);
  const recipient = db.prepare('SELECT id FROM sessions').get().id;
  const sender = f.cli('join', { name: 'sender', vendor: 'raw' });
  f.cli('send_message', { to: recipient, body: 'CONCURRENT' }, sender.id);
  const results = await Promise.all(Array.from({ length: 8 }, hook));
  assert.equal(results.filter(r => r.additional_context?.includes('CONCURRENT')).length, 1);
});
test('wrong workspace, ignored events and foreign compatibility hooks consume nothing', t => {
  const f = fixture(t, 'cursor');
  assert.deepEqual(f.hook('sessionStart', { workspace_roots: ['/'] }), {});
  assert.deepEqual(f.hook('stop'), {});
  assert.deepEqual(f.hook('postToolUse', { conversation_id: undefined, sessionId: 'grok-foreign' }), {});
  assert.deepEqual(f.hook('sessionEnd'), {});
  assert.equal(existsSync(f.database), false);
});
test('native config previews have timeouts, supported events and no writes', t => {
  for (const vendor of ['cursor','grok']) {
    const f = fixture(t,vendor);
    const result = JSON.parse(execFileSync(binary,['host-config','--vendor',vendor,...f.flags],{encoding:'utf8'}));
    const event = vendor === 'cursor' ? 'postToolUse' : 'PostToolUse';
    const handler = vendor === 'cursor' ? result.hooks[event][0] : result.hooks[event][0].hooks[0];
    assert.equal(handler.timeout,10);
    assert.ok(handler.command.includes('host-hook'));
    assert.equal(result.hooks.stop,undefined);
    assert.equal(existsSync(f.database),false);
  }
});

test('host identity is emitted once per explicit context generation', t => {
 const f=fixture(t,'cursor');
 const identity=f.hook('sessionStart').additional_context;
 assert.ok(identity.includes('Communication identity:'));
 assert.ok(identity.includes('--workspace . --database audit.sqlite'),identity);
 assert.ok(identity.length<300,'Identity context stays short and relative');
 assert.deepEqual(f.hook('sessionStart'),{});
 assert.deepEqual(f.hook('postToolUse'),{});
 assert.ok(f.hook('postToolUse',{context_generation:'after-compaction-1'}).additional_context.includes('Communication identity:'));
 assert.deepEqual(f.hook('postToolUse',{context_generation:'after-compaction-1'}),{});
});

for (const vendor of ['cursor', 'grok']) {
  test(`${vendor}: routine hooks are read-only and do not wait for an unrelated WAL writer`, async t => {
    const f = fixture(t, vendor);
    f.hook('sessionStart'); f.hook('postToolUse');
    const db = new DatabaseSync(f.database); t.after(() => db.close());
    const before = db.prepare('PRAGMA data_version').get().data_version;
    assert.deepEqual(f.hook('postToolUse'), {});
    assert.deepEqual(f.hook('postToolUseFailure'), {});
    assert.deepEqual(f.hook(vendor === 'cursor' ? 'beforeSubmitPrompt' : 'UserPromptSubmit'), vendor === 'cursor' ? {continue: true} : {});
    assert.equal(db.prepare('PRAGMA data_version').get().data_version, before, 'No committed write on an idle hook');
    db.exec('BEGIN IMMEDIATE');
    const child = spawn(binary, f.args);
    let output = '', stderr = '';
    child.stdout.on('data', value => output += value); child.stderr.on('data', value => stderr += value);
    const completed = new Promise((resolve, reject) => {
      child.once('error', reject); child.once('close', code => code ? reject(Error(stderr)) : resolve());
    });
    let timer;
    try {
      child.stdin.end(JSON.stringify(f.input('postToolUse')));
      await Promise.race([completed, new Promise((_, reject) => { timer = setTimeout(() => reject(Error('Idle hook waited for the writer')), 1500); })]);
      assert.deepEqual(JSON.parse(output), {}); assert.equal(stderr, '');
    } finally { clearTimeout(timer); db.exec('ROLLBACK'); await completed; }
  });
}

test('hook fast path renews near-expiry presence and resumes expired owners without old leases', t => {
  const f = fixture(t, 'cursor'); f.hook('sessionStart');
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const recipient = db.prepare('SELECT id FROM sessions').get().id;
  db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now() + 10000, recipient);
  assert.deepEqual(f.hook('postToolUse'), {});
  assert.ok(db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(recipient).expiresAt > Date.now() + 45000);
  f.cli('lock', {path: 'owned.txt'}, recipient);
  assert.equal(db.prepare('SELECT count(*) n FROM leases WHERE owner=?').get(recipient).n, 1);
  db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now() - 1, recipient);
  assert.deepEqual(f.hook('postToolUse'), {});
  assert.equal(db.prepare('SELECT count(*) n FROM leases WHERE owner=?').get(recipient).n, 0);
  assert.ok(db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(recipient).expiresAt > Date.now() + 45000);
});

async function nativeGrok(t) {
  const f = fixture(t, 'grok'), native = randomUUID(), endpoint = join(f.workspace, 'g.sock');
  const server = createServer();
  await new Promise(resolve => server.listen(endpoint, resolve));
  t.after(() => new Promise(resolve => server.close(resolve)));
  return {...f, native, endpoint};
}

test('Grok hook reuses a generically labeled native identity without competing for delivery', async t => {
  const f = await nativeGrok(t), {native, endpoint} = f;
  const recipient = f.cli('join', {name: 'receiver', vendor: 'other', vendorSession: native});
  f.cli('attach', {transport: 'grok', endpoint, vendorSession: native}, recipient.id);
  const sender = f.cli('join', {name: 'sender', vendor: 'raw'});
  f.cli('send_message', {to: recipient.id, body: 'NATIVE OWNER'}, sender.id);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const version = db.prepare('PRAGMA data_version').get().data_version;
  assert.deepEqual(f.hook('PostToolUse', {sessionId: native}), {});
  assert.equal(db.prepare('PRAGMA data_version').get().data_version, version, 'Native-bound hooks do not write or compete for pending mail');
  assert.equal(db.prepare('SELECT count(*) n FROM sessions WHERE vendorSession=?').get(native).n, 1);
  assert.equal(db.prepare('SELECT count(*) n FROM dispatches').get().n, 0);
  assert.equal(db.prepare('SELECT transport FROM attachments WHERE session=?').get(recipient.id).transport, 'grok');
  assert.equal(f.cli('notify_all', {body: 'ONE RECIPIENT'}, sender.id).recipients, 1);
});

test('native attach rejects a second identity for a host already registered by hooks', async t => {
  const f = await nativeGrok(t), {native, endpoint} = f;
  f.hook('SessionStart', {sessionId: native});
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const owner = db.prepare('SELECT id FROM sessions WHERE vendorSession=?').get(native).id;
  const sender = f.cli('join', {name: 'sender', vendor: 'raw'});
  f.cli('send_message', {to: owner, body: 'OFFERED BY HOOK'}, sender.id);
  assert.ok(f.hook('PostToolUse', {sessionId: native}).hookSpecificOutput.additionalContext.includes('OFFERED BY HOOK'));
  const duplicate = f.cli('join', {name: 'second', vendor: 'other'});
  assert.throws(() => f.cli('attach', {transport: 'grok', endpoint, vendorSession: native}, duplicate.id), /already registered/);
  assert.equal(db.prepare('SELECT vendorSession FROM sessions WHERE id=?').get(duplicate.id).vendorSession, null);
  assert.equal(db.prepare('SELECT count(*) n FROM attachments WHERE session=?').get(duplicate.id).n, 0);
  f.cli('attach', {transport: 'grok', endpoint, vendorSession: native}, owner);
  assert.equal(f.cli('dispatch', {}, owner).submitted, 0, 'Native transport must not replay a hook offer');
  assert.deepEqual(f.hook('PostToolUse', {sessionId: native}), {});
  assert.equal(db.prepare('SELECT count(*) n FROM sessions WHERE vendorSession=?').get(native).n, 1);
  assert.equal(db.prepare('SELECT count(*) n FROM dispatches').get().n, 1);
});

test('competing native attachments select one identity across different vendor labels', async t => {
  const f = await nativeGrok(t), {native, endpoint} = f;
  const ids = ['first-label', 'second-label'].map(vendor => f.cli('join', {name: vendor, vendor}).id);
  const results = await Promise.all(ids.map(id => new Promise((resolve, reject) => {
    const child = spawn(binary, ['attach', JSON.stringify({transport: 'grok', endpoint, vendorSession: native}), ...f.flags, '--session', id]);
    let stderr = '';
    child.stdout.resume(); child.stderr.on('data', data => stderr += data);
    child.on('error', reject); child.on('close', code => resolve({code, stderr}));
  })));
  assert.equal(results.filter(r => r.code === 0).length, 1);
  assert.match(results.find(r => r.code !== 0).stderr, /already registered/);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.equal(db.prepare('SELECT count(*) n FROM sessions WHERE vendorSession=?').get(native).n, 1);
  assert.equal(db.prepare("SELECT count(*) n FROM attachments WHERE transport='grok'").get().n, 1);
  assert.deepEqual(f.hook('PostToolUse', {sessionId: native}), {});
});
