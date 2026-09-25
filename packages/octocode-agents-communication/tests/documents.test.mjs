import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, rmSync, mkdirSync, readFileSync, writeFileSync, symlinkSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { registerBoundTools } from '../skills/octocode-agents-communication/scripts/pi-extension.mjs';

const testInput=(command,input)=>['send_message','notify_all','lock','lock_many'].includes(command)?{reasoning:`Verify ${command} behavior in this isolated regression fixture`,...input}:input;

const root = fileURLToPath(new URL('../', import.meta.url));
const target = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)[1];
const binary = process.env.COMMUNICATION_BINARY || join(root, 'skills/octocode-agents-communication/scripts/bin', target, 'octocode-agents-communication');
function fixture(t) {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'communication-documents-')));
  const database = join(workspace, 'audit.sqlite');
  t.after(() => rmSync(workspace, { recursive: true, force: true }));
  const args = (command, session) => [command, '-', '--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])];
  const call = (command, input = {}, session) => JSON.parse(execFileSync(binary, args(command, session), {
    input: JSON.stringify(testInput(command,input)), encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], maxBuffer: 4 * 1024 * 1024,
  }));
  const a = call('join', { name: 'author', vendor: 'any-vendor' });
  const b = call('join', { name: 'reader', vendor: 'no-sdk' });
  return { workspace, database, args, call, a, b };
}

test('large document stays on disk once, compact messages reference it, audit retains attribution', t => {
  const f = fixture(t), content = 'large shared evidence\n'.repeat(20000), name = 'research.md';
  const result = f.call('share_document', { name, content }, f.a.id);
  assert.equal(result.created, true);
  assert.equal(result.document.author, f.a.id);
  assert.equal(result.document.bytes, Buffer.byteLength(content));
  assert.equal(readFileSync(join(f.workspace, result.document.path), 'utf8'), content);
  assert.deepEqual(f.call('share_document', { name, content }, f.b.id), { created: false, document: result.document });
  assert.throws(() => f.call('share_document', { name, content: 'replacement' }, f.a.id), /immutable/);
  const sent = f.call('send_message', { to: f.b.id, body: `Review ${result.document.path}`, key: name }, f.a.id);
  assert.ok(sent.id > 0);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  const rows = db.prepare("SELECT * FROM audit WHERE kind='document.created'").all();
  assert.equal(rows.length, 1);
  assert.equal(rows[0].session, f.a.id);
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
    output += page.content; input = page.next;
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
    child.stdin.end(JSON.stringify({ name, content }));
  });
  const results = await Promise.all([run(f.a.id, 'first'), run(f.b.id, 'second')]);
  assert.equal(results.filter(x => x.code === 0).length, 1);
  const winner = JSON.parse(results.find(x => x.code === 0).stdout).document;
  const result = f.call('read_document', { name }, f.a.id);
  assert.equal(result.document.author, winner.author);
  const db = new DatabaseSync(f.database); t.after(() => db.close());
  assert.equal(db.prepare("SELECT count(*) AS n FROM audit WHERE kind='document.created'").get().n, 1);
});

test('Pi bound tools send large document JSON over stdin and retain cancellation', async t => {
  const f = fixture(t), tools = JSON.parse(execFileSync(binary, ['schema'], { encoding: 'utf8', maxBuffer: 1024 * 1024 })).tools;
  const registered = new Map();
  const binding = { binary, workspace: f.workspace, database: f.database, session: f.a.id, tools };
  registerBoundTools({ registerTool: tool => registered.set(tool.name, tool) }, binding);
  const content = 'quoted "words" \\ slash\n🙂\t\u0000'.repeat(20000), name = 'pi-large.md';
  assert.ok(Buffer.byteLength(content) > 400000);
  const shared = await registered.get('share_document').execute('publish', { name, content });
  assert.equal(shared.details.created, true);
  assert.equal(shared.details.document.bytes, Buffer.byteLength(content));
  assert.equal(readFileSync(join(f.workspace, shared.details.document.path), 'utf8'), content);
  const page = await registered.get('read_document').execute('read', { name, limit: 100 });
  assert.ok(content.startsWith(page.details.content));
  assert.ok(page.details.next.offset > 0);
  const controller = new AbortController(); controller.abort();
  await assert.rejects(registered.get('share_document').execute('aborted', { name: 'aborted.md', content }, controller.signal), /abort/i);
  // A child rejecting its arguments before consuming stdin must reject, not emit unhandled EPIPE.
  const earlyExit = new Map();
  registerBoundTools({ registerTool: tool => earlyExit.set(tool.name, tool) }, { ...binding, binary: process.execPath });
  await assert.rejects(earlyExit.get('share_document').execute('early-exit', { name, content }));
});
