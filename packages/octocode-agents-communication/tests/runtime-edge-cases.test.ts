// Regression tests for edge-case behavior the previous runtime defined and the TypeScript runtime must keep: early-closed
// stdout, byte order marks, blank text, host-hook input, MCP and vendor frames, record search words, UUIDs and view cursors.
import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { execFileSync, spawn, spawnSync } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';
import { binary, isChildFailure, only, tempWorkspace, withReasoning } from './helpers.ts';
import { fixture as storeFixture, runtime } from './store-support.ts';

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-edge-', { real: true });
  const database = join(workspace, 'audit.sqlite');
  const flags = ['--workspace', workspace, '--database', database];
  const args = (command: string, session?: string) => [command, '-', ...flags, ...(session ? ['--session', session] : [])];
  const call = (command: string, input: Record<string, unknown> = {}, session?: string): JsonRecord => JSON.parse(execFileSync(binary, args(command, session), {
    input: JSON.stringify(withReasoning(command, input)), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], timeout: 30_000,
  }));
  const fails = (command: string, input?: Record<string, unknown>, session?: string): string | null => {
    try {
      call(command, input, session);
    } catch (error) {
      assert.ok(isChildFailure(error));
      return String(error.stderr);
    }
    return null;
  };
  const db = () => {
    const handle = new DatabaseSync(database, { readOnly: true });
    t.after(() => handle.close());
    return handle;
  };
  const join_ = (name: string, vendor = 'any-vendor'): string => call('join', { name, vendor }).id;
  return { workspace, database, flags, args, call, fails, db, join: join_ };
}

/** Runs the CLI with its standard output reader already closed, so that the first write fails with EPIPE. */
function withClosedReader(argv: string[], input?: string) {
  return new Promise<{ code: number | null; stderr: string }>((resolve, reject) => {
    const child = spawn(binary, argv, { stdio: ['pipe', 'pipe', 'pipe'] });
    let stderr = '';
    child.stderr.on('data', chunk => (stderr += chunk));
    child.stdout.destroy();
    child.stdin.end(input);
    child.once('error', reject);
    child.once('exit', code => resolve({ code, stderr }));
  });
}

// --- Early-closed stdout -------------------------------------------------------------------------------------------

test('a raw hook whose stdout reader closed early records the offer uncertain, and the mail stays recoverable', async t => {
  const f = fixture(t);
  const sender = f.join('sender'), receiver = f.join('receiver');
  f.call('attach', { transport: 'raw' }, receiver);
  const sent = f.call('send_message', { to: receiver, body: 'must not be lost' }, sender);
  const { code } = await withClosedReader(['hook', '{"format":"text"}', ...f.flags, '--session', receiver]);
  assert.equal(code, 0, 'a consumer closing the pipe is not a failure');
  const [row]: JsonRecord[] = f.db().prepare('SELECT message,state,error FROM dispatches').all();
  assert.equal(row.message, sent.id);
  assert.equal(row.state, 'uncertain');
  assert.match(row.error, /EPIPE|broken pipe/i);
  assert.equal(f.call('inbox', {}, receiver).items[0].id, sent.id, 'the message is still pending');
  assert.equal(f.call('retry_delivery', { message: sent.id, reason: 'operator retry' }, receiver).ready, true);
  const again = f.call('hook', { format: 'json' }, receiver);
  assert.deepEqual(again.items, [{ id: sent.id }]);
  assert.match(again.context, /must not be lost/);
});

test('a host post-tool hook whose stdout reader closed early records the offer uncertain', async t => {
  const f = fixture(t);
  const event = (name: string) => JSON.stringify({ hook_event_name: name, session_id: 'host-fixture', cwd: f.workspace });
  const hook = () => ['host-hook', '--vendor', 'claude', ...f.flags];
  execFileSync(binary, hook(), { input: event('SessionStart'), encoding: 'utf8' });
  const [{ id: receiver }]: JsonRecord[] = f.db().prepare('SELECT id FROM sessions').all();
  const sent = f.call('send_message', { to: receiver, body: 'host mail' }, f.join('sender'));
  const { code } = await withClosedReader(hook(), event('PostToolUse'));
  assert.equal(code, 0);
  const [row] = f.db().prepare('SELECT message,state FROM dispatches').all();
  assert.deepEqual({ ...row }, { message: sent.id, state: 'uncertain' });
  assert.equal(f.call('inbox', {}, receiver).items[0].id, sent.id);
});

