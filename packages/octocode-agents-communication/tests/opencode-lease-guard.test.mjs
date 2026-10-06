import {test} from 'node:test';
import assert from 'node:assert/strict';
import {writeFileSync, existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {createOpenCodeLeaseGuard} from '../scripts/hooks/opencode-lease-guard.mjs';
import { launcherCommand as binary, tempWorkspace, jsonCall } from './helpers.mjs';
async function fixture(t) {
  const workspace = tempWorkspace(t, 'opencode-guard-', { real: true }), database = join(workspace, 'coord.sqlite');
  const call = jsonCall(binary, workspace, database);
  const native = 'ses_guard_fixture', session = call('join', {name: 'opencode-guard', vendor: 'opencode', vendorSession: native}).id;
  const options = {binary, workspace, database, sessions: {[native]: session}};
  const hook = (await createOpenCodeLeaseGuard(options)({directory: workspace}))['tool.execute.before'];
  const db = new DatabaseSync(database); t.after(() => db.close());
  const input = {tool: 'write', sessionID: native, callID: 'call_fixture'}, output = {args: {filePath: join(workspace, 'guarded.txt'), content: 'proof'}};
  const lock = () => call('lock', {path: 'guarded.txt', reasoning: 'Exercise OpenCode structured edit hook'}, session);
  return {workspace, database, session, options, hook, db, input, output, lock};
}
test('OpenCode plugin rejects unleased writes before side effects, then admits a leased write/edit', async t => {
  const f = await fixture(t);
  const execute = async () => {await f.hook(f.input, f.output); writeFileSync(f.output.args.filePath, f.output.args.content);};
  await assert.rejects(execute, /blocked/); assert.equal(existsSync(f.output.args.filePath), false);
  f.lock(); await execute(); assert.equal(existsSync(f.output.args.filePath), true);
  await f.hook({...f.input, tool: 'edit'}, f.output);
  f.db.prepare('UPDATE leases SET expiresAt=?').run(Date.now() - 1); await assert.rejects(execute, /blocked/);
});
test('OpenCode plugin requires mapped live native identities and preserves unrelated tools', async t => {
  const f = await fixture(t); f.lock();
  await assert.rejects(() => f.hook({...f.input, sessionID: 'ses_unbound'}, f.output), /blocked/);
  await assert.rejects(() => f.hook(f.input, {args: {}}), /blocked/);
  await assert.rejects(() => f.hook(f.input, {args: {filePath: '../escape.txt'}}), /blocked/);
  for (const tool of ['read', 'bash', 'apply_patch', 'custom_write']) await f.hook({tool}, {});
  f.db.prepare('UPDATE sessions SET vendorSession=? WHERE id=?').run('ses_rebound', f.session);
  await assert.rejects(() => f.hook(f.input, f.output), /blocked/);
});
test('OpenCode plugin refuses checker failure, wrong workspace, and input rewrites during admission', async t => {
  const f = await fixture(t); f.lock();
  const missing = (await createOpenCodeLeaseGuard({...f.options, binary: join(f.workspace, 'missing')})({directory: f.workspace}))['tool.execute.before'];
  await assert.rejects(() => missing(f.input, f.output), /blocked/);
  const wrong = (await createOpenCodeLeaseGuard(f.options)({directory: tmpdir()}))['tool.execute.before'];
  await assert.rejects(() => wrong(f.input, f.output), /blocked/);
  const pending = f.hook(f.input, f.output); f.output.args.filePath = join(f.workspace, 'other.txt');
  await assert.rejects(() => pending, /blocked/);
});
