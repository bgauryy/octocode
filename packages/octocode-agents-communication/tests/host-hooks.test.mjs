import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from './helpers.mjs';
import { existsSync, mkdirSync, realpathSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { createServer } from 'node:net';
import { randomUUID } from 'node:crypto';
import { binary, tempWorkspace, withReasoning } from './helpers.mjs';

function fixture(t, vendor, paths) {
  const workspace = paths?.workspace ?? tempWorkspace(t, 'communication-hook-');
  const database = paths?.database ?? join(workspace, 'audit.sqlite');
  const flags = ['--workspace', workspace, '--database', database];
  const cli = (command, input = {}, session) => JSON.parse(execFileSync(binary,
    [command, JSON.stringify(withReasoning(command,input)), ...flags, ...(session ? ['--session', session] : [])], { encoding: 'utf8' }));
  const input = event => vendor === 'cursor'
    ? { hook_event_name: event, conversation_id: 'host-fixture', workspace_roots: [workspace] }
    : vendor === 'grok'
      ? { hook_event_name: event, hookEventName: event, sessionId: 'host-fixture', workspaceRoot: workspace }
      : { hook_event_name: event, session_id: 'host-fixture', cwd: workspace };
  const args = ['host-hook', '--vendor', vendor, ...flags];
  const hook = (event, override = {}) => JSON.parse(execFileSync(binary, args,
    { input: JSON.stringify({ ...input(event), ...override }), encoding: 'utf8', stdio: ['pipe','pipe','pipe'] }));
  return { workspace, database, flags, cli, input, args, hook };
}
for (const vendor of ['cursor', 'grok', 'claude', 'codex']) {
  test(`${vendor}: identity reuse, event-specific context, one-time delivery and teardown`, t => {
    const f = fixture(t, vendor);
    const initial = f.hook('sessionStart');
    const db = new DatabaseSync(f.database); t.after(() => db.close());
    const recipient = db.prepare('SELECT * FROM sessions').get();
    assert.equal(recipient.vendor, vendor);
    f.hook('sessionStart');
    assert.equal(db.prepare('SELECT count(*) n FROM sessions').get().n, 1);
    if (vendor === 'grok') assert.deepEqual(initial, {});
    else assert.ok((initial.additional_context ?? initial.hookSpecificOutput.additionalContext).includes(recipient.id));
    const sender = f.cli('join', { vendor: 'raw', name: 'sender' });
    const message = f.cli('send_message', { to: recipient.id, body: 'ONE DELIVERY', replyRequired:false }, sender.id);
    const ignored = f.hook(vendor === 'cursor' ? 'beforeSubmitPrompt' : 'UserPromptSubmit');
    assert.deepEqual(ignored, vendor === 'cursor' ? { continue: true } : {});
    assert.equal(db.prepare('SELECT count(*) n FROM dispatches').get().n, 0);
    const delivered = f.hook('postToolUse');
    const content = vendor === 'cursor' ? delivered.additional_context : delivered.hookSpecificOutput.additionalContext;
    assert.ok(content.includes('ONE DELIVERY'));
    assert.equal(content.includes('Communication identity:'), vendor === 'grok');
    assert.deepEqual(f.hook('postToolUseFailure'), {});
    assert.equal(f.cli('inbox', {}, recipient.id).items[0].id, message.id);
    f.cli('complete', { message: message.id }, recipient.id);
    f.hook('sessionEnd');
    assert.ok(db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(recipient.id).expiresAt <= Date.now());
    f.hook('postToolUse');
    assert.equal(db.prepare('SELECT count(*) n FROM sessions WHERE vendor=?').get(vendor).n, 1);
  });
}
for (const vendor of ['grok', 'codex']) test(`${vendor} context cap uses explicit references without clipping stored bodies`, t => {
  const f = fixture(t, vendor); f.hook('SessionStart');
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const recipient = db.prepare('SELECT id FROM sessions').get().id;
  const sender = f.cli('join', { name: 'sender', vendor: 'raw' });
  const body = 'x'.repeat(vendor === 'codex' ? 6500 : 16000);
  const sent = f.cli('send_message', { to: recipient, body }, sender.id);
  const content = f.hook('PostToolUse').hookSpecificOutput.additionalContext;
  assert.ok(Buffer.byteLength(content) <= (vendor === 'codex' ? 5000 : 9000));
  assert.ok(content.includes('bodyOmitted'));
  assert.ok(content.includes('fetch {type:"message",where:{messageId:ID}}'));
  const references = JSON.parse(content.slice(content.indexOf('New message references: ') + 'New message references: '.length));
  assert.equal(references.length, 1);
  assert.equal(references[0].path, realpathSync(f.workspace));
  assert.equal(references[0].from, sender.id); assert.equal(references[0].to, recipient);
  assert.equal(references[0].type, 'message'); assert.equal(typeof references[0].timestamp, 'number');
  assert.deepEqual(references[0].data, { messageId: sent.id, bodyOmitted: true,
    next: { command:'fetch', input:{recordId:references[0].recordId} } });
  const fetched = f.cli(references[0].data.next.command, references[0].data.next.input, recipient).items[0];
  assert.equal(fetched.recordId, references[0].recordId); assert.equal(fetched.data.body, body);
  assert.equal(db.prepare('SELECT body FROM messages WHERE id=?').get(sent.id).body, body);
  assert.equal(db.prepare('SELECT acknowledgedAt FROM deliveries').get().acknowledgedAt, null);
});
for (const vendor of ['grok', 'codex']) test(`${vendor}: oversized batches offer every row once without stranding staged rows`, t => {
  const f = fixture(t, vendor); f.hook('SessionStart');
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const recipient = db.prepare('SELECT id FROM sessions').get().id;
  const sender = f.cli('join', { name: 'sender', vendor: 'raw' });
  const messages = Array.from({length:16}, (_, i) => f.cli('send_message', { to: recipient, body: `short ${i}`, reasoning: `${i} ${'r'.repeat(500)}`.slice(0, 500) }, sender.id).id);
  const offered = [];
  for (let attempt = 0; attempt < 16 && offered.length < messages.length; attempt++) {
    const content = f.hook('PostToolUse').hookSpecificOutput.additionalContext;
    assert.ok(Buffer.byteLength(content) <= (vendor === 'codex' ? 5000 : 9000));
    if (content.includes('bodyOmitted'))
      assert.equal(content.includes('r'.repeat(100)), false, 'References omit repeated reasoning');
    offered.push(...[...content.matchAll(/"messageId":(\d+)/g)].map(match => Number(match[1])));
    assert.equal(db.prepare("SELECT count(*) n FROM dispatches WHERE state='staged'").get().n, 0);
  }
  assert.deepEqual(offered.sort((a,b)=>a-b), messages);
  assert.equal(db.prepare("SELECT count(*) n FROM dispatches WHERE state='submitted'").get().n, 16);
});
test('Codex offers deferred mail after a large first binding without changing peer state', t => {
  const root = tempWorkspace(t, 'communication-hook-pressure-', {real:true});
  const workspace = join(root, ...Array(3).fill('w'.repeat(230)));
  const directory = join(root, ...Array(3).fill('d'.repeat(230)));
  mkdirSync(workspace, {recursive:true}); mkdirSync(directory, {recursive:true});
  const f = fixture(t, 'codex', {workspace, database:join(directory, 'audit.sqlite')});
  const recipient = f.cli('join', {vendor:'codex', name:'codex', vendorSession:'host-fixture'}).id;
  // Every peer row has the same size, so the budgeted directory is large whatever the random ID order.
  const sender = f.cli('join', {vendor:'raw', name:'s'+'p'.repeat(60), task:'t'.repeat(250)}).id;
  for (let i=0;i<10;i++) f.cli('join', {vendor:'raw', name:String(i)+'p'.repeat(60), task:'t'.repeat(250)});
  const sent = f.cli('send_message', {to:recipient,body:'x'.repeat(6500)},sender);
  const first = f.hook('PostToolUse').hookSpecificOutput.additionalContext;
  assert.ok(Buffer.byteLength(first)<=5000); assert.equal(first.includes('bodyOmitted'),false);
  assert.equal(f.cli('fetch',{type:'dispatch.staged',current:true},recipient).items.length,0);
  const second = f.hook('PostToolUse').hookSpecificOutput.additionalContext;
  assert.ok(Buffer.byteLength(second)<=5000); assert.ok(second.includes(`"messageId":${sent.id}`));
  assert.equal(f.cli('fetch',{type:'dispatch.submitted',current:true},recipient).items[0].data.messageId,sent.id);
  assert.deepEqual(f.hook('PostToolUse'),{});
  assert.equal(f.cli('inbox',{},recipient).items[0].id,sent.id);
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
  for (const vendor of ['cursor','grok','claude','codex']) {
    const f = fixture(t,vendor);
    const result = JSON.parse(execFileSync(binary,['host-config','--vendor',vendor,...f.flags],{encoding:'utf8'}));
    const event = vendor === 'cursor' ? 'postToolUse' : 'PostToolUse';
    const handler = vendor === 'cursor' ? result.hooks[event][0] : result.hooks[event][0].hooks[0];
    assert.equal(handler.timeout,10);
    if (vendor === 'codex') {
      assert.equal(handler.additionalContextLimit, 5000);
      assert.equal(result.hooks.PostToolUseFailure, undefined);
    }
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

for (const vendor of ['cursor', 'grok', 'claude', 'codex']) {
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

for (const vendor of ['claude', 'codex']) {
  test(`${vendor}: child hooks cannot consume or retire the parent's identity`, t => {
    const f = fixture(t, vendor);
    f.hook('SessionStart');
    const db = new DatabaseSync(f.database); t.after(() => db.close());
    const recipient = db.prepare('SELECT id FROM sessions').get().id;
    const sender = f.cli('join', { name: 'sender', vendor: 'raw' });
    const sent = f.cli('send_message', { to: recipient, body: 'PARENT MAIL' }, sender.id);
    const expiresAt = db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(recipient).expiresAt;
    for (const event of ['SessionStart', 'PostToolUse', 'SessionEnd'])
      assert.deepEqual(f.hook(event, { agent_id: 'child', agent_type: 'reviewer' }), {});
    assert.equal(db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(recipient).expiresAt, expiresAt);
    assert.equal(db.prepare('SELECT count(*) n FROM dispatches').get().n, 0);
    assert.ok(f.hook('PostToolUse').hookSpecificOutput.additionalContext.includes('PARENT MAIL'));
    assert.equal(f.cli('inbox', {}, recipient).items[0].id, sent.id);
  });

  test(`${vendor}: attached native receiver receives lifecycle updates without duplicate hook delivery`, t => {
    const f = fixture(t, vendor);
    f.hook('SessionStart');
    const db = new DatabaseSync(f.database); t.after(() => db.close());
    const recipient = db.prepare('SELECT id FROM sessions').get().id;
    const sender = f.cli('join', { name: 'sender', vendor: 'raw' });
    f.cli('attach', { transport: vendor, endpoint: vendor === 'claude' ? join(f.workspace, 'native.sock') : 'ws://127.0.0.1:4321', vendorSession: 'host-fixture' }, recipient);
    f.cli('send_message', { to: recipient, body: 'NATIVE ONLY' }, sender.id);
    assert.deepEqual(f.hook('PostToolUse'), {});
    assert.equal(db.prepare('SELECT count(*) n FROM dispatches').get().n, 0);
    assert.equal(db.prepare('SELECT count(*) n FROM sessions WHERE vendor=?').get(vendor).n, 1);
  });
}

test('Grok native camelCase payload with compatibility event and slash-suffixed workspace delivers once', t => {
  const f = fixture(t, 'grok');
  const envelope = { hookEventName: 'session_start', sessionId: 'host-fixture', session_id: 'host-fixture', cwd: f.workspace, workspaceRoot: f.workspace + '/' };
  assert.deepEqual(f.hook('SessionStart', envelope), {});
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const recipient = db.prepare('SELECT id FROM sessions WHERE vendor=?').get('grok').id;
  const sender = f.cli('join', { name: 'sender', vendor: 'raw' });
  f.cli('send_message', { to: recipient, body: 'GROK ENVELOPE', replyRequired: false }, sender.id);
  const event = { ...envelope, hookEventName: 'post_tool_use' };
  assert.ok(f.hook('PostToolUse', event).hookSpecificOutput.additionalContext.includes('GROK ENVELOPE'));
  assert.deepEqual(f.hook('PostToolUse', event), {});
});
