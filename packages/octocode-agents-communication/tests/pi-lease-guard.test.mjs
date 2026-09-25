import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdtempSync, rmSync, writeFileSync, existsSync, mkdirSync, symlinkSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {DatabaseSync} from 'node:sqlite';
import {registerPiInbox} from '../skills/octocode-agents-communication/scripts/pi-inbox.mjs';

const binary = process.env.COMMUNICATION_BINARY || fileURLToPath(new URL('../skills/octocode-agents-communication/scripts/agents-communication', import.meta.url));
function fixture(t, requireLeases) {
  const workspace = mkdtempSync(join(tmpdir(), 'pi-lease-guard-')), database = join(workspace, 'db.sqlite'), sessionFile = join(workspace, 'session.jsonl');
  const handlers = new Map(), ledger = [];
  const pi = {on: (name, fn) => handlers.set(name, fn), registerTool() {}, sendMessage(message) {
    ledger.push({type: 'custom_message', ...message});
    writeFileSync(sessionFile, [JSON.stringify({type: 'session', id: 'pi-guard'}), ...ledger.map(row => JSON.stringify(row))].join('\n') + '\n');
  }};
  const ctx = {cwd: workspace, sessionManager: {getSessionId: () => 'pi-guard', getSessionFile: () => sessionFile, getEntries: () => ledger}};
  const fire = (name, event = {}, context = ctx) => handlers.get(name)?.(event, context);
  const controller = registerPiInbox(pi, {binary, database, requireLeases});
  const call = (command, input = {}, session = controller.getBinding()?.session) => JSON.parse(execFileSync(binary, [command, JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])], {encoding: 'utf8', stdio: 'pipe'}));
  t.after(async () => { try { await fire('session_shutdown'); } finally { rmSync(workspace, {recursive: true, force: true}); } });
  return {workspace, database, handlers, ctx, fire, controller, call};
}
test('opt-in Pi guard blocks an unleased structured write before its side effect', async t => {
  const f = fixture(t, true); await f.fire('session_start');
  const attempt = async path => {
    const decision = await f.fire('tool_call', {toolName: 'write', input: {path, content: 'fixture'}});
    if (!decision?.block) writeFileSync(join(f.ctx.cwd, path), 'fixture');
    return decision;
  };
  assert.equal((await attempt('unleased.txt')).block, true); assert.equal(existsSync(join(f.workspace, 'unleased.txt')), false);
  f.call('lock', {path: 'leased.txt', reasoning: 'Authorize this structured write'});
  assert.equal(await attempt('leased.txt'), undefined); assert.equal(existsSync(join(f.workspace, 'leased.txt')), true);
  const lease = f.call('lock', {path: 'edit.txt', reasoning: 'Authorize this structured edit'}).lease;
  assert.equal(await f.fire('tool_call', {toolName: 'edit', input: {path: 'edit.txt', oldText: '', newText: 'x'}}), undefined);
  f.call('unlock', {lease: lease.id});
  assert.equal((await f.fire('tool_call', {toolName: 'edit', input: {path: 'edit.txt'}})).block, true);
});
test('Pi guard resolves file paths from the event cwd and denies stale or invalid bindings', async t => {
  const f = fixture(t, true); await f.fire('session_start');
  mkdirSync(join(f.workspace, 'subdir')); f.ctx.cwd = join(f.workspace, 'subdir');
  f.call('lock', {path: 'subdir/owned.txt', reasoning: 'Own the file in the event working directory'});
  assert.equal(await f.fire('tool_call', {toolName: 'write', input: {path: 'owned.txt'}}), undefined);
  assert.equal((await f.fire('tool_call', {toolName: 'write', input: {}})).block, true);
  const stale = {...f.ctx, sessionManager: {...f.ctx.sessionManager, getSessionId: () => 'different'}};
  assert.equal((await f.fire('tool_call', {toolName: 'write', input: {path: 'owned.txt'}}, stale)).block, true);
  f.call('leave');
  assert.equal((await f.fire('tool_call', {toolName: 'write', input: {path: 'owned.txt'}})).block, true);
});
test('Pi admission guard is opt-in and does not pretend to fence shell or arbitrary tools', async t => {
  const disabled = fixture(t, false); assert.equal(disabled.handlers.has('tool_call'), false);
  const f = fixture(t, true); await f.fire('session_start');
  for (const toolName of ['read', 'bash', 'powershell', 'custom_writer']) assert.equal(await f.fire('tool_call', {toolName, input: {path: 'not-leased'}}), undefined);
  await f.fire('session_shutdown');
  assert.equal((await f.fire('tool_call', {toolName: 'write', input: {path: 'not-leased'}})).block, true);
});

test('Pi guard rejects a DB native binding changed after host attachment', async t => {
  const f = fixture(t, true); await f.fire('session_start');
  f.call('lock', {path: 'owned.txt', reasoning: 'Verify native binding in the lease snapshot'});
  const db = new DatabaseSync(f.database);
  try { db.prepare('UPDATE sessions SET vendorSession=? WHERE id=?').run('different-native', f.controller.getBinding().session); }
  finally { db.close(); }
  assert.equal((await f.fire('tool_call', {toolName: 'write', input: {path: 'owned.txt'}})).block, true);
});

test('Pi guard rejects aliases and symlink-parent traversal instead of authorizing a different target', async t => {
  const f = fixture(t, true); await f.fire('session_start');
  mkdirSync(join(f.workspace, 'sub/child'), {recursive: true});
  symlinkSync(join(f.workspace, 'sub/child'), join(f.workspace, 'link'));
  f.call('lock', {path: 'sub/owned.txt', reasoning: 'Physical target must not authorize the host lexical target'});
  for (const path of ['link/../owned.txt', '@sub/owned.txt', '~/owned.txt', 'file:///owned.txt', 'sub/space\u00a0name.txt']) {
    assert.equal((await f.fire('tool_call', {toolName: 'write', input: {path}})).block, true);
  }
});
