import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { execFile, execFileSync, spawn } from './helpers.ts';
import { promisify } from 'node:util';
import { rmSync, mkdirSync, readFileSync, readdirSync, writeFileSync, symlinkSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { registerBoundTools } from '../dist/hosts/pi-extension.js';
import { binary, isChildFailure, only, tempWorkspace, waitFor, withReasoning } from './helpers.ts';
import { boundTool, type BoundTool } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';

function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-documents-', { real: true });
  const database = join(workspace, 'audit.sqlite');
  const args = (command: string, session?: string) => [command, '-', '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])];
  const call = (command: string, input: Record<string, unknown> = {}, session?: string): JsonRecord => JSON.parse(execFileSync(binary, args(command, session), {
    input: JSON.stringify(withReasoning(command,input)), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], maxBuffer: 4 * 1024 * 1024, timeout: 30_000,
  }));
  const callAsync = async (command: string, input: Record<string, unknown> = {}, session?: string): Promise<JsonRecord> => {
    const child = promisify(execFile)(binary, args(command, session), {
      encoding: 'utf8', maxBuffer: 4 * 1024 * 1024, timeout: 30_000,
    });
    const { stdin } = child.child;
    assert.ok(stdin);
    stdin.end(JSON.stringify(withReasoning(command, input)));
    return JSON.parse((await child).stdout);
  };
  const a = call('join', { name: 'author', vendor: 'any-vendor' });
  const b = call('join', { name: 'reader', vendor: 'no-sdk' });
  return { workspace, database, args, call, callAsync, a, b };
}

test('document intent is required before filesystem writes and immutable in the audit', t => {
  const f = fixture(t), input = { name: 'intent.md', content: 'Check callers before deleting src/legacy.rs.' };
  for (const reasoning of [undefined, null, '', ' \n\t', '\u2003', 'x'.repeat(513), '🙂'.repeat(129)]) {
    assert.throws(() => f.call('share_document', { ...input, reasoning }, f.a.id), /Invalid input|Invalid reasoning/);
  }
  assert.throws(() => readFileSync(join(f.workspace, '.octocode/communication', input.name)), /ENOENT/);
  const reasoning = '🙂'.repeat(128); // Exactly 512 UTF-8 bytes, independent of character count.
  const result = f.call('share_document', { ...input, reasoning }, f.a.id);
  assert.equal(result.document.reasoning, reasoning);
  assert.deepEqual(f.call('share_document', { ...input, reasoning }, f.a.id), { created: false, document: result.document });
  assert.throws(() => f.call('share_document', { ...input, reasoning: 'Changed intent' }, f.a.id), /immutable/);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const rows = db.prepare("SELECT data FROM records WHERE type='document'").all();
  assert.equal(rows.length, 1);
  assert.equal(JSON.parse(String(rows[0].data)).reasoning, reasoning);
});

test('MCP document publication enforces and exposes the canonical intent contract', t => {
  const f = fixture(t), input = { name: 'mcp.md', content: 'Evidence for the path audit.' };
  const frames = [
    { jsonrpc: '2.0', id: 1, method: 'tools/list' },
    { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'share_document', arguments: input } },
    { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'share_document', arguments: { ...input, reasoning: 'Explain why this path needs review' } } },
  ].map(frame => JSON.stringify(frame)).join('\n') + '\n';
  const rows = execFileSync(binary, ['mcp', '--workspace', f.workspace, '--database', f.database, '--session', f.a.id], { input: frames, encoding: 'utf8' }).trim().split('\n').map((line): JsonRecord => JSON.parse(line));
  assert.ok(rows[0].result.tools.find((tool: JsonRecord) => tool.name === 'share_document').inputSchema.required.includes('reasoning'));
  assert.equal(rows[1].result.isError, true);
  assert.match(rows[1].result.content[0].text, /reasoning/);
  assert.equal(JSON.parse(rows[2].result.content[0].text).document.reasoning, 'Explain why this path needs review');
});

