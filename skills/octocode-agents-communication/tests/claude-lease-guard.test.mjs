import {test} from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync, execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdirSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {randomUUID} from 'node:crypto';
import {DatabaseSync} from 'node:sqlite';
import { root, launcherCommand as binary, tempWorkspace, jsonCall } from './helpers.mjs';
const script = join(root, 'scripts/hooks/claude-lease-guard.mjs');
const exec = promisify(execFile);
function fixture(t) {
  const workspace = tempWorkspace(t, "claude-guard-'", { real: true }), database = join(workspace, 'coord.sqlite'), hostSession = randomUUID();
  const call = jsonCall(binary, workspace, database);
  const session = call('join', {name: 'claude-guard', vendor: 'claude', vendorSession: hostSession}).id;
  const db = new DatabaseSync(database); t.after(() => db.close());
  const flags = ['--binary', binary, '--workspace', workspace, '--database', database, '--session', session, '--host-session', hostSession];
  const event = {hook_event_name: 'PreToolUse', tool_name: 'Write', session_id: hostSession, cwd: workspace, tool_input: {file_path: 'guarded.txt', content: 'hello'}};
  const run = (input = event, args = flags) => JSON.parse(execFileSync(process.execPath, [script, ...args], {input: typeof input === 'string' ? input : JSON.stringify(input), encoding: 'utf8', timeout: 5000}));
  const lock = path => call('lock', {path, reasoning: 'Verify Claude structured write admission'}, session).lease;
  return {workspace, database, session, db, flags, event, call, run, lock};
}
const denied = output => assert.equal(output.hookSpecificOutput?.permissionDecision, 'deny');
test('Claude Write/Edit guard denies unleased calls and accepts own covering leases without bypassing host permissions', t => {
  const f = fixture(t); denied(f.run());
  f.lock('guarded.txt'); assert.deepEqual(f.run(), {});
  assert.deepEqual(f.run({...f.event, tool_name: 'Edit'}), {});
  denied(f.run({...f.event, tool_input: {file_path: 'other.txt'}}));
  f.db.prepare('UPDATE leases SET expiresAt=?').run(Date.now() - 1); denied(f.run());
});
test('Claude guard fails closed on identity/path/checker failures but ignores unrelated tool events', t => {
  const f = fixture(t); f.lock('guarded.txt');
  denied(f.run({...f.event, session_id: randomUUID()}));
  denied(f.run({...f.event, cwd: tmpdir()}));
  denied(f.run({...f.event, tool_input: {}}));
  denied(f.run({...f.event, tool_input: {file_path: '../escape.txt'}}));
  for (const path of ['src/../guarded.txt', '@guarded.txt', '~/guarded.txt', 'file:///guarded.txt', 'space\u00a0name']) denied(f.run({...f.event, tool_input: {file_path: path}}));
  denied(f.run('{')); denied(f.run(f.event, []));
  const missing = [...f.flags]; missing[1] = join(f.workspace, 'missing'); denied(f.run(f.event, missing));
  for (const tool_name of ['Read', 'Bash', 'CustomEdit']) assert.deepEqual(f.run({...f.event, tool_name}, []), {});
  assert.deepEqual(f.run({...f.event, hook_event_name: 'PostToolUse'}, []), {});
  f.db.prepare('UPDATE sessions SET vendorSession=? WHERE id=?').run(randomUUID(), f.session); denied(f.run());
});
test('Claude guard handles child cwd paths and is read-only with a held SQLite writer', async t => {
  const f = fixture(t); mkdirSync(join(f.workspace, 'src')); f.lock('src/guarded.txt');
  const event = {...f.event, cwd: join(f.workspace, 'src')};
  const before = f.db.prepare('PRAGMA data_version').get().data_version;
  f.db.exec('BEGIN IMMEDIATE');
  try {
    const child = exec(process.execPath, [script, ...f.flags], {timeout: 5000});
    child.child.stdin.end(JSON.stringify(event));
    assert.deepEqual(JSON.parse((await child).stdout), {});
  } finally { f.db.exec('ROLLBACK'); }
  assert.equal(f.db.prepare('PRAGMA data_version').get().data_version, before);
  f.db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now() - 1, f.session); denied(f.run(event));
});
test('configuration is a shell-safe preview and malformed configuration does not look valid', t => {
  const f = fixture(t); f.lock('guarded.txt');
  const config = JSON.parse(execFileSync(process.execPath, [script, '--config', ...f.flags], {encoding: 'utf8'}));
  const item = config.hooks.PreToolUse[0]; assert.equal(item.matcher, '^(Write|Edit)$');
  assert.equal(item.hooks[0].timeout, 10);
  assert.deepEqual(JSON.parse(execFileSync('/bin/sh', ['-c', item.hooks[0].command], {input: JSON.stringify(f.event), encoding: 'utf8'})), {});
  assert.throws(() => execFileSync(process.execPath, [script, '--config'], {stdio: 'pipe'}));
});
