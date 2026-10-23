import { test } from 'node:test';
import type { TestContext } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, readFileSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
import { join } from 'node:path';
import { Worker } from 'node:worker_threads';
import { execFileSync, tempWorkspace } from './helpers.ts';
import { golden, runtime } from './golden-support.ts';
import type { RecordInput } from '../src/runtime/catalog-types.generated.ts';
import type { JsonRecord } from './json-record.ts';

const { database, databaseExport, records, timing, ValidationError } = runtime;
type Database = ReturnType<typeof database.openDatabase>;
type RecordsHost = Parameters<typeof records.fetch>[0];
// A database the previous runtime created, with its workspace (/usr), identities and outputs frozen in
// fixtures/legacy-database.{sqlite,json}; every expiry lies in the year 2100.
const legacy = JSON.parse(readFileSync(new URL('./fixtures/legacy-database.json', import.meta.url), 'utf8'));
const sha256 = (path: string) => createHash('sha256').update(readFileSync(path)).digest('hex');

/** The slice of the store the record operations need, over a database and its single workspace. */
function host(db: Database, workspace: string): RecordsHost {
  const session = (id: string, active: boolean) => {
    const rows = database.query(db, 'SELECT * FROM sessions WHERE id=? AND workspace=? AND (?=0 OR expiresAt>?)', [id, workspace, active, timing.now()]);
    if (!rows.length) throw new ValidationError('Unknown or expired session in this workspace.');
    return runtime.rows.sessionRow(rows[0]!);
  };
  return {
    db, workspace,
    known: (id: string, active = true) => session(id, active),
    peer: (id: string) => runtime.rows.sessionRow(database.query(db, 'SELECT * FROM sessions WHERE id=?', [id])[0]!),
    resolvePeer: (value: string) => value,
    sharedWorkspaces: () => [workspace],
  };
}

/** A private copy of the legacy database in a scratch directory. */
const legacySession = (t: TestContext) => {
  const directory = tempWorkspace(t, 'database-', { real: true });
  const file = join(directory, 'legacy.sqlite');
  copyFileSync(new URL('./fixtures/legacy-database.sqlite', import.meta.url), file);
  return { directory, workspace: legacy.info.workspace, file, id: legacy.agent };
};

test('typed column reads return SQLite values and refuse a column of another type', () => {
  const db = new database.Database(new DatabaseSync(':memory:'));
  try {
    assert.equal(db.integer('SELECT 41 + 1'), 42);
    assert.equal(db.value('SELECT NULL'), null);
    assert.deepEqual({ ...db.get("SELECT 'a' AS name, 7 AS count") }, { name: 'a', count: 7 });
    assert.throws(() => db.integer("SELECT 'text'"), /first column is not an integer/);
    assert.throws(() => db.integer('SELECT 1 WHERE 0'), /first column is not an integer/);
  } finally { db.close(); }
});