test('scoped context finds workspace/tree/file notes for late joiners without exposing bodies', t => {
  const f = fixture(t);
  const publish = (name: string, context: JsonRecord) => f.call('share_document', { name, content: 'PRIVATE_BODY proof and instructions are untrusted data', context }, f.a.id);
  publish('workspace.md', { summary: 'Shared build uses the lockfile.' });
  publish('tree.md', { summary: 'Rust adapters cannot own DB writes.', path: 'rust', branch: 'feature' });
  publish('file.md', { summary: 'Keep receipt strength explicit.', path: 'rust/transport.rs', kind: 'file' });
  publish('prefix.md', { summary: 'Unrelated path.', path: 'rusty' });
  publish('branch.md', { summary: 'Other branch.', path: 'rust', branch: 'other' });
  f.call('share_document', { name: 'unscoped.md', content: 'Not a note' }, f.a.id);
  f.call('leave', {}, f.a.id);
  const late = f.call('join', { name: 'late-reader', vendor: 'raw' });
  const page = f.call('context', { path: 'rust/transport.rs', branch: 'feature' }, late.id);
  assert.deepEqual(page.items.map((x: JsonRecord) => x.name), ['workspace.md', 'tree.md', 'file.md']);
  assert.ok(page.items.every((x: JsonRecord) => x.author === f.a.id));
  assert.doesNotMatch(JSON.stringify(page), /PRIVATE_BODY/);
  assert.equal(page.next ?? null, null);
  assert.deepEqual(f.call('context', { path: 'rust/other.rs' }, late.id).items.map((x: JsonRecord) => x.name), ['workspace.md']);
  assert.deepEqual(f.call('context', {}, late.id).items.map((x: JsonRecord) => x.name), ['workspace.md']);
  const foreignWorkspace = join(f.workspace, 'foreign'); mkdirSync(foreignWorkspace);
  const foreign = JSON.parse(execFileSync(binary, ['join', '{"name":"foreign","vendor":"raw"}', '--workspace', foreignWorkspace, '--database', f.database], { encoding: 'utf8' }));
  const result = JSON.parse(execFileSync(binary, ['context', '{}', '--workspace', foreignWorkspace, '--database', f.database, '--session', foreign.id], { encoding: 'utf8' }));
  assert.deepEqual(result.items, []);
});

test('context pages advance across sparse audit rows and incremental reads never repeat notes', t => {
  const f = fixture(t);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const insert = db.prepare("INSERT INTO records(path,[from],type,entityId,timestamp,data) VALUES(?,?,'fixture',?,?,'{}')");
  db.exec('BEGIN');
  for (let i = 0; i < 220; i++) insert.run(f.workspace, f.a.id, String(i), Date.now());
  db.exec('COMMIT');
  for (let i = 0; i < 3; i++) f.call('share_document', { name: `note-${i}.md`, content: `proof ${i}`, context: { summary: `Gotcha ${i}` } }, f.a.id);
  let page = f.call('context', { limit: 1 }, f.b.id);
  // The document registry skips unrelated audit history: the first page holds the first note.
  assert.deepEqual(page.items.map((x: JsonRecord) => x.name), ['note-0.md']); assert.equal(page.next?.command, 'context'); assert.equal(page.scanned, 1);
  f.call('share_document', { name: 'concurrent.md', content: 'later snapshot', context: { summary: 'Published during pagination' } }, f.a.id);
  const names = page.items.map((x: JsonRecord) => x.name);
  while (page.next) { page = f.call(page.next.command, page.next.input, f.b.id); names.push(...page.items.map((x: JsonRecord) => x.name)); }
  assert.deepEqual(names, ['note-0.md', 'note-1.md', 'note-2.md']);
  page = f.call('context', { after: page.cursor }, f.b.id);
  assert.deepEqual(page.items.map((x: JsonRecord) => x.name), ['concurrent.md']);
  assert.deepEqual(f.call('context', { after: page.cursor }, f.b.id).items, []);
  f.call('share_document', { name: 'later.md', content: 'new proof', context: { summary: 'New fact' } }, f.a.id);
  assert.deepEqual(f.call('context', { after: page.cursor }, f.b.id).items.map((x: JsonRecord) => x.name), ['later.md']);
});

test('context expiry hides discovery, preserves evidence, and identical retries do not renew it', async t => {
  const f = fixture(t), input = { name: 'expires.md', content: 'proof', context: { summary: 'Temporary fact', ttlMs: 1000 } };
  const first = f.call('share_document', input, f.a.id);
  assert.equal(f.call('context', {}, f.b.id).items.length, 1);
  await waitFor(() => f.call('context', {}, f.b.id).items.length === 0, { message: 'The context note did not expire after its ttl' });
  assert.deepEqual(f.call('context', {}, f.b.id).items, []);
  assert.deepEqual(f.call('share_document', input, f.a.id), { created: false, document: first.document });
  assert.equal(f.call('read_document', { name: input.name }, f.b.id).content, 'proof');
  assert.throws(() => f.call('share_document', { ...input, context: { ...input.context, summary: 'Changed' } }, f.a.id), /immutable/);
});

