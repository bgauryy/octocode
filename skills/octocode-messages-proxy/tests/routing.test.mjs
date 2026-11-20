import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { fakeProcesses } from './fake-host.mjs';

const tools = fileURLToPath(new URL('../scripts/', import.meta.url));
const text = 'Status ✓\r\nsecond line  \n';

function workspace(t) {
  const dir = mkdtempSync(path.join(tmpdir(), 'octocode-relay-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

function send(dir, args) {
  return spawnSync('python3', [path.join(tools, 'relay.py'), 'send', ...args], {
    cwd: dir, input: text, encoding: 'utf8',
    env: { ...process.env, PATH: dir + path.delimiter + process.env.PATH, PYTHONDONTWRITEBYTECODE: '1' },
  });
}

test('Codex queue receives one literal argument including whitespace', t => {
  const dir = workspace(t);
  writeFileSync(path.join(dir, 'codex'), '#!/usr/bin/env python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n', { mode: 0o700 });
  const native = send(dir, ['--from', 'codex', '--to', 'codex:thread']);
  assert.equal(native.status, 0, native.stderr);
  assert.deepEqual(JSON.parse(native.stdout), ['queue', '--thread', 'thread', `--message=${text}`]);
  const mixed = send(dir, ['--from', 'claude', '--to', 'codex:thread']);
  assert.notEqual(mixed.status, 0);
  assert.match(mixed.stderr, /requires --session/);
  assert.equal(mixed.stdout, '');
});

test('a native send without --from uses the host found in the process tree', t => {
  const dir = workspace(t);
  writeFileSync(path.join(dir, 'codex'), '#!/usr/bin/env python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n', { mode: 0o700 });
  fakeProcesses(dir, { selfParent: 9001, rows: [[9001, 1, '/opt/homebrew/bin/codex', 'codex']] });
  const native = send(dir, ['--to', 'codex:thread']);
  assert.equal(native.status, 0, native.stderr);
  assert.equal(JSON.parse(native.stdout)[0], 'queue');
  fakeProcesses(dir, { selfParent: 9001, rows: [[9001, 1, 'node', 'octocode-main-abcd1234']] });
  const crossVendor = send(dir, ['--to', 'codex:thread']);
  assert.notEqual(crossVendor.status, 0);
  assert.match(crossVendor.stderr, /requires --session/);
});

test('Claude authorizes one named recipient and restores the original text at dispatch', t => {
  const dir = workspace(t);
  const request = path.join(dir, 'message.json');
  writeFileSync(request, JSON.stringify({ name: 'build', text }));
  function guard(to) {
    return spawnSync('python3', [path.join(tools, 'claude_send.py'), request], {
      input: JSON.stringify({ tool_name: 'SendMessage', tool_input: { to, message: text.trim() } }),
      encoding: 'utf8',
    });
  }
  assert.equal(guard('review').status, 2);
  const first = guard('build');
  assert.equal(first.status, 0, first.stderr);
  assert.deepEqual(JSON.parse(first.stdout).hookSpecificOutput.updatedInput, { to: 'build', message: text });
  assert.equal(guard('build').status, 2);
});

test('Octocode resolves the vendor and native ID across pages, then sends exactly once', t => {
  const dir = workspace(t);
  writeFileSync(path.join(dir, 'octocode-agents-communication'), `#!/usr/bin/env python3
import json, sys
op = sys.argv[2]
if op == 'binding':
    output = {'id': 'sender', 'vendor': 'claude', 'active': True}
elif op == 'peers':
    page = json.load(sys.stdin)
    if page == {}:
        output = {'items': [{'id':'wrong','vendor':'claude','vendorSession':'thread'}], 'next': {'input': {'cursor': 'next'}}}
    else:
        assert page == {'cursor': 'next'}
        output = {'items': [{'id':'worker','vendor':'codex','vendorSession':'thread'}]}
elif op == 'schema':
    output = {'inputSchema': {'properties': {'parts': {}, 'contextId': {}}}}
elif op == 'send_message':
    output = json.load(sys.stdin)
    with open('sent', 'x') as f:
        f.write(output['to'])
else:
    sys.exit('unexpected command ' + op)
print(json.dumps(output))
`, { mode: 0o700 });
  const result = send(dir, ['--session', 'sender', '--from', 'claude', '--to', 'codex:thread',
    '--context', 'topic', '--key', 'request-1', '--reply-required']);
  assert.equal(result.status, 0, result.stderr);
  const { transport, from, to, receipt } = JSON.parse(result.stdout);
  assert.deepEqual([transport, from, to], ['octocode', 'sender', 'worker']);
  assert.deepEqual(receipt.parts, [{ text }]);
  assert.deepEqual([receipt.to, receipt.contextId, receipt.replyRequired, receipt.key], ['worker', 'topic', true, 'request-1']);
});