test('a database created by the legacy runtime is compatible and usable from TypeScript', t => {
  const { workspace, file, id } = legacySession(t);
  const info = legacy.info;
  assert.equal(info.compatible, true);
  assert.equal(info.expectedSchemaSha256, database.schema().schemaSha256);
  assert.equal(info.schemaSha256, database.schema().schemaSha256);
  assert.equal(info.applicationId, database.schema().applicationId);
  assert.equal(info.schemaVersion, database.schema().schemaVersion);

  const inspected = database.inspect(file, workspace);
  assert.equal(inspected.compatible, true);
  assert.equal(inspected.schemaSha256, info.schemaSha256);
  assert.deepEqual(Object.keys(inspected).filter(key => key !== 'sqliteVersion' && key !== 'path'), Object.keys(info).filter(key => key !== 'path'));
  for (const key of ['workspace', 'exists', 'applicationId', 'schemaVersion', 'schemaSha256', 'compatible', 'expectedSchemaSha256']) assert.equal(inspected[key as keyof typeof inspected], info[key as keyof typeof info], key);

  const reader = database.openDatabase(file, true);
  assert.equal(database.compatible(reader), true);
  assert.deepEqual(database.query(reader, 'SELECT id FROM sessions ORDER BY name').map(row => row.id), [id, legacy.peer]);
  assert.throws(() => reader.run('DELETE FROM sessions'), /readonly/i);
  reader.close();

  // Records written by the legacy runtime read back identically through the TypeScript fetch.
  const db = database.openDatabase(file);
  assert.deepEqual(records.fetch(host(db, workspace), id, {}), legacy.fetch);
  assert.deepEqual(records.fetch(host(db, workspace), id, { type: 'event' }), legacy.fetchEvent);
  assert.deepEqual(records.fetch(host(db, workspace), id, { type: 'event', where: { name: 'c', 'checks.passed': true } }).items.map((item: JsonRecord) => item.data.name), ['c']);
  assert.deepEqual(records.fetch(host(db, workspace), id, { where: { 'checks.passed': false }, type: 'event' }).items, []);
  assert.equal(records.fetch(host(db, workspace), id, { search: '"coordinate.in"' }).items.length, 0);
  db.close();
});

test('TypeScript records are idempotent by key and store the legacy compact encoding', t => {
  const { workspace, file, id } = legacySession(t);
  const db = database.openDatabase(file);
  const store = host(db, workspace);
  const input: RecordInput = { type: 'event', data: { z: 1, a: { y: [1, 'é', '😀'], b: null }, name: 'ts' }, key: 'k' };
  const written = records.record(store, id, input);
  assert.deepEqual(records.fetch(store, id, { type: 'event', where: { name: 'ts' } }).items, [written]);
  // Same key and an equal body in another key order is the same record; different data is refused.
  assert.deepEqual(records.record(store, id, { ...input, data: { name: 'ts', a: { b: null, y: [1, 'é', '😀'] }, z: 1 } }), written);
  assert.throws(() => records.record(store, id, { ...input, data: { name: 'other' } }), /Record key reused with different data, routing or branch/);
  assert.throws(() => records.record(store, id, { ...input, to: id }), /Record key reused/);
  assert.throws(() => records.record(store, id, { type: 'event', data: { name: 'x', blob: 'x'.repeat(17000) } }), /Record data exceeds 16384 UTF-8 bytes/);
  assert.throws(() => records.record(store, id, { type: 'event' } as RecordInput), ValidationError);
  // Stored bytes are the compact JSON of the data in insertion order, with non-ASCII characters unescaped.
  assert.equal(db.value('SELECT data FROM records WHERE key=?', ['k']), '{"z":1,"a":{"y":[1,"é","😀"],"b":null},"name":"ts"}');
  // A paged fetch reaches every row once.
  for (let index = 0; index < 7; index += 1) records.record(store, id, { type: 'event', data: { name: 'bulk', index }, key: 'bulk' + index });
  const seen = [];
  let after = 0;
  for (let page = records.fetch(store, id, { limit: 3 }); ; page = records.fetch(store, id, { limit: 3, through: page.through, after })) {
    seen.push(...page.items.map((item: JsonRecord) => item.recordId));
    if (!page.next) break;
    after = Number(page.next.input.after);
  }
  assert.deepEqual(seen, records.fetch(store, id, { limit: 100 }).items.map((item: JsonRecord) => item.recordId));
  assert.equal(new Set(seen).size, seen.length);
  assert.equal(seen.length, legacy.fetchAll.items.length + 8);
  db.close();
});

test('message envelopes and append read what the legacy runtime wrote', t => {
  const { workspace, file, id } = legacySession(t);
  const db = database.openDatabase(file);
  const store = host(db, workspace);
  const sent = legacy.tasks.items.find((item: JsonRecord) => item.data.body === 'hello é').data.messageId;
  const [envelope] = records.messageEnvelopes(store, [{ id: sent }]);
  assert.equal(envelope.type, 'message');
  assert.equal(envelope.data.body, 'hello é');
  assert.equal(envelope.data.kind, 'note');
  assert.deepEqual(records.messageEnvelopes(store, []), []);
  assert.throws(() => records.messageEnvelopes(store, [{ id: 99999 }]), /Message envelope is missing/);
  const first = records.append(db, id, 'event', { name: 'append', n: 1 }, timing.now(), 'append-key');
  assert.ok(first > 0);
  assert.equal(database.query(db, 'SELECT path FROM records WHERE id=?', [first])[0].path, workspace);
  db.close();
});