test('context validates bounds and containment and preserves canonical path aliases', t => {
  const f = fixture(t), input = { name: 'valid-note.md', content: 'proof', context: { summary: 'fact' } };
  for (const [context, refused] of [[{ summary: ' ' }, /Context summary must explain a useful fact/], [{ summary: 'x'.repeat(321) }, /summary has invalid string length/], [{ summary: 'x', ttlMs: 0 }, /ttlMs is outside numeric bounds/], [{ summary: 'x', ttlMs: 604800001 }, /ttlMs is outside numeric bounds/], [{ summary: 'x', path: '../escape' }, /Context path must stay inside/]]) {
    assert.throws(() => f.call('share_document', { ...input, context }, f.a.id), refused);
  }
  assert.throws(() => f.call('context', { path: '../escape' }, f.b.id), /inside/);
  assert.throws(() => f.call('context', { limit: 21 }, f.b.id), /limit is outside numeric bounds/);
  assert.throws(() => f.call('context', { path: '.', after: 10, through: 1 }, f.b.id), /through must be at least after/);
  assert.throws(() => f.call('context', {}, 'unknown'), /Unknown or expired session/);
  mkdirSync(join(f.workspace, 'src'));
  if (process.platform !== 'win32') {
    symlinkSync('src', join(f.workspace, 'alias'));
    f.call('share_document', { ...input, context: { summary: 'Alias fact', path: 'alias' } }, f.a.id);
    assert.equal(f.call('context', { path: 'src/new.rs' }, f.b.id).items[0].context.path, 'src');
  }
});

test('a revision that supersedes a published document records the ADR link; unknown or self targets are refused', t => {
  const f = fixture(t);
  f.call('share_document', { name: 'adr-1.md', content: 'Use SQLite' }, f.a.id);
  assert.throws(() => f.call('share_document', { name: 'adr-2.md', content: 'Use Postgres', supersedes: 'adr-0.md' }, f.a.id), /supersedes must name a published document/);
  assert.throws(() => f.call('share_document', { name: 'adr-3.md', content: 'Self', supersedes: 'adr-3.md' }, f.a.id), /cannot supersede itself/);
  const revision = f.call('share_document', { name: 'adr-2.md', content: 'Use Postgres', supersedes: 'adr-1.md' }, f.a.id);
  assert.equal(revision.document.supersedes, 'adr-1.md');
  assert.equal(f.call('share_document', { name: 'adr-2.md', content: 'Use Postgres', supersedes: 'adr-1.md' }, f.a.id).created, false);
  assert.throws(() => f.call('share_document', { name: 'adr-2.md', content: 'Use Postgres' }, f.a.id), /immutable/);
});

test('large document stays on disk once, compact messages reference it, audit retains attribution', async t => {
  const f = fixture(t), content = 'large shared evidence\n'.repeat(20000), name = 'research.md';
  const result = await f.callAsync('share_document', { name, content }, f.a.id);
  assert.equal(result.created, true);
  assert.equal(result.document.author, f.a.id);
  assert.equal(result.document.bytes, Buffer.byteLength(content));
  assert.equal(readFileSync(join(f.workspace, result.document.path), 'utf8'), content);
  assert.deepEqual(await f.callAsync('share_document', { name, content }, f.b.id), { created: false, document: result.document });
  await assert.rejects(f.callAsync('share_document', { name, content: 'replacement' }, f.a.id), /immutable/);
  const sent = f.call('send_message', { to: f.b.id, body: `Review ${result.document.path}`, key: name }, f.a.id);
  assert.ok(sent.id > 0);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const rows = db.prepare("SELECT * FROM records WHERE type='document'").all();
  assert.equal(rows.length, 1);
  assert.equal(rows[0].from, f.a.id);
  assert.deepEqual(JSON.parse(String(rows[0].data)), result.document);
  assert.ok(String(rows[0].data).length < 400);
  assert.ok(Number(only(db.prepare('SELECT length(body) AS n FROM messages').get()).n) < 100);
});