// --- Byte order marks ----------------------------------------------------------------------------------------------

test('sharing a document that begins with a byte order mark is idempotent', t => {
  const f = fixture(t), author = f.join('author');
  const input = { name: 'bom.md', content: '\uFEFFhello world' };
  const first = f.call('share_document', input, author);
  assert.equal(first.document.bytes, 14);
  assert.deepEqual(f.call('share_document', input, author), { created: false, document: first.document });
});

test('paged reads of a document with byte order marks reassemble it byte for byte', t => {
  const f = fixture(t), author = f.join('author'), reader = f.join('reader');
  const content = '\uFEFFab\uFEFFcdefghij\uFEFF\uFEFFklm';
  f.call('share_document', { name: 'marks.md', content }, author);
  let input = { name: 'marks.md', limit: 4 }, output = '';
  for (let pages = 0; input; pages += 1) {
    assert.ok(pages < 50);
    const page = f.call('read_document', input, reader);
    assert.equal(page.offset, Buffer.byteLength(output));
    output += page.content;
    input = page.next?.input;
  }
  assert.equal(output, content);
});

test('a byte order mark in JSON read from standard input is invalid JSON, as the previous runtime rejected it', t => {
  const f = fixture(t);
  const bom = Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from('{}')]);
  assert.throws(() => execFileSync(binary, ['peers', '-', ...f.flags], { input: bom, stdio: ['pipe', 'pipe', 'pipe'] }), error => isChildFailure(error) && /Invalid JSON input/.test(error.stderr.toString()));
});

test('UTF-8 text decoding keeps U+FEFF and still refuses invalid bytes', () => {
  assert.equal(runtime.jsonText.decodeUtf8(Buffer.from('\uFEFFx')), '\uFEFFx');
  assert.throws(() => runtime.jsonText.decodeUtf8(Buffer.from([0xff])), TypeError);
  assert.deepEqual(runtime.jsonText.parseJsonBytes(Buffer.from('\uFEFF{"a":1}')), { a: 1 }, 'json.loads(bytes) reads one leading mark');
  assert.throws(() => runtime.jsonText.parseJsonBytes(Buffer.from('\uFEFF\uFEFF{}')), SyntaxError);
  assert.throws(() => runtime.jsonText.parseJsonBytes(Buffer.from([0x22, 0xff, 0x22])), TypeError, 'invalid UTF-8 is an error, not U+FFFD');
});

// --- Backslash in file names ---------------------------------------------------------------------------------------

test('a backslash is an ordinary filename character on POSIX', { skip: process.platform === 'win32' }, t => {
  const f = fixture(t), a = f.join('a'), b = f.join('b');
  const file = f.call('lock', { path: 'a\\b', kind: 'file' }, a);
  assert.equal(file.ok, true);
  assert.equal(file.lease.path, 'a/b', 'the display form replaces backslashes, as the previous runtime did');
  const { path }: JsonRecord = only(f.db().prepare('SELECT path FROM leases').get());
  assert.ok(path.endsWith('/a\\b'), 'the stored path keeps the backslash');
  assert.equal(f.call('lock', { path: 'a', kind: 'tree' }, b).ok, true, 'the tree lease on `a` does not cover the file `a\\b`');
  assert.equal(runtime.paths.leaseKey('/w/a\\b'), '/w/a\\b');
  assert.deepEqual(runtime.paths.ancestors(runtime.paths.leaseKey('/a\\b/c')), ['', '/a\\b']);
});

// --- Blank text and whitespace -------------------------------------------------------------------------------------

test('blank text is what the whitespace set of the previous runtime empties: U+0085, U+001C, U+001F and U+2028 are blank, U+FEFF and U+200B are not', t => {
  const f = fixture(t), sender = f.join('sender'), receiver = f.join('receiver');
  for (const body of ['\x85', '\x1c', '\x1f', '\u2028', ' \t\n']) assert.match(f.fails('send_message', { to: receiver, body }, sender) ?? '', /blank|Invalid/, JSON.stringify(body));
  for (const body of ['\ufeff', '\u200b', 'a']) assert.equal(f.fails('send_message', { to: receiver, body }, sender), null, JSON.stringify(body));
});

test('reasoning is non-blank by the previous runtime \\S, so U+FEFF counts as a character and U+0085 does not', t => {
  const f = fixture(t), sender = f.join('sender'), receiver = f.join('receiver');
  const send = (reasoning: string) => f.fails('send_message', { to: receiver, body: 'x', reasoning }, sender);
  assert.equal(send('\ufeff'), null);
  for (const reasoning of ['\x85', '\x1c', '\u2003']) assert.match(send(reasoning) ?? '', /Invalid/, JSON.stringify(reasoning));
});