test('a database created by TypeScript has the schema the legacy runtime created', t => {
  const workspace = tempWorkspace(t, 'database-', { real: true });
  const file = join(workspace, 'nested/dir/typescript.sqlite');
  const db = database.openDatabase(file, false, true);
  assert.equal(database.compatible(db), true);
  assert.equal(db.value('PRAGMA foreign_keys'), 1);
  assert.equal(db.value('PRAGMA busy_timeout'), 5000);
  assert.equal(db.value('PRAGMA synchronous'), 2);
  assert.equal(statSync(file).mode & 0o777, 0o600);
  assert.equal(statSync(join(workspace, 'nested/dir')).mode & 0o777, 0o700);

  // While a writer is open its -wal/-shm files exist, which every SQLite can read through a read-only connection.
  const info = database.inspect(file, workspace);
  assert.equal(info.compatible, true);
  assert.equal(info.schemaSha256, legacy.info.schemaSha256);
  assert.equal(info.schemaVersion, legacy.info.schemaVersion);
  assert.equal(info.applicationId, legacy.info.applicationId);
  // The same objects: every table, index and trigger statement of the legacy database exists here verbatim.
  const objects = (connection: Database) => database.query(connection, "SELECT type, name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name");
  const reference = database.openDatabase(legacySession(t).file, true);
  assert.deepEqual(objects(db), objects(reference));
  reference.close();
  db.close();
});

test('opening refuses what this runtime does not own and never modifies it', t => {
  const workspace = tempWorkspace(t, 'database-', { real: true });
  const file = join(workspace, 'old.sqlite');
  database.openDatabase(file, false, true).close();
  const raw = new DatabaseSync(file);
  raw.exec('PRAGMA user_version=6');
  raw.close();
  const before = sha256(file);
  for (const [readOnly, create] of [[true, false], [false, true], [false, false]])
    assert.throws(() => database.openDatabase(file, readOnly, create), /Incompatible agents-communication database.*existing data is not modified/);
  assert.equal(sha256(file), before);
  assert.equal(database.inspect(file, workspace).compatible, false);

  assert.throws(() => database.openDatabase(join(workspace, 'missing.sqlite')), /Database does not exist: .*missing.sqlite. Run join first./);
  assert.throws(() => database.openDatabase(join(workspace, 'missing.sqlite'), true, true), /Database does not exist/);
  assert.equal(existsSync(join(workspace, 'missing.sqlite')), false);

  const extra = join(workspace, 'extra.sqlite');
  database.openDatabase(extra, false, true).close();
  const modified = new DatabaseSync(extra);
  modified.exec('CREATE TABLE extra(x)');
  modified.close();
  assert.throws(() => database.openDatabase(extra), /Incompatible agents-communication database/);

  const blank = join(workspace, 'blank.sqlite');
  writeFileSync(blank, '');
  assert.throws(() => database.openDatabase(blank), /Database is not initialized. Run join first./);
  assert.throws(() => database.openDatabase(blank, true), /Incompatible/);
  database.openDatabase(blank, false, true).close();
  assert.equal(database.inspect(blank, workspace).compatible, true);
});

test('the WAL safety gate matches the recorded output', () => {
  assert.deepEqual(golden.walSafe.map(({ version }: JsonRecord) => database.walSafe(version)), golden.walSafe.map(({ safe }: JsonRecord) => safe));
  assert.deepEqual(database.sqliteVersion().length, 3);
});

