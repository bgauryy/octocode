import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { binary, execFileSync, tempWorkspace, withReasoning } from './helpers.mjs';

function fixture(t, { legacy = false } = {}) {
  const directory = tempWorkspace(t, 'communication-worktrees-', { real: true });
  const main = join(directory, 'main');
  const topic = join(directory, 'topic');
  const clone = join(directory, 'independent-clone');
  const outside = join(directory, 'non-git');
  for (const path of [main, topic, outside]) mkdirSync(path);
  execFileSync('git', ['-c', 'init.defaultBranch=main', 'init', '--quiet', main], { stdio: 'pipe' });
  // Construct Git's linked-worktree metadata without creating a commit.
  const metadata = join(main, '.git', 'worktrees', 'topic');
  mkdirSync(metadata, { recursive: true });
  writeFileSync(join(topic, '.git'), `gitdir: ${metadata}\n`);
  writeFileSync(join(metadata, 'HEAD'), 'ref: refs/heads/feature/topic\n');
  writeFileSync(join(metadata, 'commondir'), '../..\n');
  writeFileSync(join(metadata, 'gitdir'), `${join(topic, '.git')}\n`);
  execFileSync('git', ['clone', '--quiet', main, clone], { stdio: 'pipe' });
  const gitCommon = path => realpathSync(execFileSync('git', ['-C', path, 'rev-parse', '--path-format=absolute', '--git-common-dir'], { encoding: 'utf8', stdio: 'pipe' }).trim());
  const scope = gitCommon(main);
  assert.equal(gitCommon(topic), scope, 'Git recognizes the linked checkout and shared common directory');
  assert.notEqual(gitCommon(clone), scope, 'A clone of the same repository has a distinct local coordination scope');
  const database = join(directory, 'shared.sqlite');
  const call = (workspace, command, input = {}, session) => JSON.parse(execFileSync(binary, [
    ...command.split(' '), '-', '--workspace', workspace, '--database', database,
    ...(session ? ['--session', session] : []),
  ], { input: JSON.stringify(withReasoning(command, input)), encoding: 'utf8', stdio: 'pipe', timeout: 10_000, maxBuffer: 4 * 1024 * 1024 }));
  if (legacy) {
    const db = new DatabaseSync(database);
    db.exec(readFileSync(new URL('./fixtures/schema-v3.sql', import.meta.url), 'utf8'));
    db.close();
    return { main, topic, clone, outside, scope, database, call };
  }
  const a = call(main, 'join', { name: 'main-author', vendor: 'codex' }).id;
  const b = call(topic, 'join', { name: 'topic-reviewer', vendor: 'claude' }).id;
  const c = call(topic, 'join', { name: 'topic-observer', vendor: 'pi' }).id;
  const d = call(clone, 'join', { name: 'clone-agent', vendor: 'codex' }).id;
  const e = call(outside, 'join', { name: 'unrelated-agent', vendor: 'grok' }).id;
  return { main, topic, clone, outside, scope, call, a, b, c, d, e };
}

function allPages(f, workspace, session, command, input) {
  const rows = [];
  let page = f.call(workspace, command, input, session);
  let count = 0;
  for (;;) {
    rows.push(...page.items);
    if (!page.next) return rows;
    assert.equal(page.next.command, command);
    assert.ok(++count < 100, 'Continuation must make bounded forward progress');
    page = f.call(workspace, page.next.command, page.next.input, session);
  }
}

test('linked worktrees discover cross-vendor peers but preserve actual bindings and isolate clones', t => {
  const f = fixture(t);
  const peers = allPages(f, f.main, f.a, 'peers', {});
  assert.deepEqual(peers.map(row => row.id).sort(), [f.a, f.b, f.c].sort());
  const reviewer = peers.find(row => row.id === f.b);
  assert.equal(reviewer.workspace, f.topic);
  assert.equal(reviewer.vendor, 'claude');
  assert.equal(reviewer.branch, 'feature/topic');
  const mainBinding = f.call(f.main, 'binding', {}, f.a);
  const topicBinding = f.call(f.topic, 'binding', {}, f.b);
  assert.equal(mainBinding.coordinationScope, f.scope);
  assert.equal(topicBinding.coordinationScope, f.scope);
  assert.equal(mainBinding.workspace, f.main);
  assert.equal(topicBinding.workspace, f.topic);
  assert.deepEqual(f.call(f.clone, 'peers').items.map(row => row.id), [f.d]);
  assert.deepEqual(f.call(f.outside, 'peers').items.map(row => row.id), [f.e]);
  assert.throws(() => f.call(f.topic, 'binding', {}, f.a), /session in this workspace/);
  assert.throws(() => f.call(f.topic, 'record', { type: 'event', data: { name: 'borrowed-identity' } }, f.a), /session in this workspace/);
});

