import assert from 'node:assert/strict';
import { execFileSync, type ExecFileSyncOptionsWithStringEncoding } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { bindingFlags, errorStack, integer, isReply, list, object, parseReply, sha256, text, type Reply } from './script-support.ts';

// Packs the package with npm, extracts the tarball and drives its dist/communication.js directly on Windows, Linux and
// macOS. No vendor account, npm dependencies or host SDK is required.
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const outputIndex = process.argv.indexOf('--output');
if (outputIndex >= 0 && !process.argv[outputIndex + 1]) throw new Error('--output requires a file path');
const output = resolve(outputIndex < 0 ? join(root, 'out/release-smoke.json') : process.argv[outputIndex + 1] ?? '');
const run = (file: string, args: string[], options: Omit<ExecFileSyncOptionsWithStringEncoding, 'encoding'> = {}): string => execFileSync(file, args, {
  encoding: 'utf8', timeout: 15000, killSignal: 'SIGKILL', maxBuffer: 2 * 1024 * 1024,
  stdio: ['pipe', 'pipe', 'pipe'], ...options,
});
const target = `${process.platform}-${process.arch}-node${process.versions.node.split('.')[0]}`;
const hash = (file: string): string => sha256(readFileSync(file));
const temporary = realpathSync(mkdtempSync(join(tmpdir(), 'communication-release-')));
// A session on a smoke database registers itself under the Octocode home; keep that out of the real one.
process.env.OCTOCODE_HOME = join(temporary, 'octocode-home');
/** What the smoke run records in its JSON report. */
interface SmokeResult {
  passed: boolean; startedAt: string; target: string; platform: string; node: string; version: string;
  checks: string[]; restore: boolean;
  package?: { archive: string; files: number; sha256: string };
  entrySha256?: string; operatingSha256?: string; error?: string; completedAt?: string;
}
const result: SmokeResult = {
  passed: false, startedAt: new Date().toISOString(), target, platform: process.platform,
  node: process.version,
  version: text(parseReply(readFileSync(join(root, 'package.json'), 'utf8')).version, 'package.json version'),
  checks: [], restore: false,
};
let binary = '';
const runRuntime = (entry: string, args: string[], options?: Omit<ExecFileSyncOptionsWithStringEncoding, 'encoding'>): string => run(process.execPath, [entry, ...args], options);
const cli = (workspace: string, database: string, args: string[], session?: string): Reply => parseReply(runRuntime(binary, [...args, ...bindingFlags(workspace, database, session)]));
const call = (workspace: string, database: string, name: string, input: Reply, session?: string): Reply => cli(workspace, database, [name, JSON.stringify(input)], session);
const items = (reply: Reply): unknown[] => list(reply.items, 'items');
const count = (db: DatabaseSync, sql: string): number => integer(db.prepare(sql).get()?.n, sql);
const inspect = <T>(database: string, fn: (db: DatabaseSync) => T): T => {
  const db = new DatabaseSync(database, { readOnly: true });
  try { return fn(db); } finally { db.close(); }
};
try {
  if (process.env.COMMUNICATION_EXPECTED_TARGET) assert.equal(target, process.env.COMMUNICATION_EXPECTED_TARGET, 'Use the operating system and Node version declared by this CI job');
  const packed = object(list(JSON.parse(run('npm', ['pack', '--ignore-scripts', '--json', '--pack-destination', temporary], { cwd: root, shell: process.platform === 'win32' })), 'npm pack output')[0], 'npm pack entry');
  const archive = join(temporary, text(packed.filename, 'packed filename'));
  result.package = { archive, files: list(packed.files, 'packed files').length, sha256: hash(archive) };
  run('tar', ['-xzf', archive, '-C', temporary]);
  const installed = join(temporary, 'package');
  binary = join(installed, 'dist/communication.js');
  result.entrySha256 = hash(binary); result.operatingSha256 = hash(join(installed, 'OPERATING.md'));
  assert.equal(parseReply(runRuntime(binary, ['skill'])).instructions, readFileSync(join(installed, 'OPERATING.md'), 'utf8'));
  const help = parseReply(run(process.execPath, [join(installed, 'bin/octocode-agents-communication.mjs'), '/cli', '--help', '--json']));
  assert.ok(list(help.commands, 'help commands').some(command => isReply(command) && command.name === 'join'));
  assert.equal(run(process.execPath, [join(installed, 'bin/octocode-agents-communication.mjs'), '--version']).trim(), result.version);
  result.checks.push('packed runtime, CLI entry and packaged skill agree');

  // The `./hosts/*` package exports load from the packed files and expose their entry points.
  const hostExports = { 'pi-extension': ['registerBoundTools'], 'pi-inbox': ['registerPiInbox'], 'lease-check': ['checkHostWrite', 'plainHostPath'], 'opencode-lease-guard': ['createOpenCodeLeaseGuard'] };
  for (const [name, exported] of Object.entries(hostExports)) {
    const host = object(await import(pathToFileURL(join(installed, 'dist/hosts', `${name}.js`)).href), `hosts/${name} module`);
    for (const key of exported) assert.equal(typeof host[key], 'function', `hosts/${name} exports ${key}`);
  }
  assert.ok(parseReply(runRuntime(join(installed, 'dist/hosts/claude-lease-guard.js'), ['--help'])).usage);
  result.checks.push('packed host adapters load and the Claude guard prints its usage');

  const workspace = join(temporary, 'workspace'); mkdirSync(workspace);
  const database = join(workspace, 'communication.sqlite');
  const a = text(call(workspace, database, 'join', { name: 'portable-sender', vendor: 'generic' }).id, 'sender id');
  const b = text(call(workspace, database, 'join', { name: 'portable-receiver', vendor: 'generic' }).id, 'receiver id');
  assert.equal(items(call(workspace, database, 'peers', {}, a)).length, 2);
  const lease = call(workspace, database, 'lock', { path: 'src', kind: 'tree', reasoning: 'Reserve shared source work before editing' }, a);
  assert.equal(lease.ok, true);
  assert.equal(call(workspace, database, 'lock', { path: 'src/new-file.txt', reasoning: 'Detect another writer before creating a file' }, b).ok, false);
  assert.equal(call(workspace, database, 'unlock', { leaseId: object(lease.lease, 'lease').id }, a).released, true);
  const handoff = call(workspace, database, 'lock', { path: 'src/new-file.txt', reasoning: 'Continue after the previous owner releases its lease' }, b);
  assert.equal(handoff.ok, true);
  call(workspace, database, 'unlock', { leaseId: object(handoff.lease, 'handoff lease').id }, b);
  result.checks.push('workspace discovery and conflicting lease handoff');

  const request = { to: b, parts: [{ text: 'Confirm this work handoff' }], replyRequired: true, reasoning: 'Verify durable delivery across CLI and MCP', key: 'release-question', wake: 'action' };
  const sent = call(workspace, database, 'send_message', request, a);
  assert.deepEqual(call(workspace, database, 'send_message', request, a), sent);
  const frames = [
    { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'release-smoke', version: '1' } } },
    { jsonrpc: '2.0', method: 'notifications/initialized' },
    { jsonrpc: '2.0', id: 2, method: 'tools/list', params: {} },
    { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'messages', arguments: { type: 'complete', input: { message: sent.id, outcome: 'completed', reply: [{ text: 'Handoff confirmed' }], reasoning: 'Complete the requested check' } } } },
  ];
  const responses = runRuntime(binary, ['mcp', '--tools', 'agents,messages', ...bindingFlags(workspace, database, b)], { input: frames.map(frame => JSON.stringify(frame)).join('\n') + '\n' }).trim().split('\n').map(parseReply);
  assert.equal(responses.length, 3);
  const [initialized, listed, completed] = responses.map((response, index) => object(response.result, `MCP response ${index + 1} result`));
  assert.equal(initialized?.protocolVersion, '2024-11-05');
  assert.deepEqual(list(listed?.tools, 'MCP tools').map(tool => object(tool, 'MCP tool').name).sort(), ['agents', 'messages']);
  assert.equal(completed?.isError, undefined, JSON.stringify(responses[2]));
  const reply = parseReply(text(object(list(completed?.content, 'MCP content')[0], 'MCP content item').text, 'MCP content text'));
  assert.equal(reply.completed, true);
  assert.equal(reply.count, 1);
  assert.ok(Number.isSafeInteger(reply.id));
  assert.equal(reply.recipients, 1);
  assert.equal(items(call(workspace, database, 'inbox', {}, b)).length, 0);
  const inbox = items(call(workspace, database, 'inbox', {}, a));
  assert.equal(inbox.length, 1); assert.equal(object(inbox[0], 'inbox item').replyTo, sent.id);
  const record = object(items(call(workspace, database, 'fetch', { incoming: true }, a))[0], 'fetched record');
  assert.equal(record.type, 'message'); assert.equal(record.path, workspace);
  assert.equal(record.from, b); assert.equal(record.to, a);
  const data = object(record.data, 'record data');
  assert.equal(data.messageId, reply.id); assert.equal(data.replyTo, sent.id);
  const memory = call(workspace, database, 'record', { type: 'memory', branch: 'portable', data: { content: 'Retained portable fact', optional: null }, key: 'portable-memory' }, a);
  const fetched = items(call(workspace, database, 'fetch', { type: 'memory', branch: 'portable', search: 'Retained', where: { optional: null } }, b));
  assert.deepEqual(fetched, [memory]); assert.equal(memory.to, null);
  call(workspace, database, 'complete', { message: reply.id }, a);
  inspect(database, db => {
    assert.equal(count(db, 'SELECT count(*) n FROM messages'), 2);
    assert.equal(count(db, 'SELECT count(*) n FROM deliveries WHERE acknowledgedAt IS NULL'), 0);
    assert.equal(count(db, 'SELECT count(*) n FROM leases'), 0);
    assert.ok(count(db, 'SELECT count(*) n FROM records') > 0);
  });
  result.checks.push('CLI/MCP correlated reply, atomic acknowledgement, deduplication and unified records');

  const snapshot = join(workspace, 'backup.sqlite'), restored = join(workspace, 'restored.sqlite');
  const exported = cli(workspace, database, ['db', 'export', JSON.stringify({ path: snapshot })]);
  assert.equal(exported.integrity, 'ok');
  assert.equal(exported.sha256, hash(snapshot));
  copyFileSync(snapshot, restored);
  assert.equal(cli(workspace, restored, ['db', 'info']).compatible, true);
  inspect(restored, db => assert.equal(count(db, 'SELECT count(*) n FROM messages'), 2));
  result.restore = true;
  result.checks.push('Current-schema backup integrity and restore');
  assert.equal(hash(binary), result.entrySha256);
  result.passed = true;
} catch (error) {
  result.error = errorStack(error);
  process.exitCode = 1;
} finally {
  rmSync(temporary, { recursive: true, force: true });
  result.completedAt = new Date().toISOString();
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify({ passed: result.passed, target, output, checks: result.checks, restore: result.restore, error: result.error }));
}
