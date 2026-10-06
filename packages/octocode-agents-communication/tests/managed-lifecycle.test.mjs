import {test} from 'node:test';
import assert from 'node:assert/strict';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {registerPiInbox} from '../scripts/pi-inbox.mjs';
import {binary, fastHeartbeatEnv, jsonCall, spawn, tempWorkspace} from './helpers.mjs';

function fixture(t) {
  const workspace = tempWorkspace(t, 'managed-leases-'), database = join(workspace, 'db.sqlite');
  const call = jsonCall(binary, workspace, database);
  const joinAgent = name => call('join', {name, vendor: 'test'}).id;
  const session = joinAgent('worker'), peer = joinAgent('peer');
  const db = new DatabaseSync(database);
  db.exec('PRAGMA busy_timeout=5000');
  t.after(() => db.close());
  const lock = (path, ttlMs = 60000, owner = session) => call('lock', {path, ttlMs, reasoning: 'Verify managed lifecycle'}, owner).lease;
  const lease = id => db.prepare('SELECT * FROM leases WHERE id=?').get(id);
  return {workspace, database, call, db, session, peer, lock, lease};
}

async function until(check, timeout = 18000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (check()) return;
    await new Promise(resolve => setTimeout(resolve, 30));
  }
  assert.fail('Managed lifecycle did not reach expected state');
}

test('managed heartbeat renews only live owned leases and preserves unmanaged semantics', t => {
  const f = fixture(t);
  const live = f.lock('live', 20000), long = f.lock('long', 600000), expired = f.lock('expired');
  const foreign = f.lock('foreign', 20000, f.peer);
  f.db.prepare('UPDATE leases SET expiresAt=0 WHERE id=?').run(expired.id);
  const beat = f.call('heartbeat', {}, f.session);
  assert.deepEqual({...beat, expiresAt: undefined}, {alive: true, expiresAt: undefined});
  assert.ok(beat.expiresAt > Date.now() + 50000 && beat.expiresAt <= Date.now() + 60000);
  assert.equal(f.lease(live.id).expiresAt, live.expiresAt);
  const before = Date.now();
  const { expiresAt, ...renewed } = f.call('heartbeat', {renewLeases: true}, f.session);
  assert.deepEqual(renewed, {alive: true, renewedLeases: 1});
  assert.ok(Number.isSafeInteger(expiresAt));
  assert.ok(f.lease(live.id).expiresAt >= before + 60000);
  assert.ok(f.lease(live.id).expiresAt <= Date.now() + 60000);
  assert.equal(f.lease(long.id).expiresAt, long.expiresAt, 'Never shorten an explicit longer lease');
  assert.equal(f.lease(expired.id).expiresAt, 0, 'Never revive an expired lease');
  assert.equal(f.lease(foreign.id).expiresAt, foreign.expiresAt, 'Never renew a peer lease');
  f.db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(f.session);
  const owned = f.lease(live.id).expiresAt;
  assert.throws(() => f.call('heartbeat', {renewLeases: true}, f.session), /expired session/);
  assert.equal(f.db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(f.session).expiresAt, 0);
  assert.equal(f.lease(live.id).expiresAt, owned);
});

