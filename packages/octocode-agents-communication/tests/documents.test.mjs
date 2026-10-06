import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFile, execFileSync, spawn } from './helpers.mjs';
import { promisify } from 'node:util';
import { rmSync, mkdirSync, readFileSync, writeFileSync, symlinkSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { registerBoundTools } from '../scripts/pi-extension.mjs';
import { binary, tempWorkspace, withReasoning } from './helpers.mjs';

function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-documents-', { real: true });
  const database = join(workspace, 'audit.sqlite');
  const args = (command, session) => [command, '-', '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])];
  const call = (command, input = {}, session) => JSON.parse(execFileSync(binary, args(command, session), {
    input: JSON.stringify(withReasoning(command,input)), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], maxBuffer: 4 * 1024 * 1024, timeout: 30_000,
  }));
  const callAsync = async (command, input = {}, session) => {
    const child = promisify(execFile)(binary, args(command, session), {
      encoding: 'utf8', maxBuffer: 4 * 1024 * 1024, timeout: 30_000,
    });
    child.child.stdin.end(JSON.stringify(withReasoning(command, input)));
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
  assert.equal(JSON.parse(rows[0].data).reasoning, reasoning);
  // Old evidence is readable but a retry cannot invent its previously unrecorded intent.
  const historical = { ...result.document, name: 'historical.md', path: '.octocode/communication/historical.md' };
  delete historical.reasoning;
  writeFileSync(join(f.workspace, historical.path), input.content);
  db.prepare("INSERT INTO records(path,[from],type,entityId,timestamp,data,key) VALUES(?,?,'document',?,?,?,?)").run(f.workspace, f.a.id, historical.name, Date.now(), JSON.stringify(historical), historical.name);
  assert.equal(f.call('read_document', { name: historical.name }, f.b.id).content, input.content);
  assert.throws(() => f.call('share_document', { ...input, name: historical.name, reasoning }, f.a.id), /immutable/);
});

test('MCP document publication enforces and exposes the canonical intent contract', t => {
  const f = fixture(t), input = { name: 'mcp.md', content: 'Evidence for the path audit.' };
  const frames = [
    { jsonrpc: '2.0', id: 1, method: 'tools/list' },
    { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'share_document', arguments: input } },
    { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'share_document', arguments: { ...input, reasoning: 'Explain why this path needs review' } } },
  ].map(JSON.stringify).join('\n') + '\n';
  const rows = execFileSync(binary, ['mcp', '--workspace', f.workspace, '--database', f.database, '--session', f.a.id], { input: frames, encoding: 'utf8' }).trim().split('\n').map(JSON.parse);
  assert.ok(rows[0].result.tools.find(tool => tool.name === 'share_document').inputSchema.required.includes('reasoning'));
  assert.equal(rows[1].result.isError, true);
  assert.match(rows[1].result.content[0].text, /reasoning/);
  assert.equal(JSON.parse(rows[2].result.content[0].text).document.reasoning, 'Explain why this path needs review');
});

