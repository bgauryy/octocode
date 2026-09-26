import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { digest as hash } from './artifact-checks.mjs';
import { packSkill } from './pack-skill.mjs';

// Uses the extracted native executable directly: no shell/shebang, vendor account,
// npm dependencies, or host SDK is required on Windows, Linux, or macOS.
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const outputIndex = process.argv.indexOf('--output');
const output = resolve(outputIndex < 0 ? join(root, 'out/release-smoke.json') : process.argv[outputIndex + 1]);
const run = (file, args, options = {}) => execFileSync(file, args, {
  encoding: 'utf8', timeout: 15000, killSignal: 'SIGKILL', maxBuffer: 2 * 1024 * 1024,
  stdio: ['pipe', 'pipe', 'pipe'], ...options,
});
const rustc = run('rustc', ['-vV']);
const target = rustc.match(/^host: (.+)$/m)[1];
const temporary = realpathSync(mkdtempSync(join(tmpdir(), 'communication-release-')));
const result = {
  passed: false, startedAt: new Date().toISOString(), target, platform: process.platform,
  node: process.version, rustc: rustc.split('\n')[0], harnessSha256: hash(fileURLToPath(import.meta.url)),
  version: JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version,
  sourceRevision: run('git', ['rev-parse', 'HEAD'], { cwd: root }).trim(),
  dirty: run('git', ['status', '--porcelain', '--', '.'], { cwd: root }).trim().length > 0,
  providerCalls: 0, checks: [], restore: false,
};
let binary;
const cli = (workspace, database, args, session) => JSON.parse(run(binary, [
  ...args, '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : []),
]));
const call = (workspace, database, name, input, session) => cli(workspace, database, [name, JSON.stringify(input)], session);
const inspect = (database, fn) => {
  const db = new DatabaseSync(database, { readOnly: true });
  try { return fn(db); } finally { db.close(); }
};
try {
  if (process.env.COMMUNICATION_EXPECTED_TARGET) assert.equal(target, process.env.COMMUNICATION_EXPECTED_TARGET, 'Use a native host toolchain matching the release target');
  result.package = packSkill(root, { hostTarget: target });
  run('tar', ['-xzf', result.package.archive, '-C', temporary]);
  const skill = join(temporary, 'octocode-agents-communication');
  binary = join(skill, 'scripts', `octocode-agents-communication${process.platform === 'win32' ? '.exe' : ''}`);
  result.binarySha256 = hash(binary); result.skillSha256 = hash(join(skill, 'SKILL.md'));
  assert.equal(JSON.parse(run(binary, ['skill'])).instructions, readFileSync(join(skill, 'SKILL.md'), 'utf8'));
  result.checks.push('extracted executable and embedded skill agree');
  if (process.platform === 'win32') {
    const help = run('pwsh', ['-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', join(skill, 'scripts/agents-communication.ps1'), '--help']);
    const contract = JSON.parse(help);
    assert.equal(contract.package, '@octocodeai/octocode-agents-communication');
    assert.equal(contract.implementation, 'Rust');
    result.launcher = { passed: true, shell: 'pwsh' };
  } else {
    assert.equal(result.package.launcher.passed, true);
    result.launcher = result.package.launcher;
  }

  const workspace = join(temporary, 'workspace'); mkdirSync(workspace);
  const database = join(workspace, 'communication.sqlite');
  const a = call(workspace, database, 'join', { name: 'portable-sender', vendor: 'generic' }).id;
  const b = call(workspace, database, 'join', { name: 'portable-receiver', vendor: 'generic' }).id;
  assert.equal(call(workspace, database, 'peers', {}, a).items.length, 2);
  const lease = call(workspace, database, 'lock', { path: 'src', kind: 'tree', reasoning: 'Reserve shared source work before editing' }, a);
  assert.equal(lease.ok, true);
  assert.equal(call(workspace, database, 'lock', { path: 'src/new-file.txt', reasoning: 'Detect another writer before creating a file' }, b).ok, false);
  assert.equal(call(workspace, database, 'unlock', { leaseId: lease.lease.id }, a).released, true);
  const handoff = call(workspace, database, 'lock', { path: 'src/new-file.txt', reasoning: 'Continue after the previous owner releases its lease' }, b);
  assert.equal(handoff.ok, true);
  call(workspace, database, 'unlock', { leaseId: handoff.lease.id }, b);
  result.checks.push('workspace discovery and conflicting lease handoff');

  const request = { to: b, body: 'Confirm this work handoff', reasoning: 'Verify durable delivery across CLI and MCP', key: 'release-question', wake: 'action' };
  const sent = call(workspace, database, 'send_message', request, a);
  assert.deepEqual(call(workspace, database, 'send_message', request, a), sent);
  const frames = [
    { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'release-smoke', version: '1' } } },
    { jsonrpc: '2.0', method: 'notifications/initialized' },
    { jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} },
    { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'send_message', arguments: { to: a, body: 'Handoff confirmed', reasoning: 'Complete the requested check', replyTo: sent.id, ackReply: true, key: 'release-answer' } } },
  ];
  const responses = run(binary, ['mcp', '--tools', 'peers,send_message,ack', '--workspace', workspace, '--database', database, '--session', b], { input: frames.map(frame => JSON.stringify(frame)).join('\n') + '\n' }).trim().split('\n').map(line => JSON.parse(line));
  assert.equal(responses.length, 3);
  assert.equal(responses[0].result.protocolVersion, '2024-11-05');
  assert.deepEqual(responses[1].result.tools.map(tool => tool.name).sort(), ['ack', 'peers', 'send_message']);
  assert.equal(responses[2].result.isError, undefined, JSON.stringify(responses[2]));
  const reply = JSON.parse(responses[2].result.content[0].text);
  assert.equal(call(workspace, database, 'inbox', {}, b).items.length, 0);
  const inbox = call(workspace, database, 'inbox', {}, a).items;
  assert.equal(inbox.length, 1); assert.equal(inbox[0].replyTo, sent.id);
  call(workspace, database, 'ack', { message: reply.id }, a);
  inspect(database, db => {
    assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n, 2);
    assert.equal(db.prepare('SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL').get().n, 0);
    assert.equal(db.prepare('SELECT count(*) n FROM leases').get().n, 0);
    assert.ok(db.prepare('SELECT count(*) n FROM audit').get().n > 0);
  });
  result.checks.push('CLI/MCP correlated reply, atomic acknowledgement, deduplication and audit');

  const snapshot = join(workspace, 'backup.sqlite'), restored = join(workspace, 'restored.sqlite');
  const exported = cli(workspace, database, ['db', 'export', JSON.stringify({ path: snapshot })]);
  assert.equal(exported.integrity, 'ok');
  assert.equal(exported.sha256, hash(snapshot));
  copyFileSync(snapshot, restored);
  assert.equal(cli(workspace, restored, ['db', 'info']).compatible, true);
  inspect(restored, db => assert.equal(db.prepare('SELECT count(*) n FROM messages').get().n, 2));
  result.restore = true;
  result.checks.push('Current-schema backup integrity and restore');
  assert.equal(hash(binary), result.binarySha256);
  result.passed = true;
} catch (error) {
  result.error = error.stack;
  process.exitCode = 1;
} finally {
  rmSync(temporary, { recursive: true, force: true });
  result.completedAt = new Date().toISOString();
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify({ passed: result.passed, target, output, checks: result.checks, restore: result.restore, error: result.error }));
}