test('cross-worktree requests complete with one correlated final reply and both acknowledgements', t => {
  const f = fixture(t);
  const sent = f.call(f.main, 'send_message', { to: f.b, body: 'Review the topic checkout', key: 'review-topic', conversationId: 'worktree-review' }, f.a);
  assert.equal(sent.recipients, 1);
  const received = f.call(f.topic, 'fetch', { incoming: true, type: 'message' }, f.b).items;
  assert.equal(received.length, 1);
  assert.equal(received[0].path, f.main);
  assert.equal(received[0].from, f.a);
  assert.equal(received[0].branch, 'main');
  assert.equal(received[0].data.messageId, sent.id);
  assert.throws(() => f.call(f.topic, 'complete', { message: sent.id }, f.b), /requires a final answer/);
  const finished = f.call(f.topic, 'complete', { message: sent.id, reply: 'Reviewed feature/topic; compatibility is preserved.' }, f.b);
  assert.equal(finished.completed, true);
  assert.equal(f.call(f.topic, 'complete', { message: sent.id, reply: 'Reviewed feature/topic; compatibility is preserved.' }, f.b).id, finished.id);
  assert.deepEqual(f.call(f.topic, 'inbox', {}, f.b).items, []);
  const answer = f.call(f.main, 'fetch', { incoming: true, type: 'message' }, f.a).items[0];
  assert.equal(answer.path, f.topic);
  assert.equal(answer.branch, 'feature/topic');
  assert.equal(answer.data.replyTo, sent.id);
  assert.equal(answer.data.conversationId, 'worktree-review');
  assert.equal(answer.data.replyRequired, false);
  f.call(f.main, 'complete', { message: answer.data.messageId }, f.a);
  assert.deepEqual(f.call(f.main, 'inbox', {}, f.a).items, []);
  assert.deepEqual(f.call(f.topic, 'fetch', { type: 'message' }, f.c).items, [], 'Repository coordination does not expose another participant conversation');
  for (const [workspace, identity] of [[f.clone, f.d], [f.outside, f.e]]) {
    assert.deepEqual(f.call(workspace, 'fetch', { type: 'message' }, identity).items, []);
    assert.throws(() => f.call(f.main, 'send_message', { to: identity, body: 'Wrong repository' }, f.a), /Unknown session in this repository/);
  }
});

test('topics and broadcasts include linked checkout recipients and exclude other repositories', t => {
  const f = fixture(t);
  for (const [workspace, identity] of [[f.topic, f.b], [f.clone, f.d], [f.outside, f.e]]) f.call(workspace, 'subscribe', { topics: ['api'] }, identity);
  const topic = f.call(f.main, 'send_message', { topic: 'api', body: 'API evidence is ready' }, f.a);
  assert.equal(topic.recipients, 1);
  assert.equal(f.call(f.topic, 'inbox', {}, f.b).items[0].id, topic.id);
  assert.deepEqual(f.call(f.topic, 'inbox', {}, f.c).items, []);
  const broadcast = f.call(f.main, 'notify_all', { body: 'Repository handoff is ready' }, f.a);
  assert.equal(broadcast.recipients, 2);
  assert.deepEqual(f.call(f.main, 'inbox', {}, f.a).items, [], 'Sender is not a broadcast recipient');
  assert.deepEqual(f.call(f.topic, 'inbox', {}, f.b).items.map(row => row.id), [topic.id, broadcast.id]);
  assert.deepEqual(f.call(f.topic, 'inbox', {}, f.c).items.map(row => row.id), [broadcast.id]);
  f.call(f.topic, 'complete', { messages: [topic.id, broadcast.id] }, f.b);
  f.call(f.topic, 'complete', { message: broadcast.id }, f.c);
  assert.deepEqual(f.call(f.clone, 'inbox', {}, f.d).items, []);
  assert.deepEqual(f.call(f.outside, 'inbox', {}, f.e).items, []);
});