test('scoped context finds workspace/tree/file notes for late joiners without exposing bodies', t => {
  const f = fixture(t);
  const publish = (name, context) => f.call('share_document', { name, content: 'PRIVATE_BODY proof and instructions are untrusted data', context }, f.a.id);
  publish('workspace.md', { summary: 'Shared build uses the lockfile.' });
  publish('tree.md', { summary: 'Rust adapters cannot own DB writes.', path: 'rust', branch: 'feature' });
  publish('file.md', { summary: 'Keep receipt strength explicit.', path: 'rust/transport.rs', kind: 'file' });
  publish('prefix.md', { summary: 'Unrelated path.', path: 'rusty' });
  publish('branch.md', { summary: 'Other branch.', path: 'rust', branch: 'other' });
  f.call('share_document', { name: 'unscoped.md', content: 'Not a note' }, f.a.id);
  f.call('leave', {}, f.a.id);
  const late = f.call('join', { name: 'late-reader', vendor: 'raw' });
  const page = f.call('context', { path: 'rust/transport.rs', branch: 'feature' }, late.id);
  assert.deepEqual(page.items.map(x => x.name), ['workspace.md', 'tree.md', 'file.md']);
  assert.ok(page.items.every(x => x.author === f.a.id));
  assert.doesNotMatch(JSON.stringify(page), /PRIVATE_BODY/);
  assert.equal(page.next ?? null, null);
  assert.deepEqual(f.call('context', { path: 'rust/other.rs' }, late.id).items.map(x => x.name), ['workspace.md']);
  assert.deepEqual(f.call('context', {}, late.id).items.map(x => x.name), ['workspace.md']);
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
  assert.deepEqual(page.items.map(x => x.name), ['note-0.md']); assert.ok(page.next); assert.equal(page.scanned, 1);
  f.call('share_document', { name: 'concurrent.md', content: 'later snapshot', context: { summary: 'Published during pagination' } }, f.a.id);
  const names = page.items.map(x => x.name);
  while (page.next) { page = f.call(page.next.command, page.next.input, f.b.id); names.push(...page.items.map(x => x.name)); }
  assert.deepEqual(names, ['note-0.md', 'note-1.md', 'note-2.md']);
  page = f.call('context', { after: page.cursor }, f.b.id);
  assert.deepEqual(page.items.map(x => x.name), ['concurrent.md']);
  assert.deepEqual(f.call('context', { after: page.cursor }, f.b.id).items, []);
  f.call('share_document', { name: 'later.md', content: 'new proof', context: { summary: 'New fact' } }, f.a.id);
  assert.deepEqual(f.call('context', { after: page.cursor }, f.b.id).items.map(x => x.name), ['later.md']);
});

test('context expiry hides discovery, preserves evidence, and identical retries do not renew it', async t => {
  const f = fixture(t), input = { name: 'expires.md', content: 'proof', context: { summary: 'Temporary fact', ttlMs: 1000 } };
  const first = f.call('share_document', input, f.a.id);
  assert.equal(f.call('context', {}, f.b.id).items.length, 1);
  await new Promise(resolve => setTimeout(resolve, 1100));
  assert.deepEqual(f.call('context', {}, f.b.id).items, []);
  assert.deepEqual(f.call('share_document', input, f.a.id), { created: false, document: first.document });
  assert.equal(f.call('read_document', { name: input.name }, f.b.id).content, 'proof');
  assert.throws(() => f.call('share_document', { ...input, context: { ...input.context, summary: 'Changed' } }, f.a.id), /immutable/);
});

test('context validates bounds and containment and preserves canonical path aliases', t => {
  const f = fixture(t), input = { name: 'valid-note.md', content: 'proof', context: { summary: 'fact' } };
  for (const context of [{ summary: ' ' }, { summary: 'x'.repeat(321) }, { summary: 'x', ttlMs: 0 }, { summary: 'x', ttlMs: 604800001 }, { summary: 'x', path: '../escape' }]) {
    assert.throws(() => f.call('share_document', { ...input, context }, f.a.id));
  }
  assert.throws(() => f.call('context', { path: '../escape' }, f.b.id), /inside/);
  assert.throws(() => f.call('context', { limit: 21 }, f.b.id));
  assert.throws(() => f.call('context', {}, 'unknown'));
  mkdirSync(join(f.workspace, 'src'));
  if (process.platform !== 'win32') {
    symlinkSync('src', join(f.workspace, 'alias'));
    f.call('share_document', { ...input, context: { summary: 'Alias fact', path: 'alias' } }, f.a.id);
    assert.equal(f.call('context', { path: 'src/new.rs' }, f.b.id).items[0].context.path, 'src');
  }
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
  assert.deepEqual(JSON.parse(rows[0].data), result.document);
  assert.ok(rows[0].data.length < 400);
  assert.ok(db.prepare('SELECT length(body) AS n FROM messages').get().n < 100);
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
    const message = error.stderr.toString();
    assert.match(message, /exact published document.name, including its extension/);
    assert.match(message, /handoff\.0\.md/);
    assert.match(message, /handoff\.4\.md/);
    assert.doesNotMatch(message, /handoff\.5\.md|other\.md|Evidence/);
    return true;
  });
  assert.equal(f.call('read_document', { name: 'handoff.0.md' }, f.b.id).content, 'Evidence 0');
});

