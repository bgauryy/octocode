import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { fakeComm } from './fake-comm.mjs';
import { fakeProcesses } from './fake-host.mjs';

const relay = fileURLToPath(new URL('../scripts/relay.py', import.meta.url));

function workspace(t) {
  const dir = mkdtempSync(path.join(tmpdir(), 'octocode-relay-peers-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

function run(dir, args) {
  const result = spawnSync('python3', [relay, ...args, '--workspace', dir], {
    cwd: dir, encoding: 'utf8',
    env: { ...process.env, PATH: dir + path.delimiter + process.env.PATH, PYTHONDONTWRITEBYTECODE: '1' },
  });
  return { status: result.status, output: result.stdout ? JSON.parse(result.stdout) : null, stderr: result.stderr };
}

/** A fake `claude` whose `agents --json` prints roster, or fails when roster is null. */
function fakeClaude(dir, roster) {
  writeFileSync(path.join(dir, 'claude'), `#!/usr/bin/env python3
import json, sys
roster = json.loads(${JSON.stringify(JSON.stringify(roster))})
if sys.argv[1:2] == ['agents']:
    if roster is None:
        sys.exit('agents unavailable')
    print(json.dumps(roster))
else:
    print(json.dumps({'loggedIn': True}))
`, { mode: 0o700 });
}

test('peers merges store peers and Claude sessions by confirmed vendor/native ID and reports the unreachable', t => {
  const dir = workspace(t);
  const sub = path.join(dir, 'sub');
  mkdirSync(sub);
  fakeComm(dir, {
    peers: [
      { id: 'store-a', name: 'reviewer', vendor: 'claude', vendorSession: 'session-a', task: 'review', status: 'busy', expiresAt: 1 },
      { id: 'store-t', name: 'builder', vendor: 'codex', vendorSession: 'thread-1', task: 'build', status: 'busy', expiresAt: 1 },
      { id: 'store-n', name: 'unnamed', vendor: 'claude', task: '', status: 'idle', expiresAt: 1 },
    ],
  });
  fakeClaude(dir, [
    { pid: 5001, cwd: dir, kind: 'interactive', sessionId: 'session-a', name: 'reviewer', status: 'busy' },
    { pid: 5004, cwd: sub, kind: 'interactive', sessionId: 'session-b', name: 'loner', status: 'idle' },
  ]);
  fakeProcesses(dir, {
    selfParent: 9001,
    rows: [[9001, 1, 'node', 'octocode-main-abcd1234'], [5001, 1, 'claude', 'claude'], [5004, 1, 'claude', 'claude'],
      [5002, 1, 'codex', 'codex'], [5003, 1, 'pi', 'pi'], [5005, 1, 'pi', 'pi']],
    cwd: { 5001: dir, 5004: sub, 5002: dir, 5003: sub, 5005: tmpdir() },
  });
  const { status, output } = run(dir, ['peers']);
  assert.equal(status, 0);
  assert.equal(output.ok, true);
  const nativeIds = output.peers.map(row => `${row.vendor}:${row.nativeId}`);
  assert.equal(new Set(nativeIds).size, nativeIds.length, 'one row per vendor/native ID');
  const merged = output.peers.find(row => row.nativeId === 'session-a');
  assert.deepEqual([merged.store.map(item => item.id), merged.native.pid, merged.routes], [['store-a'], 5001, ['store']]);
  const loner = output.peers.find(row => row.nativeId === 'session-b');
  assert.deepEqual(loner.routes, [], 'a native-only Claude session is unreachable for a non-Claude operator');
  assert.ok(loner.reason);
  assert.ok(output.peers.some(row => row.nativeId === null && row.store[0].id === 'store-n'), 'a peer without a native ID stays its own row');
  assert.deepEqual(output.processes.map(item => [item.vendor, item.binding]).sort(), [['codex', 'unconfirmed'], ['pi', 'none']],
    'roster-confirmed and out-of-workspace processes are not listed');
  assert.equal(output.unreachable, output.peers.filter(row => !row.routes.length).length + output.processes.filter(item => item.binding === 'none').length);
  assert.equal(output.rosters.store.count, 3);
  assert.equal(output.rosters.claude.count, 2);
});

test('a Claude operator reaches native sessions, and its own session and binding are marked self', t => {
  const dir = workspace(t);
  fakeComm(dir, { peers: [{ id: 'mine', name: 'operator', vendor: 'claude', vendorSession: 'session-self', task: '', status: 'busy', expiresAt: 1 }] });
  fakeClaude(dir, [{ pid: 9001, cwd: dir, sessionId: 'session-self', name: 'operator' }, { pid: 5004, cwd: dir, sessionId: 'session-b', name: 'peer' }]);
  fakeProcesses(dir, { selfParent: 9001, rows: [[9001, 1, 'claude', 'claude'], [5004, 1, 'claude', 'claude']], cwd: { 9001: dir, 5004: dir } });
  const { output } = run(dir, ['peers', '--session', 'mine']);
  const self = output.peers.find(row => row.nativeId === 'session-self');
  assert.equal(self.self, true);
  assert.deepEqual(output.peers.find(row => row.nativeId === 'session-b').routes, ['native']);
  assert.equal(output.unreachable, 0);
});

test('an unreadable roster still prints what was read and fails with rosterUnread', t => {
  const dir = workspace(t);
  fakeComm(dir, { peers: [{ id: 'store-t', name: 'builder', vendor: 'codex', vendorSession: 'thread-1', task: '', status: 'busy', expiresAt: 1 }] });
  fakeClaude(dir, null);
  fakeProcesses(dir, {});
  const { status, output } = run(dir, ['peers']);
  assert.equal(status, 1);
  assert.equal(output.ok, false);
  assert.equal(output.error.code, 'rosterUnread');
  assert.match(output.rosters.claude.error, /agents unavailable/);
  assert.deepEqual(output.peers.map(row => row.store[0].id), ['store-t']);
});

const answer = (id, replyTo, outcome = 'completed') => ({ id, replyTo, outcome, sender: 'proxy-1', senderName: 'proxy', replyRequired: false, parts: [{ text: `answer ${id}` }] });

test('wait finds the answer to the receipt, completes only it and ignores other pending mail', t => {
  const dir = workspace(t);
  const state = fakeComm(dir, { inbox: [answer(3, 99), { id: 4, sender: 'x', parts: [{ text: 'fyi' }], replyRequired: false }], later: [answer(8, 5)] });
  const { status, output } = run(dir, ['wait', '--session', 'mine', '--receipt', '5', '--timeout', '20']);
  assert.equal(status, 0);
  assert.deepEqual([output.sent, output.answer, output.outcome], [5, 8, 'completed']);
  assert.deepEqual(output.reply, [{ text: 'answer 8' }]);
  const recorded = state();
  assert.deepEqual(recorded.completed, [8]);
  const waits = recorded.calls.filter(([command]) => command === 'inbox-wait');
  assert.ok(waits.length > 0);
  for (const [, session, payload] of waits) {
    assert.equal(session, 'mine');
    assert.ok(payload.after >= 5, 'each wait asks only for deliveries newer than the receipt and the mail already read');
  }
});

test('wait exits 1 for a non-completed outcome and still acknowledges the answer', t => {
  const dir = workspace(t);
  const state = fakeComm(dir, { inbox: [answer(6, 5, 'failed')] });
  const { status, output } = run(dir, ['wait', '--session', 'mine', '--receipt', '5']);
  assert.equal(status, 1);
  assert.equal(output.outcome, 'failed');
  assert.deepEqual(state().completed, [6]);
});

test('wait reports a timeout, an inactive binding and a bad receipt without completing anything', t => {
  const dir = workspace(t);
  const state = fakeComm(dir, { inbox: [answer(3, 99)] });
  const timeout = run(dir, ['wait', '--session', 'mine', '--receipt', '5', '--timeout', '1']);
  assert.equal(timeout.status, 1);
  assert.deepEqual([timeout.output.ok, timeout.output.error.code], [false, 'noReply']);
  const inactive = run(dir, ['wait', '--session', 'other', '--receipt', '5']);
  assert.equal(inactive.status, 1);
  assert.equal(inactive.output.error.code, 'bindingInactive');
  const bad = run(dir, ['wait', '--session', 'mine', '--receipt', '0']);
  assert.equal(bad.status, 2);
  assert.deepEqual(state().completed, []);
});