test('document reads use byte paging without repeating or splitting Unicode and detect edits', t => {
  const f = fixture(t), name = 'unicode.md', content = 'α🙂a'.repeat(25);
  f.call('share_document', { name, content }, f.a.id);
  let input = { name, limit: 4 }, output = '', pages = 0;
  while (input) {
    const page = f.call('read_document', input, f.b.id);
    assert.equal(page.offset, Buffer.byteLength(output));
    assert.ok(Buffer.byteLength(page.content) <= 4);
    output += page.content; input = page.next?.input;
    assert.ok(++pages < 100);
  }
  assert.equal(output, content);
  assert.throws(() => f.call('read_document', { name, offset: 1 }, f.b.id), /boundary/);
  assert.throws(() => f.call('read_document', { name, offset: 1000 }, f.b.id), /boundary/);
  assert.equal(f.call('read_document', { name, offset: Buffer.byteLength(content) }, f.b.id).content, '');
  writeFileSync(join(f.workspace, '.octocode/communication', name), 'tampered');
  assert.throws(() => f.call('read_document', { name }, f.b.id), /integrity/);
});

test('missing document names suggest bounded exact names without silently reading a substitute', t => {
  const f = fixture(t);
  for (let i = 0; i < 7; i++) f.call('share_document', { name: `handoff.${i}.md`, content: `Evidence ${i}` }, f.a.id);
  f.call('share_document', { name: 'other.md', content: 'Unrelated' }, f.a.id);
  assert.throws(() => f.call('read_document', { name: 'handoff' }, f.b.id), error => {
    assert.ok(isChildFailure(error));
    const message = String(error.stderr);
    assert.match(message, /exact published document.name, including its extension/);
    assert.match(message, /handoff\.0\.md/);
    assert.match(message, /handoff\.4\.md/);
    assert.doesNotMatch(message, /handoff\.5\.md|other\.md|Evidence/);
    return true;
  });
  assert.equal(f.call('read_document', { name: 'handoff.0.md' }, f.b.id).content, 'Evidence 0');
});

/** The CLI's stderr for a command it refused (exit 1); a timeout or crash fails with its cause. */
function refusal(error: unknown): string {
  assert.ok(error instanceof Error, 'expected the command to be refused');
  const status = 'status' in error ? error.status : undefined, code = 'code' in error ? error.code : undefined, signal = 'signal' in error ? error.signal : undefined;
  assert.equal(status ?? code, 1, `expected exit 1, got ${code ?? signal ?? error.message}`);
  assert.ok('stderr' in error);
  return String(error.stderr).trim();
}
const refused = (run: () => unknown) => { try { run(); } catch (error) { return refusal(error); } return refusal(undefined); };
const refusedAsync = async (run: () => Promise<unknown>) => { try { await run(); } catch (error) { return refusal(error); } return refusal(undefined); };
type Fixture = ReturnType<typeof fixture>;
const published = (f: Fixture) => { try { return readdirSync(join(f.workspace, '.octocode/communication')).sort(); } catch (error) { if (error instanceof Error && 'code' in error && error.code === 'ENOENT') return []; throw error; } };
function documentRecords(t: TestContext, f: Fixture) {
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  return db.prepare("SELECT key FROM records WHERE type='document' ORDER BY key").all().map(row => row.key);
}
const unknownSession = /^Unknown or expired session in this workspace\. Refresh peers and copy the exact current ID/;

test('share_document refuses unsafe or noncanonical names before writing anything', t => {
  const f = fixture(t);
  const pattern = 'Invalid input: /name does not match pattern';
  for (const name of ['../secret', '/secret', 'a/b', 'a\\b', '.hidden', '-dash.md', 'UPPER.md', 'space file.md']) {
    assert.equal(refused(() => f.call('share_document', { name, content: 'no' }, f.a.id)), pattern, name);
  }
  assert.equal(refused(() => f.call('share_document', { name: 'a..b', content: 'no' }, f.a.id)),
    "Use a lowercase document filename (letters, digits, dot, dash, underscore), without directories or '..'");
  assert.equal(refused(() => f.call('share_document', { name: 'a'.repeat(129), content: 'no' }, f.a.id)),
    'Invalid input: /name has invalid string length');
  assert.deepEqual(published(f), []);
  assert.deepEqual(documentRecords(t, f), []);
  // The longest valid name is accepted, so the refusals above are the name rules, not a broken command.
  assert.equal(f.call('share_document', { name: 'a'.repeat(128), content: 'yes' }, f.a.id).created, true);
  assert.deepEqual(published(f), ['a'.repeat(128)]);
});