test('identical relative files in linked checkouts have independent leases and write coverage', t => {
  const f = fixture(t), target = 'src/shared.ts';
  const mainLease = f.call(f.main, 'lock', { path: target }, f.a).lease;
  assert.equal(f.call(f.topic, 'check_write', { paths: [{ path: target }] }, f.b).ok, false, 'Lease from the main checkout cannot cover a linked checkout write');
  const topicLease = f.call(f.topic, 'lock', { path: target }, f.b).lease;
  assert.notEqual(topicLease.id, mainLease.id);
  assert.equal(f.call(f.main, 'check_write', { paths: [{ path: target }] }, f.a).ok, true);
  assert.equal(f.call(f.topic, 'check_write', { paths: [{ path: target }] }, f.b).ok, true);
  assert.deepEqual(f.call(f.main, 'locks', {}, f.a).items.map(row => row.id), [mainLease.id]);
  assert.deepEqual(f.call(f.topic, 'locks', {}, f.b).items.map(row => row.id), [topicLease.id]);
  assert.equal(f.call(f.topic, 'lock', { path: target }, f.c).ok, false, 'Same-checkout conflicts remain enforced');
  assert.throws(() => f.call(f.topic, 'check_write', { paths: [{ path: target }] }, f.a), /session in this workspace/);
  assert.equal(f.call(f.topic, 'unlock', { leaseId: mainLease.id }, f.b).released, false);
  f.call(f.main, 'unlock', { leaseId: mainLease.id }, f.a);
  assert.equal(f.call(f.topic, 'check_write', { paths: [{ path: target }] }, f.b).ok, true);
});

test('shared history preserves origin worktree and branch across fixed-ceiling pagination', t => {
  const f = fixture(t);
  const origins = [f.main, f.topic, f.main, f.topic, f.main];
  const identities = [f.a, f.b, f.a, f.b, f.a];
  const records = origins.map((workspace, index) => f.call(workspace, 'record', { type: 'event', data: { name: 'checkpoint', index } }, identities[index]));
  f.call(f.clone, 'record', { type: 'event', data: { name: 'checkpoint', index: -1 } }, f.d);
  let page = f.call(f.topic, 'fetch', { type: 'event', where: { name: 'checkpoint' }, limit: 2 }, f.b);
  assert.ok(page.next);
  f.call(f.main, 'record', { type: 'event', data: { name: 'checkpoint', index: 99 } }, f.a);
  const rows = [...page.items];
  while (page.next) { page = f.call(f.topic, page.next.command, page.next.input, f.b); rows.push(...page.items); }
  assert.deepEqual(rows.map(row => row.recordId), records.map(row => row.recordId));
  assert.deepEqual(rows.map(row => row.path), origins);
  assert.deepEqual(rows.map(row => row.branch), ['main', 'feature/topic', 'main', 'feature/topic', 'main']);
  assert.equal(new Set(rows.map(row => row.recordId)).size, records.length);
  const memory = f.call(f.main, 'record', { type: 'memory', data: { content: 'Verified shared API contract' }, to: f.b }, f.a);
  assert.equal(f.call(f.topic, 'fetch', { type: 'memory' }, f.b).items[0].recordId, memory.recordId);
  assert.deepEqual(f.call(f.topic, 'fetch', { type: 'memory' }, f.c).items, [], 'Targeted memory remains participant-visible');
  assert.deepEqual(f.call(f.outside, 'fetch', { type: 'event' }, f.e).items, []);
});

