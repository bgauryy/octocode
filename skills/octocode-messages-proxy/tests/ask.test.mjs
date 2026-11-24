import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { fakeComm } from './fake-comm.mjs';
import { fakeProcesses } from './fake-host.mjs';

const relay = fileURLToPath(new URL('../scripts/relay.py', import.meta.url));
const text = 'Status ✓  two  spaces\nsecond line';
const worker = { id: 'worker-1', name: 'build-worker', vendor: 'codex' };

function ask(t, args, comm = {}) {
  const dir = mkdtempSync(path.join(tmpdir(), 'octocode-proxy-ask-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  writeFileSync(path.join(dir, 'claude'), '#!/bin/sh\necho \'{"loggedIn": true}\'\n', { mode: 0o700 });
  writeFileSync(path.join(dir, 'codex'), '#!/bin/sh\nexit 1\n', { mode: 0o700 });
  fakeProcesses(dir, { selfParent: 9001, rows: [[9001, 1, 'node', 'octocode-main-abcd1234']] });
  const state = fakeComm(dir, { peers: [worker], ...comm });
  const result = spawnSync('python3', [relay, 'ask', '--workspace', dir, '--to', 'build-worker', '--text', text,
    '--timeout', '20', '--ready-timeout', '300', ...args], {
    cwd: dir, encoding: 'utf8',
    env: { ...process.env, PATH: dir + path.delimiter + process.env.PATH, PYTHONDONTWRITEBYTECODE: '1' },
  });
  return { result, state: state(), dir };
}

// The proxy's log of one run: scratch under the workspace, never the OS temp dir.
const logDir = dir => path.join(realpathSync(dir), '.octocode', 'tmp', 'octocode-messages-proxy');
const logNamed = stderr => stderr.match(/log: (\S+\.log)/)?.[1];

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

test('ask relays one message, returns the verbatim reply, completes it and stops what it started', t => {
  const { result, state } = ask(t, []);
  assert.equal(result.status, 0, result.stderr);
  const output = JSON.parse(result.stdout);
  assert.deepEqual(output.reply, [{ text: `PONG: ${text}` }]);
  assert.equal(output.outcome, 'completed');
  assert.deepEqual([output.operator.vendor, output.worker.id, output.proxy.id], ['octocode', 'worker-1', 'proxy-1']);
  assert.deepEqual(state.joined.vendor, 'octocode');
  const [message] = state.sent;
  assert.deepEqual([message.to, message.parts[0].data.to, message.replyRequired], ['proxy-1', 'worker-1', true]);
  assert.deepEqual(message.parts[0].data.parts, [{ text }]);
  assert.deepEqual(state.completed, [output.answer]);
  assert.deepEqual(state.left, ['operator-1']);
  assert.equal(alive(state.proxyPid), false);
  const prompt = state.run[state.run.indexOf('--prompt') + 1];
  assert.match(prompt, /Authorized worker: worker-1/);
  assert.equal(state.run[state.run.indexOf('--tools') + 1], 'messaging');
});

test('ask with --session uses that binding and does not leave it', t => {
  const { result, state } = ask(t, ['--session', 'mine']);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(state.joined, undefined);
  assert.deepEqual(state.left, []);
  assert.ok(state.calls.some(([command, session]) => command === 'send_message' && session === 'mine'));
});

test('an old package schema refuses before joining or starting a proxy', t => {
  const { result, state } = ask(t, [], { schema: 'body' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /predates the parts message schema/);
  assert.deepEqual(state.calls.map(([command]) => command), ['schema']);
});

test('an unknown or ambiguous worker refuses before starting a proxy, and the temporary operator leaves', t => {
  for (const peers of [[], [worker, { ...worker, id: 'worker-2' }]]) {
    const { result, state } = ask(t, [], { peers });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Expected one live peer/);
    assert.equal(state.proxyPid, undefined);
    assert.deepEqual(state.left, ['operator-1']);
  }
});

test('a proxy vendor error fails before sending and names the cause', t => {
  const { result, state } = ask(t, [], { proxy: 'error' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /usageLimitExceeded/);
  assert.deepEqual(state.sent, []);
  assert.deepEqual(state.left, ['operator-1'], `stderr: ${result.stderr}\ncalls: ${JSON.stringify(state.calls)}`);
});

test('a proxy that never reports ready fails after --ready-timeout, sends nothing and stops it', t => {
  const { result, state } = ask(t, ['--ready-timeout', '1'], { proxy: 'silent' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Proxy not ready within 1.0s/);
  assert.deepEqual(state.sent, []);
  assert.deepEqual(state.left, ['operator-1']);
  assert.equal(alive(state.proxyPid), false);
});

test('no reply within --timeout fails, leaves the message pending and stops the proxy', t => {
  const { result, state } = ask(t, ['--timeout', '2'], { reply: 'none' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /No reply within 2.0s to message 1/);
  assert.deepEqual(state.completed, []);
  assert.equal(alive(state.proxyPid), false);
});

test('dry-run resolves operator, worker and proxy without starting a model', t => {
  const { result, state } = ask(t, ['--dry-run']);
  assert.equal(result.status, 0, result.stderr);
  const plan = JSON.parse(result.stdout);
  assert.deepEqual([plan.operator.vendor, plan.worker.vendor, plan.proxy.vendor, plan.proxy.model],
    ['octocode', 'codex', 'claude', 'haiku']);
  assert.equal(state.proxyPid, undefined);
  assert.deepEqual(state.sent, []);
});

test('the proxy log is workspace scratch: named in the output, kept on failure, removed on success', t => {
  const failed = ask(t, ['--ready-timeout', '1'], { proxy: 'silent' });
  const kept = logNamed(failed.result.stderr);
  assert.ok(kept, failed.result.stderr);
  assert.equal(path.dirname(kept), logDir(failed.dir));
  assert.ok(existsSync(kept), 'a failed run keeps the log its error names');
  const passed = ask(t, []);
  assert.equal(passed.result.status, 0, passed.result.stderr);
  assert.equal(path.dirname(logNamed(passed.result.stderr)), logDir(passed.dir));
  assert.deepEqual(readdirSync(logDir(passed.dir)), []);
});