test('share_document accepts exactly 1 MiB of UTF-8 content and refuses one byte more', async t => {
  const f = fixture(t);
  const limit = 1024 * 1024;
  // Large payloads stream over an asynchronous stdin pipe, as the Pi bound tools send them.
  assert.equal(await refusedAsync(() => f.callAsync('share_document', { name: 'over.md', content: 'x'.repeat(limit + 1) }, f.a.id)),
    `Invalid content: ${limit + 1} UTF-8 bytes exceeds 1 MiB`);
  // The bound counts UTF-8 bytes, not characters: 262145 four-byte characters are 4 bytes over.
  assert.equal(await refusedAsync(() => f.callAsync('share_document', { name: 'emoji.md', content: '🙂'.repeat(limit / 4 + 1) }, f.a.id)),
    `Invalid content: ${limit + 4} UTF-8 bytes exceeds 1 MiB`);
  assert.deepEqual(published(f), []);
  assert.deepEqual(documentRecords(t, f), []);
  const content = '🙂'.repeat(limit / 4);
  const { created, document } = await f.callAsync('share_document', { name: 'exact.md', content }, f.a.id);
  assert.equal(created, true);
  assert.equal(document.bytes, limit);
  assert.equal(document.sha256, createHash('sha256').update(content).digest('hex'));
  assert.equal(readFileSync(join(f.workspace, '.octocode/communication/exact.md'), 'utf8'), content);
  assert.deepEqual(documentRecords(t, f), ['exact.md']);
});

test('document commands refuse unknown and foreign sessions and out-of-bounds read windows', t => {
  const f = fixture(t);
  assert.match(refused(() => f.call('share_document', { name: 'unknown.md', content: 'no' }, 'unknown')), unknownSession);
  assert.deepEqual(published(f), []);
  assert.equal(f.call('share_document', { name: 'valid.md', content: 'yes' }, f.a.id).created, true);
  assert.match(refused(() => f.call('read_document', { name: 'valid.md' }, 'unknown')), unknownSession);
  assert.equal(refused(() => f.call('read_document', { name: 'valid.md', limit: 16385 }, f.b.id)),
    'Invalid input: /limit is outside numeric bounds');
  assert.equal(f.call('read_document', { name: 'valid.md', limit: 16384 }, f.b.id).content, 'yes');
  // A session of another workspace on the same database cannot read this workspace's documents.
  const other = join(f.workspace, 'other'); mkdirSync(other);
  const foreign = JSON.parse(execFileSync(binary, ['join', JSON.stringify({ name: 'foreign', vendor: 'generic' }), '--workspace', other, '--database', f.database], { encoding: 'utf8' }));
  assert.match(refused(() => f.call('read_document', { name: 'valid.md' }, foreign.id)), unknownSession);
  assert.deepEqual(documentRecords(t, f), ['valid.md']);
});

test('document directories and files reject symlinks; unregistered files are preserved', { skip: process.platform === 'win32' }, t => {
  const f = fixture(t), outside = join(f.workspace, 'outside'); mkdirSync(outside);
  symlinkSync(outside, join(f.workspace, '.octocode'));
  assert.throws(() => f.call('share_document', { name: 'escape.md', content: 'no' }, f.a.id), /symlink/);
  rmSync(join(f.workspace, '.octocode'));
  mkdirSync(join(f.workspace, '.octocode'));
  symlinkSync(outside, join(f.workspace, '.octocode/communication'));
  assert.throws(() => f.call('share_document', { name: 'escape.md', content: 'no' }, f.a.id), /symlink/);
  rmSync(join(f.workspace, '.octocode/communication'));
  mkdirSync(join(f.workspace, '.octocode/communication'));
  const orphan = join(f.workspace, '.octocode/communication/orphan.md'); writeFileSync(orphan, 'keep');
  assert.throws(() => f.call('share_document', { name: 'orphan.md', content: 'keep' }, f.a.id), /Unregistered/);
  assert.equal(readFileSync(orphan, 'utf8'), 'keep');
  f.call('share_document', { name: 'real.md', content: 'original' }, f.a.id);
  const real = join(f.workspace, '.octocode/communication/real.md'); rmSync(real); symlinkSync(orphan, real);
  assert.throws(() => f.call('read_document', { name: 'real.md' }, f.b.id), /symlink/);
});