test('immutable evidence and context carry worktree origins across complete Unicode pages', t => {
  const f = fixture(t), content = 'α🙂abc\n'.repeat(4);
  const shared = f.call(f.main, 'share_document', { name: 'worktree-proof.md', content, context: { summary: 'Main checkout evidence', path: 'src/shared.ts', kind: 'file' } }, f.a);
  assert.equal(resolve(f.main, shared.document.path), join(f.scope, 'octocode-communication', 'worktree-proof.md'));
  assert.equal(readFileSync(resolve(f.main, shared.document.path), 'utf8'), content);
  let page = f.call(f.topic, 'read_document', { name: shared.document.name, limit: 7 }, f.b);
  let text = page.content, pages = 1;
  while (page.next) {
    assert.equal(page.next.command, 'read_document');
    assert.equal(page.next.input.offset, Buffer.byteLength(text));
    page = f.call(f.topic, page.next.command, page.next.input, f.b);
    text += page.content;
    assert.ok(++pages < 100);
  }
  assert.ok(pages > 1);
  assert.equal(text, content);
  const published = f.call(f.topic, 'fetch', { type: 'document' }, f.b).items[0];
  assert.equal(published.path, f.main);
  assert.equal(published.branch, 'main');
  const mainContext = f.call(f.topic, 'context', { path: 'src/shared.ts' }, f.b).items;
  assert.deepEqual(mainContext.map(row => row.name), ['worktree-proof.md']);
  assert.equal(mainContext[0].workspace, f.main);
  assert.equal(mainContext[0].branch, 'main');
  f.call(f.topic, 'share_document', { name: 'topic-proof.md', content: 'Topic checkout evidence', context: { summary: 'Topic checkout evidence', path: 'src/shared.ts', kind: 'file' } }, f.b);
  const contexts = allPages(f, f.topic, f.b, 'context', { path: 'src/shared.ts', limit: 1 });
  assert.deepEqual(contexts.map(row => row.name), ['worktree-proof.md', 'topic-proof.md']);
  assert.deepEqual(contexts.map(row => row.workspace), [f.main, f.topic]);
  assert.throws(() => f.call(f.topic, 'share_document', { name: shared.document.name, content: 'Replacement evidence' }, f.b), /immutable/);
  assert.throws(() => f.call(f.clone, 'read_document', { name: shared.document.name }, f.d), /Unknown document/);
});