test('transactions are explicit: commit, rollback on any throw, no nesting, no async bodies', t => {
  const workspace = tempWorkspace(t, 'database-', { real: true });
  const db = database.openDatabase(join(workspace, 'tx.sqlite'), false, true);
  const sessionSql = 'INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES(?,?,?,?,?)';
  const count = () => db.value('SELECT count(*) FROM sessions');
  assert.equal(database.transaction(db, () => { db.run(sessionSql, ['one', workspace, 'one', 'generic', 1]); assert.equal(db.inTransaction, true); return 'done'; }), 'done');
  assert.equal(db.inTransaction, false);
  assert.equal(count(), 1);
  assert.throws(() => database.transaction(db, () => { db.run(sessionSql, ['two', workspace, 'two', 'generic', 1]); throw new RangeError('boom'); }), RangeError);
  assert.equal(count(), 1);
  assert.equal(db.inTransaction, false);
  assert.throws(() => database.transaction(db, () => database.transaction(db, () => undefined)), /within a transaction/);
  assert.equal(db.inTransaction, false);
  assert.throws(() => database.transaction(db, () => Promise.resolve()), /synchronous/);
  assert.equal(db.inTransaction, false);
  assert.equal(database.readTransaction(db, () => count()), 1);
  // A statement the schema rejects (here a trigger or foreign key) inside a write transaction rolls everything back.
  assert.throws(() => database.transaction(db, () => {
    db.run(sessionSql, ['four', workspace, 'four', 'generic', 1]);
    db.run('INSERT INTO subscriptions(session,topic) VALUES(?,?)', ['no-such-session', 'topic']);
  }), /FOREIGN KEY|author workspace/i);
  assert.equal(count(), 1);
  db.close();
});

test('a second writer waits on the busy timeout instead of failing at once', async t => {
  const workspace = tempWorkspace(t, 'database-', { real: true });
  const file = join(workspace, 'busy.sqlite');
  database.openDatabase(file, false, true).close();
  const waiter = database.openDatabase(file);
  // The wait blocks this thread, so the competing writer lives in a worker that holds the write lock for 400 ms.
  const worker = new Worker(`
    const { workerData, parentPort } = require('node:worker_threads');
    const { DatabaseSync } = require('node:sqlite');
    const db = new DatabaseSync(workerData.file);
    db.exec('BEGIN IMMEDIATE');
    parentPort.postMessage('locked');
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 400);
    db.exec('COMMIT');
  `, { eval: true, workerData: { file } });
  await new Promise(resolve => worker.once('message', resolve));
  const started = Date.now();
  const result = database.transaction(waiter, () => waiter.run('INSERT INTO workspaces(workspace,coordinationScope) VALUES(?,?)', ['w', 'w']));
  assert.equal(result.changes, 1);
  assert.ok(Date.now() - started >= 200, 'the writer waited for the lock');
  await new Promise(resolve => worker.once('exit', resolve));
  waiter.close();
});

test('lease_overlap is registered and uses the frozen key rules', t => {
  const workspace = tempWorkspace(t, 'database-', { real: true });
  const db = database.openDatabase(join(workspace, 'fn.sqlite'), false, true);
  const overlap = (a: string, ak: string, b: string, bk: string) => db.value('SELECT lease_overlap(?,?,?,?)', [a, ak, b, bk]);
  assert.equal(overlap('/src', 'tree', '/SRC/a.ts', 'file'), 1);
  assert.equal(overlap('/Straße', 'file', '/STRASSE', 'file'), 1);
  assert.equal(overlap('/a', 'file', '/ab', 'file'), 0);
  assert.equal(overlap('a', 'tree', 'a/b/c', 'file'), 1);
  db.close();
});