test('catalog patterns read \\s and \\S with the previous runtime whitespace set, and refuse what cannot be rewritten', () => {
  const { compilePattern } = runtime.validation;
  assert.equal(compilePattern('\\S').test('\ufeff'), true);
  assert.equal(compilePattern('\\S').test('\x85\x1c \u3000'), false);
  assert.equal(compilePattern('^a\\sb$').test('a\x85b'), true);
  assert.equal(compilePattern('[\\s,]+').test('\x1c'), true);
  assert.equal(compilePattern('^\\\\s[\\]]$').test('\\s]'), true, 'an escaped backslash is not a whitespace escape');
  assert.throws(() => compilePattern('[\\S]'), /Unsupported catalog pattern/);
});

// --- Queued lease targets ------------------------------------------------------------------------------------------

test('queued lease targets are stored ASCII-escaped, and a retried wait finds its row', t => {
  const f = fixture(t), holder = f.join('holder'), waiter = f.join('waiter');
  assert.equal(f.call('lock', { path: 'café.txt', kind: 'file' }, holder).ok, true);
  const first = f.call('lock', { path: 'café.txt', kind: 'file', wait: true }, waiter);
  assert.equal(first.queued, true);
  const [{ targets }]: JsonRecord[] = f.db().prepare('SELECT targets FROM lease_waits').all();
  assert.match(targets, /^[\x00-\x7f]*$/);
  assert.ok(targets.includes('caf\\u00e9.txt'));
  assert.equal(f.call('lock', { path: 'café.txt', kind: 'file', wait: true }, waiter).wait.id, first.wait.id);
});

// --- Host hook input -----------------------------------------------------------------------------------------------

type Fixture = ReturnType<typeof fixture>;
const hookInput = (f: Fixture, extra: Record<string, unknown> = {}) => JSON.stringify({ hook_event_name: 'SessionStart', session_id: 'host-fixture', cwd: f.workspace, ...extra });
const hookArgs = (f: Fixture) => ['host-hook', '--vendor', 'claude', ...f.flags];
const runHook = (f: Fixture, input: string) => {
  const { stdout, stderr } = spawnSync(binary, hookArgs(f), { input, encoding: 'utf8' });
  return { stdout, stderr };
};

test('a host hook reads JSON input that begins with a byte order mark', t => {
  const f = fixture(t);
  const stdout = execFileSync(binary, hookArgs(f), { input: Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from(hookInput(f))]), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] });
  const [{ id }]: JsonRecord[] = f.db().prepare('SELECT id FROM sessions').all();
  assert.ok(JSON.parse(stdout).hookSpecificOutput.additionalContext.includes(id));
});

test('a host hook refuses a context_generation that is not a string instead of coercing it', t => {
  const f = fixture(t);
  for (const value of [7, null, ['x'], false]) {
    const { stdout, stderr } = runHook(f, hookInput(f, { context_generation: value }));
    assert.deepEqual(JSON.parse(stdout), {}, JSON.stringify(value));
    assert.match(stderr, /Missing string: id/, JSON.stringify(value));
  }
  assert.equal(only(f.db().prepare('SELECT count(*) n FROM records WHERE type=?').get('host.context')).n, 0);
  assert.equal(JSON.parse(runHook(f, hookInput(f, { context_generation: 'gen-1' })).stdout).hookSpecificOutput.hookEventName, 'SessionStart', 'a valid string generation is accepted as a fresh binding');
  assert.equal(only(f.db().prepare('SELECT count(*) n FROM records WHERE type=?').get('host.context')).n, 1);
});

// --- MCP and vendor frames -----------------------------------------------------------------------------------------

const mcp = (f: Fixture, session: string, frames: string[]): JsonRecord[] => execFileSync(binary, ['mcp', ...f.flags, '--session', session], { input: frames.join('\n') + '\n', encoding: 'utf8' }).trim().split('\n').map((line): JsonRecord => JSON.parse(line));

