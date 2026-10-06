import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, execFile, execFileSync, launcher, tempWorkspace, withReasoning } from './helpers.mjs';
import { promisify } from 'node:util';

function gitRepo(t) {
  const directory = tempWorkspace(t, 'communication-hardening-', { real: true });
  const repo = join(directory, 'repo');
  mkdirSync(join(repo, 'pkg'), { recursive: true });
  execFileSync('git', ['-c', 'init.defaultBranch=main', 'init', '--quiet', repo], { stdio: 'pipe' });
  const database = join(directory, 'shared.sqlite');
  const call = (workspace, command, input = {}, session) => JSON.parse(execFileSync(binary,
    [command, JSON.stringify(withReasoning(command, input)), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])],
    { encoding: 'utf8', stdio: 'pipe' }));
  const fails = (workspace, command, input, session) => {
    try { call(workspace, command, input, session); } catch (error) { return String(error.stderr); }
    assert.fail(`${command} unexpectedly succeeded`);
  };
  return { directory, repo, database, call, fails };
}

for (const vendor of ['claude', 'codex']) {
  test(`${vendor}: host hooks register and deliver inside a Git repository`, t => {
    const f = gitRepo(t);
    const hook = event => JSON.parse(execFileSync(binary, ['host-hook', '--vendor', vendor, '--workspace', f.repo, '--database', f.database],
      { input: JSON.stringify({ hook_event_name: event, session_id: 'git-host', cwd: f.repo }), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
    const start = hook('SessionStart');
    const db = new DatabaseSync(f.database); t.after(() => db.close());
    const recipient = db.prepare('SELECT id FROM sessions').get().id;
    assert.ok(start.hookSpecificOutput.additionalContext.includes(recipient));
    assert.equal(db.prepare('SELECT count(*) n FROM workspaces').get().n, 1);
    const sender = f.call(f.repo, 'join', { name: 'sender', vendor: 'generic' }).id;
    f.call(f.repo, 'send_message', { to: recipient, body: 'GIT DELIVERY', replyRequired: false }, sender);
    assert.match(JSON.stringify(hook('PostToolUse')), /GIT DELIVERY/);
  });
}

test('nested workspaces of one checkout contend for the same absolute lease keys', t => {
  const f = gitRepo(t);
  writeFileSync(join(f.repo, 'pkg', 'index.ts'), '');
  const root = f.call(f.repo, 'join', { name: 'root', vendor: 'generic' }).id;
  const nested = join(f.repo, 'pkg');
  const child = f.call(nested, 'join', { name: 'child', vendor: 'generic' }).id;
  assert.equal(f.call(f.repo, 'lock', { path: 'pkg/index.ts' }, root).ok, true);
  const conflict = f.call(nested, 'lock', { path: 'index.ts' }, child);
  assert.equal(conflict.ok, false);
  assert.equal(conflict.owner.id, root);
  assert.equal(f.call(nested, 'check_paths', { paths: [{ path: 'index.ts' }] }, child).ok, false);
  assert.equal(f.call(nested, 'locks', { path: 'index.ts' }, child).items.length, 1);
});

test('directory paths default to tree conflicts; file contexts default to file scope', t => {
  const f = gitRepo(t);
  writeFileSync(join(f.repo, 'pkg', 'api.ts'), '');
  const a = f.call(f.repo, 'join', { name: 'a', vendor: 'generic' }).id;
  const b = f.call(f.repo, 'join', { name: 'b', vendor: 'generic' }).id;
  f.call(f.repo, 'lock', { path: 'pkg/api.ts' }, a);
  assert.equal(f.call(f.repo, 'locks', { presence: 'all', path: 'pkg' }, b).items.length, 1);
  assert.equal(f.call(f.repo, 'lock', { path: 'pkg' }, b).ok, false);
  f.call(f.repo, 'share_document', { name: 'note.md', content: 'x', context: { path: 'pkg/api.ts', summary: 'API gotcha' } }, a);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const stored = JSON.parse(db.prepare("SELECT data FROM records WHERE type='document'").get().data);
  assert.equal(stored.context.kind, 'file');
});

test('wrong message IDs and whole-number floats fail with actionable errors', t => {
  const f = gitRepo(t);
  const a = f.call(f.repo, 'join', { name: 'a', vendor: 'generic' }).id;
  assert.match(f.fails(f.repo, 'complete', { message: 999, reply: 'done' }, a), /Unknown message 999.*data\.messageId/);
  // JSON.stringify writes 5.0 as 5, so send the raw JSON text a Python client would.
  const raw = (command, json) => {
    try { execFileSync(binary, [command, json, '--workspace', f.repo, '--database', f.database, '--session', a], { encoding: 'utf8', stdio: 'pipe' }); }
    catch (error) { return String(error.stderr); }
    assert.fail(`${command} unexpectedly succeeded`);
  };
  assert.match(raw('fetch', '{"limit":5.0}'), /Invalid input: \/limit has invalid type/);
  assert.match(raw('read_document', '{"name":"x.md","offset":5.0}'), /Invalid input: \/offset has invalid type/);
});

test('POSIX launcher and inbox hook resolve npm-style symlinks', { skip: process.platform === 'win32' }, t => {
  const directory = tempWorkspace(t, 'communication-bin-', { real: true });
  mkdirSync(join(directory, 'bin')); mkdirSync(join(directory, 'nested'));
  symlinkSync(launcher, join(directory, 'bin', 'agents-communication'));
  symlinkSync('../bin/agents-communication', join(directory, 'nested', 'chained'));
  symlinkSync(join(launcher, '..', 'inbox-hook'), join(directory, 'bin', 'inbox-hook'));
  for (const path of [join(directory, 'bin', 'agents-communication'), join(directory, 'nested', 'chained')]) {
    assert.ok(JSON.parse(execFileSync('/bin/sh', [path, '--help'], { encoding: 'utf8' })).commands.includes('join'));
  }
  assert.equal(JSON.parse(execFileSync('/bin/sh', [join(directory, 'bin', 'inbox-hook'), '--help'], { encoding: 'utf8' })).name, 'hook');
});

test('raw CLI heartbeat ttlMs keeps presence and live leases across a long turn', t => {
  const f = gitRepo(t);
  const a = f.call(f.repo, 'join', { name: 'raw', vendor: 'generic' }).id;
  const b = f.call(f.repo, 'join', { name: 'peer', vendor: 'generic' }).id;
  f.call(f.repo, 'lock', { path: 'pkg' }, a);
  const beat = f.call(f.repo, 'heartbeat', { ttlMs: 600000, renewLeases: true }, a);
  assert.equal(beat.renewedLeases, 1);
  assert.ok(beat.expiresAt > Date.now() + 590000);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.ok(db.prepare('SELECT expiresAt FROM leases WHERE owner=?').get(a).expiresAt >= beat.expiresAt);
  // Two minutes later a default 60s presence would be gone; this one still holds the tree.
  db.prepare('UPDATE sessions SET expiresAt=expiresAt-120000 WHERE id=?').run(a);
  db.prepare('UPDATE leases SET expiresAt=expiresAt-120000 WHERE owner=?').run(a);
  assert.equal(f.call(f.repo, 'lock', { path: 'pkg' }, b).ok, false);
  assert.match(f.fails(f.repo, 'heartbeat', { ttlMs: 600001 }, a), /Invalid/);
});

test('Claude dispatch predicts held mail from hook-reported permission mode and lets the hook deliver it', { skip: process.platform === 'win32' }, async t => {
  const f = gitRepo(t);
  const socketDir = tempWorkspace(t, 'cs-', { real: true }), path = join(socketDir, 'c.sock'), frames = [];
  const server = createServer(stream => { let body = ''; stream.on('data', chunk => body += chunk); stream.on('end', () => { frames.push(body); stream.end(); }); });
  await new Promise(resolve => server.listen(path, resolve)); t.after(() => server.close());
  const receiver = f.call(f.repo, 'join', { name: 'claude-receiver', vendor: 'claude', vendorSession: 'claude-host' }).id;
  const sender = f.call(f.repo, 'join', { name: 'sender', vendor: 'generic' }).id;
  const hook = (event, permission_mode) => JSON.parse(execFileSync(binary, ['host-hook', '--vendor', 'claude', '--workspace', f.repo, '--database', f.database],
    { input: JSON.stringify({ hook_event_name: event, session_id: 'claude-host', cwd: f.repo, permission_mode }), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
  const dispatch = async () => JSON.parse((await promisify(execFile)(binary, ['dispatch', '{}', '--workspace', f.repo, '--database', f.database, '--session', receiver], { encoding: 'utf8' })).stdout);
  assert.equal(f.call(f.repo, 'attach', { transport: 'claude', endpoint: path }, receiver).inbound.outcome, 'unknown');
  hook('PostToolUse', 'bypassPermissions');
  assert.equal(f.call(f.repo, 'attach', { transport: 'claude', endpoint: path }, receiver).inbound.outcome, 'held');
  f.call(f.repo, 'send_message', { to: receiver, body: 'HELD QUESTION', wake: 'action' }, sender);
  const deferred = await dispatch();
  assert.equal(deferred.submitted, 0);
  assert.equal(deferred.deferred, 'claude-inbound-held');
  assert.match(deferred.inbound.reason, /bypasses permission prompts/);
  await new Promise(resolve => setTimeout(resolve, 50));
  assert.equal(frames.length, 0, 'nothing written into a hold queue');
  assert.match(JSON.stringify(hook('PostToolUse', 'bypassPermissions')), /HELD QUESTION/, 'receiver hook delivers instead');
  hook('PostToolUse', 'default');
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.deepEqual(db.prepare("SELECT data FROM records WHERE type='host.inbound' ORDER BY id").all().map(r => JSON.parse(r.data).permissionMode), ['bypassPermissions', 'default']);
  f.call(f.repo, 'send_message', { to: receiver, body: 'DELIVERED QUESTION', wake: 'action' }, sender);
  const delivered = await dispatch();
  assert.equal(delivered.submitted, 1);
  assert.equal(delivered.inbound.outcome, 'delivered');
  await new Promise(resolve => setTimeout(resolve, 50));
  assert.equal(frames.length, 1);
  assert.match(frames[0], /DELIVERED QUESTION/);
  assert.deepEqual(hook('PostToolUse', 'default'), {}, 'a deliverable socket binding gets no second hook delivery');
});

test('live peer names are unique per repository and address messages and records', t => {
  const f = gitRepo(t);
  const first = f.call(f.repo, 'join', { name: 'api', vendor: 'claude' });
  const second = f.call(f.repo, 'join', { name: 'api', vendor: 'codex' });
  const nested = f.call(join(f.repo, 'pkg'), 'join', { name: 'api', vendor: 'generic' });
  assert.deepEqual([first.name, second.name, nested.name], ['api', 'api-2', 'api-3']);
  const sent = f.call(f.repo, 'send_message', { to: 'api-2', body: 'BY NAME', replyRequired: false }, first.id);
  assert.equal(f.call(f.repo, 'inbox', {}, second.id).items[0].id, sent.id);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.equal(db.prepare('SELECT target FROM messages WHERE id=?').get(sent.id).target, second.id, 'stored target stays the stable ID');
  assert.equal(f.call(f.repo, 'send_message', { to: 'api-2', body: 'BY NAME', replyRequired: false, key: 'k1' }, first.id).id,
    f.call(f.repo, 'send_message', { to: second.id, body: 'BY NAME', replyRequired: false, key: 'k1' }, first.id).id, 'name and ID retries share one key');
  assert.equal(f.call(f.repo, 'record', { type: 'event', data: { name: 'n' }, to: 'api-3' }, first.id).to, nested.id);
  assert.match(f.fails(f.repo, 'send_message', { to: 'nobody', body: 'x' }, first.id), /Unknown peer nobody/);
  assert.match(f.fails(f.repo, 'heartbeat', { name: 'api' }, second.id), /belongs to a live peer/);
  // An expired identity that resumes after its name was taken gets a free suffix.
  db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(first.id);
  const replacement = f.call(f.repo, 'join', { name: 'api', vendor: 'grok' });
  assert.equal(replacement.name, 'api');
  assert.equal(f.call(f.repo, 'resume', { vendor: 'claude' }, first.id).name, 'api-4');
  assert.equal(f.call(f.repo, 'send_message', { to: 'api', body: 'live one', replyRequired: false }, second.id).recipientOffline, undefined);
});

test('host hook registrations get distinct names per host session', t => {
  const f = gitRepo(t);
  const hook = session => execFileSync(binary, ['host-hook', '--vendor', 'claude', '--workspace', f.repo, '--database', f.database],
    { input: JSON.stringify({ hook_event_name: 'SessionStart', session_id: session, cwd: f.repo }), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
  hook('one'); hook('two');
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.deepEqual(db.prepare('SELECT name FROM sessions ORDER BY name').all().map(r => r.name), ['claude', 'claude-2']);
});