test('v3 migration preserves every row and old document path while merging linked worktree coordination', t => {
  const f = fixture(t, { legacy: true }), at = Date.now();
  const db = new DatabaseSync(f.database);
  const insertSession = db.prepare('INSERT INTO sessions(id,workspace,name,vendor,branch,expiresAt) VALUES(?,?,?,?,?,?)');
  insertSession.run('legacy-main', f.main, 'Main author', 'codex', 'main', at + 600000);
  insertSession.run('legacy-topic', f.topic, 'Topic author', 'claude', 'feature/topic', at + 600000);
  insertSession.run('legacy-recipient', f.main, 'Main recipient', 'grok', 'main', at + 600000);
  const message = Number(db.prepare("INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,replyRequired) VALUES('legacy-main','legacy-recipient','Retained v3 request','v3-request',?,'Preserve conversation during migration',1)").run(at + 600000).lastInsertRowid);
  db.prepare("INSERT INTO deliveries(message,recipient) VALUES(?,'legacy-recipient')").run(message);
  db.prepare("INSERT INTO leases(workspace,path,kind,owner,acquiredAt,refreshedAt,expiresAt,reasoning,pathKey) VALUES(?,?,'file','legacy-main',?,?,?,'Preserve existing file ownership',?)").run(f.main, join(f.main, 'retained.ts'), at, at, at + 60000, join(f.main, 'retained.ts').toLowerCase());
  const contents = new Map();
  const publishLegacy = (workspace, author, name, content) => {
    const folder = join(workspace, '.octocode', 'communication');
    mkdirSync(folder, { recursive: true });
    writeFileSync(join(folder, name), content);
    const data = { name, path: '.octocode/communication/' + name, author, bytes: Buffer.byteLength(content), sha256: createHash('sha256').update(content).digest('hex'), reasoning: 'Preserve historical evidence' };
    db.prepare("INSERT INTO records(path,[from],type,entityId,timestamp,data,key) VALUES(?,?,'document',?,?,?,?)").run(workspace, author, name, at, JSON.stringify(data), name);
    contents.set(workspace + '/' + name, content);
  };
  publishLegacy(f.main, 'legacy-main', 'old-main.md', 'Preserved main checkout evidence.');
  publishLegacy(f.main, 'legacy-main', 'same-name.md', 'Main α🙂 evidence across pages.');
  publishLegacy(f.topic, 'legacy-topic', 'same-name.md', 'Topic β🙂 evidence across pages.');
  const tables = ['sessions', 'records', 'messages', 'deliveries', 'leases', 'documents'];
  const snapshot = Object.fromEntries(tables.map(table => [table, db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all()]));
  db.close();
  assert.throws(() => f.call(f.main, 'binding', {}, 'legacy-main'), /Incompatible/);
  const backup = join(f.main, 'before-v4.sqlite');
  const result = f.call(f.main, 'db migrate', { backup });
  assert.equal(result.sourceSchemaVersion, 3);
  assert.equal(result.schemaVersion, 5);
  const migrated = new DatabaseSync(f.database, { readOnly: true });
  const previous = new DatabaseSync(backup, { readOnly: true });
  t.after(() => { migrated.close(); previous.close(); });
  assert.equal(previous.prepare('PRAGMA user_version').get().user_version, 3);
  for (const table of tables) {
    assert.deepEqual(migrated.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all(), snapshot[table], `${table} state remains exact`);
    assert.deepEqual(previous.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all(), snapshot[table], `${table} backup remains exact`);
  }
  assert.deepEqual(migrated.prepare('PRAGMA foreign_key_check').all(), []);
  assert.deepEqual(migrated.prepare('SELECT workspace,coordinationScope FROM workspaces ORDER BY workspace').all().map(row => ({ ...row })), [f.main, f.topic].sort().map(workspace => ({ workspace, coordinationScope: f.scope })));
  assert.equal(f.call(f.topic, 'read_document', { name: 'old-main.md' }, 'legacy-topic').content, contents.get(f.main + '/old-main.md'));
  assert.throws(() => f.call(f.topic, 'read_document', { name: 'same-name.md' }, 'legacy-topic'), /Ambiguous historical document name/);
  for (const workspace of [f.main, f.topic]) {
    let page = f.call(f.topic, 'read_document', { name: 'same-name.md', workspace, limit: 7 }, 'legacy-topic');
    let content = page.content;
    assert.ok(page.next);
    while (page.next) {
      assert.equal(page.next.input.workspace, workspace, 'Continuation keeps the selected historical origin');
      assert.equal(page.next.input.offset, Buffer.byteLength(content));
      page = f.call(f.topic, page.next.command, page.next.input, 'legacy-topic');
      content += page.content;
    }
    assert.equal(content, contents.get(workspace + '/same-name.md'));
    assert.equal(readFileSync(join(workspace, '.octocode', 'communication', 'same-name.md'), 'utf8'), content);
  }
  assert.throws(() => f.call(f.topic, 'read_document', { name: 'same-name.md', workspace: f.clone }, 'legacy-topic'), /outside this repository/);
  const sent = f.call(f.main, 'send_message', { to: 'legacy-topic', body: 'Migration now permits cross-worktree coordination' }, 'legacy-main');
  assert.equal(f.call(f.topic, 'inbox', {}, 'legacy-topic').items[0].id, sent.id);
  assert.equal(f.call(f.main, 'check_write', { paths: [{ path: 'retained.ts' }] }, 'legacy-main').ok, true);
  assert.equal(f.call(f.topic, 'check_write', { paths: [{ path: 'retained.ts' }] }, 'legacy-topic').ok, false);
});


test('repository routing ignores Git environment overrides and does not require a Git executable', t => {
  const f = fixture(t);
  for (const unavailable of [false, true]) {
    const identity = JSON.parse(execFileSync(binary, ['join', JSON.stringify({ name: 'metadata-bound', vendor: 'generic' }), '--workspace', f.topic, '--database', join(f.main, '..', 'shared.sqlite')], {
      encoding: 'utf8', stdio: 'pipe', timeout: 10000,
      env: { ...process.env, GIT_DIR: join(f.clone, '.git'), GIT_COMMON_DIR: join(f.clone, '.git'), ...(unavailable ? { PATH: join(f.main, 'no-executables') } : {}) },
    }));
    const binding = f.call(f.topic, 'binding', {}, identity.id);
    assert.equal(binding.coordinationScope, f.scope);
    if (!unavailable) assert.equal(binding.branch, 'feature/topic');
    assert.ok(f.call(f.main, 'peers', {}, f.a).items.some(row => row.id === identity.id));
  }
});