test('document bounds, identity and path checks reject invalid or foreign access', t => {
  const f = fixture(t);
  for (const name of ['../secret', '/secret', 'a/b', 'a\\b', '.hidden', 'a..b', 'UPPER.md', 'space file.md']) {
    assert.throws(() => f.call('share_document', { name, content: 'no' }, f.a.id));
  }
  assert.throws(() => f.call('share_document', { name: 'large.md', content: '🙂'.repeat(262145) }, f.a.id), /1 MiB/);
  assert.throws(() => f.call('share_document', { name: 'unknown.md', content: 'no' }, 'unknown'));
  f.call('share_document', { name: 'valid.md', content: 'yes' }, f.a.id);
  assert.throws(() => f.call('read_document', { name: 'valid.md' }, 'unknown'));
  assert.throws(() => f.call('read_document', { name: 'valid.md', limit: 16385 }, f.b.id));
  const other = join(f.workspace, 'other'); mkdirSync(other);
  const foreign = JSON.parse(execFileSync(binary, ['join', JSON.stringify({ name: 'foreign', vendor: 'generic' }), '--workspace', other, '--database', f.database], { encoding: 'utf8' }));
  assert.throws(() => f.call('read_document', { name: 'valid.md' }, foreign.id));
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
  const run = (session, content) => new Promise((resolve, reject) => {
    const child = spawn(binary, f.args('share_document', session));
    let stdout = ''; child.stdout.on('data', x => stdout += x);
    child.on('error', reject); child.on('close', code => resolve({ code, stdout }));
    child.stdin.end(JSON.stringify({ name, content, reasoning: 'Verify concurrent publication preserves one author' }));
  });
  const results = await Promise.all([run(f.a.id, 'first'), run(f.b.id, 'second')]);
  assert.equal(results.filter(x => x.code === 0).length, 1);
  const winner = JSON.parse(results.find(x => x.code === 0).stdout).document;
  const result = f.call('read_document', { name }, f.a.id);
  assert.equal(result.document.author, winner.author);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.equal(db.prepare("SELECT count(*) AS n FROM records WHERE type='document'").get().n, 1);
});

test('Pi bound tools send large document JSON over stdin and retain cancellation', async t => {
  const f = fixture(t), tools = JSON.parse(execFileSync(binary, ['schema'], { encoding: 'utf8', maxBuffer: 1024 * 1024 })).tools;
  const registered = new Map();
  const binding = { binary, workspace: f.workspace, database: f.database, session: f.a.id, tools };
  registerBoundTools({ registerTool: tool => registered.set(tool.name, tool) }, binding);
  const content = 'quoted "words" \\ slash\n🙂\t\u0000'.repeat(20000), name = 'pi-large.md';
  assert.ok(Buffer.byteLength(content) > 400000);
  await assert.rejects(registered.get('share_document').execute('missing-intent', { name, content }), /reasoning/);
  const shared = await registered.get('share_document').execute('publish', { name, content, reasoning: 'Share large Unicode evidence through the Pi bridge' });
  assert.equal(shared.details.created, true);
  assert.equal(shared.details.document.bytes, Buffer.byteLength(content));
  assert.equal(readFileSync(join(f.workspace, shared.details.document.path), 'utf8'), content);
  const page = await registered.get('read_document').execute('read', { name, limit: 100 });
  assert.ok(content.startsWith(page.details.content));
  assert.ok(page.details.next.input.offset > 0);
  const controller = new AbortController(); controller.abort();
  await assert.rejects(registered.get('share_document').execute('aborted', { name: 'aborted.md', content }, controller.signal), /abort/i);
  // A child rejecting its arguments before consuming stdin must reject, not emit unhandled EPIPE.
  const earlyExit = new Map();
  registerBoundTools({ registerTool: tool => earlyExit.set(tool.name, tool) }, { ...binding, binary: process.execPath });
  await assert.rejects(earlyExit.get('share_document').execute('early-exit', { name, content }));
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