test('concurrent authors cannot overwrite each other or duplicate document audit', async t => {
  const f = fixture(t), name = 'shared.md';
  const run = (session: string, content: string) => new Promise<{ code: number | null; stdout: string }>((resolve, reject) => {
    const child = spawn(binary, f.args('share_document', session));
    let stdout = ''; child.stdout.on('data', x => stdout += x);
    child.on('error', reject); child.on('close', code => resolve({ code, stdout }));
    child.stdin.end(JSON.stringify({ name, content, reasoning: 'Verify concurrent publication preserves one author' }));
  });
  const results = await Promise.all([run(f.a.id, 'first'), run(f.b.id, 'second')]);
  assert.equal(results.filter(x => x.code === 0).length, 1);
  const winning = results.find(x => x.code === 0);
  assert.ok(winning);
  const winner = JSON.parse(winning.stdout).document;
  const result = f.call('read_document', { name }, f.a.id);
  assert.equal(result.document.author, winner.author);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.equal(only(db.prepare("SELECT count(*) AS n FROM records WHERE type='document'").get()).n, 1);
});

test('Pi bound tools send large document JSON over stdin and retain cancellation', async t => {
  const f = fixture(t), tools = JSON.parse(execFileSync(binary, ['schema'], { encoding: 'utf8', maxBuffer: 1024 * 1024 })).tools;
  const registered = new Map<string, BoundTool>();
  const registeredTool = (name: string) => {
    const tool = registered.get(name);
    assert.ok(tool, `${name} is registered`);
    return tool;
  };
  const binding = { binary, workspace: f.workspace, database: f.database, session: f.a.id, tools };
  registerBoundTools({ registerTool: tool => registered.set(tool.name, boundTool(tool)) }, binding);
  const content = 'quoted "words" \\ slash\n🙂\t\u0000'.repeat(20000), name = 'pi-large.md';
  assert.ok(Buffer.byteLength(content) > 400000);
  await assert.rejects(registeredTool('share_document').execute('missing-intent', { name, content }), /reasoning/);
  const shared = await registeredTool('share_document').execute('publish', { name, content, reasoning: 'Share large Unicode evidence through the Pi bridge' });
  assert.equal(shared.details.created, true);
  assert.equal(shared.details.document.bytes, Buffer.byteLength(content));
  assert.equal(readFileSync(join(f.workspace, shared.details.document.path), 'utf8'), content);
  const page = await registeredTool('read_document').execute('read', { name, limit: 100 });
  assert.ok(content.startsWith(page.details.content));
  assert.ok(page.details.next.input.offset > 0);
  const controller = new AbortController(); controller.abort();
  await assert.rejects(registeredTool('share_document').execute('aborted', { name: 'aborted.md', content }, controller.signal), /abort/i);
  // A child rejecting its arguments before consuming stdin must reject, not emit unhandled EPIPE.
  const earlyExit = new Map<string, BoundTool>();
  registerBoundTools({ registerTool: tool => earlyExit.set(tool.name, boundTool(tool)) }, { ...binding, binary: process.execPath });
  const earlyExitTool = earlyExit.get('share_document');
  assert.ok(earlyExitTool, 'share_document is registered');
  await assert.rejects(earlyExitTool.execute('early-exit', { name, content }));
});


test('targeted document reads cross scan boundaries and reject off-page same-size tampering', t => {
  const f = fixture(t), name = 'window.md';
  const content = 'a'.repeat(8191) + '🙂β' + 'z'.repeat(1024 * 1024 - 8197);
  assert.equal(Buffer.byteLength(content), 1024 * 1024);
  f.call('share_document', { name, content }, f.a.id);
  const page = f.call('read_document', { name, offset: 8191, limit: 5 }, f.b.id);
  assert.equal(page.content, '🙂');
  assert.deepEqual(page.next, { command: 'read_document', input: { name, offset: 8195, limit: 5 } });
  assert.equal(f.call('read_document', page.next.input, f.b.id).content, 'βzzz');
  assert.throws(() => f.call('read_document', { name, offset: 8192, limit: 4 }, f.b.id), /boundary/);
  const path = join(f.workspace, '.octocode/communication', name);
  const modified = Buffer.from(content); modified[modified.length - 1] = 120;
  writeFileSync(path, modified);
  // Both a repeated earlier page and an empty EOF page must revalidate the whole file.
  assert.throws(() => f.call('read_document', { name, offset: 8191, limit: 5 }, f.b.id), /integrity/);
  assert.throws(() => f.call('read_document', { name, offset: modified.length }, f.b.id), /integrity/);
  writeFileSync(path, content);
  assert.equal(f.call('read_document', { name, offset: 8191, limit: 5 }, f.b.id).content, '🙂');
});