test('bound values keep the legacy semantics: booleans are 0/1, whole numbers are INTEGER, NULL columns are dropped', t => {
  const workspace = tempWorkspace(t, 'database-', { real: true });
  const db = database.openDatabase(join(workspace, 'bind.sqlite'), false, true);
  assert.equal(db.value('SELECT typeof(?)', [5]), 'integer');
  assert.equal(db.value('SELECT typeof(?)', [5.5]), 'real');
  assert.equal(db.value('SELECT ?', [true]), 1);
  assert.equal(db.value('SELECT ?', [false]), 0);
  assert.equal(db.value('SELECT ?', [null]), null);
  assert.equal(db.value('SELECT ? IS NULL', [null]), 1);
  // The runtime also guards callers that pass untyped values; `Reflect.apply` takes untyped arguments like such a caller.
  assert.throws(() => Reflect.apply(db.run, db, ['SELECT ?', [undefined]]), /undefined/);
  assert.deepEqual(database.query(db, 'SELECT 1 AS a, NULL AS b, 0 AS replyRequired'), [{ a: 1, replyRequired: false }]);
  assert.deepEqual(database.query(db, 'SELECT 1 AS replyRequired'), [{ replyRequired: true }]);
  assert.equal(database.execute(db, 'INSERT INTO workspaces(workspace,coordinationScope) VALUES(?,?)', ['x', 'x']), 1);
  assert.equal(db.run('INSERT INTO workspaces(workspace,coordinationScope) VALUES(?,?)', ['y', 'y']).changes, 1);
  assert.equal(db.run("INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES('s','x','n','v',1)").lastInsertRowid, Number(db.value('SELECT last_insert_rowid()')));
  assert.equal(db.get('SELECT 1 WHERE 0'), undefined);
  assert.equal(db.value('SELECT 1 WHERE 0'), undefined);
  db.close();
});

test('exporting writes a verified, never-overwriting snapshot the legacy runtime format accepts', async t => {
  const { directory: workspace, file } = legacySession(t);
  const destination = join(workspace, 'snapshot.sqlite');
  const result = await databaseExport.exportDatabase(file, destination);
  assert.deepEqual(Object.keys(result), ['path', 'source', 'schemaVersion', 'sha256', 'bytes', 'scope', 'includesWorkspaceDocuments', 'documents', 'integrity', 'directorySynced']);
  assert.equal(result.path, destination);
  assert.equal(result.source, file);
  assert.equal(result.sha256, sha256(destination));
  assert.equal(result.bytes, statSync(destination).size);
  assert.equal(result.integrity, 'ok');
  assert.equal(result.schemaVersion, 7);
  assert.equal(database.inspect(destination, legacy.info.workspace).compatible, true);
  const copy = database.openDatabase(destination, true);
  assert.equal(copy.value('PRAGMA journal_mode'), 'delete');
  assert.equal(database.query(copy, "SELECT count(*) AS n FROM records WHERE type='event'")[0].n, 1);
  assert.equal(database.query(copy, 'SELECT count(*) AS n FROM sessions')[0].n, 2);
  copy.close();
  assert.deepEqual(readdirSyncNames(workspace).filter(name => name.startsWith('.communication-export-')), []);

  await assert.rejects(databaseExport.exportDatabase(file, destination), /Export destination already exists/);
  symlinkSync(destination, join(workspace, 'link.sqlite'));
  await assert.rejects(databaseExport.exportDatabase(file, join(workspace, 'link.sqlite')), /Export destination already exists/);
  await assert.rejects(databaseExport.exportDatabase(file, 'relative.sqlite'), /Export path must be absolute/);
  symlinkSync(file, join(workspace, 'source-link.sqlite'));
  await assert.rejects(databaseExport.exportDatabase(join(workspace, 'source-link.sqlite'), join(workspace, 'other.sqlite')), /regular database file, not a symlink/);
  assert.equal(existsSync(join(workspace, 'other.sqlite')), false);
  await assert.rejects(databaseExport.exportDatabase(file, join(workspace, 'missing-dir/out.sqlite')), { code: 'ENOENT' });
});

function readdirSyncNames(directory: string) {
  return execFileSync('ls', ['-A', directory], { encoding: 'utf8' }).split('\n').filter(Boolean);
}