test('managed MCP renews leases while idle, rejects expired identity, and releases on failure', async t => {
  const f = fixture(t);
  const child = spawn(binary, ['mcp', '--managed', '--session', f.session, '--vendor', 'test', '--tools', 'editing', '--workspace', f.workspace, '--database', f.database], {env: fastHeartbeatEnv()});
  t.after(() => { if (child.exitCode === null) child.kill('SIGKILL'); });
  let stderr = '', stdout = '';
  child.stderr.on('data', chunk => stderr += chunk);
  child.stdout.on('data', chunk => stdout += chunk);
  const closed = new Promise(resolve => child.once('close', resolve));
  await until(() => stderr.includes('mcp_ready'));
  const ready = JSON.parse(stderr.trim().split('\n')[0]);
  assert.equal(ready.leases, 'managed');
  child.stdin.write(JSON.stringify({jsonrpc: '2.0', id: 1, method: 'initialize', params: {}}) + '\n');
  await until(() => stdout.includes('\n'));
  assert.match(JSON.parse(stdout.trim()).result.instructions, /renews live owned leases/);
  const live = f.lock('live', 20000), expired = f.lock('expired', 1000);
  // Expire it explicitly: a 100 ms heartbeat would otherwise renew the still-live lease.
  f.db.prepare('UPDATE leases SET expiresAt=? WHERE id=?').run(Date.now() - 1, expired.id);
  const lapsed = f.lease(expired.id).expiresAt;
  await until(() => f.lease(live.id).expiresAt > live.expiresAt);
  assert.equal(f.lease(expired.id).expiresAt, lapsed, 'renewal never revives an expired lease');
  f.db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(f.session);
  const code = await closed;
  assert.equal(code, 1);
  assert.match(stderr, /expired session/);
  assert.equal(f.db.prepare('SELECT count(*) AS n FROM leases WHERE owner=?').get(f.session).n, 0);
  assert.ok(f.db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(f.session).expiresAt <= Date.now());
});

test('Pi renews leases during active work and shuts down after lifecycle failure without reviving identity', async t => {
  const f = fixture(t), handlers = new Map(), ledger = [];
  const pi = {on: (name, handler) => handlers.set(name, handler), registerTool() {},
    sendMessage(message) { ledger.push({type: 'custom_message', ...message}); }};
  const ctx = {cwd: f.workspace, sessionManager: {getSessionId: () => 'pi-managed-test', getEntries: () => ledger}};
  const prior = process.env.OCTOCODE_COMMUNICATION_HEARTBEAT_MS;
  process.env.OCTOCODE_COMMUNICATION_HEARTBEAT_MS = '100';
  t.after(() => { if (prior === undefined) delete process.env.OCTOCODE_COMMUNICATION_HEARTBEAT_MS; else process.env.OCTOCODE_COMMUNICATION_HEARTBEAT_MS = prior; });
  const controller = registerPiInbox(pi, {binary, database: f.database, tools: 'editing'});
  const fire = name => handlers.get(name)?.({}, ctx);
  try {
  await fire('session_start');
  const session = controller.getBinding().session;
  assert.ok(ledger.some(entry => entry.content?.includes('renews live owned leases')));
  await fire('agent_start');
  const live = f.lock('pi-active', 20000, session), expired = f.lock('pi-expired', 20000, session);
  f.db.prepare('UPDATE leases SET expiresAt=0 WHERE id=?').run(expired.id);
  await until(() => f.lease(live.id).expiresAt > live.expiresAt);
  assert.equal(f.lease(expired.id).expiresAt, 0);
  f.db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(session);
  await until(() => controller.getBinding() === null);
  await until(() => f.db.prepare('SELECT count(*) AS n FROM leases WHERE owner=?').get(session).n === 0);
  assert.ok(f.db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(session).expiresAt <= Date.now());
  } finally { await fire('session_shutdown'); }
});

test('managed hook renews idle leases without resuming identity; Pi shutdown releases leases', async t => {
  const f = fixture(t), handlers = new Map(), ledger = [];
  const pi = {on: (name, handler) => handlers.set(name, handler), registerTool() {},
    sendMessage(message) { ledger.push({type: 'custom_message', ...message}); }};
  const ctx = {cwd: f.workspace, sessionManager: {getSessionId: () => 'pi-idle-test', getEntries: () => ledger}};
  const controller = registerPiInbox(pi, {binary, database: f.database});
  const fire = name => handlers.get(name)?.({}, ctx);
  try {
  await fire('session_start');
  const session = controller.getBinding().session, live = f.lock('pi-idle', 20000, session);
  f.db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now() + 30000, session);
  await controller.drain();
  assert.ok(f.lease(live.id).expiresAt > live.expiresAt);
  await fire('session_shutdown');
  assert.equal(f.lease(live.id), undefined);
  assert.throws(() => f.call('hook', {format: 'json', managed: true}, session), /expired session/);
  assert.equal(controller.getBinding(), null);
  } finally { await fire('session_shutdown'); }
});