test('an integer request id beyond 2^53 is refused, not echoed as a different id', t => {
  const f = fixture(t), session = f.join('agent');
  const rows = mcp(f, session, [
    '{"jsonrpc":"2.0","id":9007199254740993,"method":"ping"}',
    '{"jsonrpc":"2.0","id":9007199254740991,"method":"ping"}',
    '{"jsonrpc":"2.0","id":1.5,"method":"ping"}',
    '{"jsonrpc":"2.0","id":"9007199254740993","method":"ping"}',
  ]);
  assert.equal(rows[0].id, null);
  assert.equal(rows[0].error.code, -32600);
  assert.match(rows[0].error.message, /2\^53/);
  assert.deepEqual(rows.slice(1).map(row => row.id), [9007199254740991, 1.5, '9007199254740993']);
  assert.ok(rows.slice(1).every(row => 'result' in row));
});

test('tools/call with arguments:null is an invalid tool call, not an empty argument object', t => {
  const f = fixture(t), session = f.join('agent');
  const rows = mcp(f, session, [
    '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"peers","arguments":{}}}',
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"peers","arguments":null}}',
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"peers"}}',
  ]);
  assert.equal(rows[0].result.isError, undefined);
  assert.equal(rows[1].result.isError, true);
  assert.equal(rows[2].result.isError, undefined, 'omitted arguments still default to {}');
});

test('an MCP tool defect names its type like the CLI, while expected refusals keep their plain message', t => {
  const f = fixture(t), session = f.join('agent');
  const deep = '['.repeat(30000) + ']'.repeat(30000);
  const rows = mcp(f, session, [
    `{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"peers","arguments":{"x":${deep}}}}`,
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nope","arguments":{}}}',
  ]);
  assert.equal(rows[0].result.isError, true);
  assert.match(rows[0].result.content[0].text, /^Internal error \(RangeError\): /);
  assert.equal(rows[1].result.content[0].text, 'Unknown tool: nope');
});

test('a vendor frame reads one leading byte order mark but refuses invalid UTF-8', { skip: process.platform === 'win32' }, async t => {
  const directory = tempWorkspace(t, 'communication-edge-wire-', { real: true });
  const script = join(directory, 'vendor.sh');
  writeFileSync(script, `#!/bin/sh\nprintf '\\357\\273\\277{"a":1}\\n{"b":"\\377"}\\n'\nsleep 5\n`, { mode: 0o755 });
  const host = new runtime.wire.Wire(script, [], directory, [], null);
  t.after(() => host.close());
  assert.deepEqual(await host.receive(5000), { a: 1 });
  await assert.rejects(host.receive(5000), TypeError);
});

test('record search words end at the previous runtime whitespace', t => {
  const f = fixture(t), author = f.join('author'), reader = f.join('reader');
  const memory = f.call('record', { type: 'memory', data: { content: 'Remember unicode paths' } }, author);
  const words = (separator: string) => f.call('fetch', { search: `paths${separator}unicode` }, reader).items.map((item: JsonRecord) => item.recordId);
  assert.deepEqual(words(' '), [memory.recordId]);
  for (const separator of ['\x1c', '\x85']) assert.deepEqual(words(separator), [memory.recordId], JSON.stringify(separator));
});

// --- Search words, UUIDs and cursors -------------------------------------------------------------------------------

test('UUID acceptance removes every urn: and uuid: marker, as the previous runtime did', () => {
  const hex = '0123456789abcdef0123456789abcdef';
  assert.equal(runtime.validation.isUuid(hex), true);
  assert.equal(runtime.validation.isUuid(`urn:uuid:{${hex}}`), true);
  assert.equal(runtime.validation.isUuid(`urn:urn:${hex}`), true);
  assert.equal(runtime.validation.isUuid(`${hex}00`), false);
  assert.equal(runtime.validation.isUuid('urn:uuid:' + hex.slice(2)), false);
});

test('a view cursor may use the standard base64 alphabet, as the previous runtime decoding accepted', async t => {
  const f = storeFixture(t);
  const sender = await f.join('sender'), receiver = await f.join('receiver');
  for (const body of ['one', 'two', 'three']) await f.call(sender, 'send_message', { to: receiver, body });
  const cursor = { entity: 'message', id: 3, filters: {}, pad: '??????' };
  const standard = Buffer.from(JSON.stringify(cursor)).toString('base64');
  assert.match(standard, /[+/]/, 'the fixture must use a standard-only character');
  const page = runtime.viewData.page(f.store, 'message', standard.replace(/=+$/, ''));
  assert.deepEqual(page.items.map((item: JsonRecord) => item.id), [2, 1]);
  await assert.rejects(async () => runtime.viewData.page(f.store, 'message', Buffer.from([0x7b, 0xff, 0x7d]).toString('base64url')));
});
