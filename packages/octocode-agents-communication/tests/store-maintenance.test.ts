// Store maintenance: retention reports and compaction, the active-database registry and single-instant reads. Every case
// mirrors the black-box CLI tests of the same name (tests/retention.test.ts, registry) at the store level.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { octocodeHome, only } from './helpers.ts';
import type { JsonRecord } from './json-record.ts';
import { fixture, runtime, Store } from './store-support.ts';

// --- Retention -------------------------------------------------------------------------------------------------

function message(db: DatabaseSync, session: string, key: string, { acknowledged = true, uncertain = false, expiresAt = 1, replyTo = null }: { acknowledged?: boolean; uncertain?: boolean; expiresAt?: number; replyTo?: number | null } = {}) {
  const id = Number(db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,replyTo,replyRequired,kind) VALUES(?,?,?,?,?,?,?,?,?)')
    .run(session, session, `audit body ${key}`, key, expiresAt, 'Keep complete protocol evidence', replyTo, replyTo === null ? 1 : 0, replyTo === null ? 'question' : 'answer').lastInsertRowid);
  db.prepare('INSERT INTO deliveries(message,recipient,acknowledgedAt) VALUES(?,?,?)').run(id, session, acknowledged ? Date.now() : null);
  if (uncertain) db.prepare('INSERT INTO dispatches(message,recipient,token,transport,state,attemptedAt) VALUES(?,?,?,?,?,?)').run(id, session, 'attempt', 'raw', 'uncertain', Date.now());
  return id;
}

const contents = (db: DatabaseSync) => Object.fromEntries(['sessions', 'subscriptions', 'leases', 'messages', 'deliveries', 'attachments', 'dispatches', 'records', 'sqlite_sequence'].map(table => [table, db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all()]));

test('retention report is bounded, paginated and preserves pending, uncertain and correlated history', async t => {
  const f = fixture(t);
  const session = await f.join('retention-owner');
  const db = new DatabaseSync(f.file);
  db.exec('PRAGMA foreign_keys=ON');
  t.after(() => db.close());
  const parent = message(db, session, 'settled');
  message(db, session, 'pending', { acknowledged: false });
  message(db, session, 'uncertain', { uncertain: true });
  message(db, session, 'future', { expiresAt: Date.now() + 60000 });
  message(db, session, 'reply', { replyTo: parent });
  const before = contents(db);
  const one = runtime.retention.report(f.file, { limit: 3 });
  assert.equal(one.readOnly, true);
  assert.equal(one.deletionSupported, false);
  assert.equal(one.page.count, 3);
  assert.equal(one.page.hasMore, true);
  assert.equal(one.page.expiredSettledMessages, 1);
  assert.equal(one.page.unacknowledgedMessages, 1);
  assert.equal(one.page.unresolvedDispatchMessages, 1);
  const two = runtime.retention.report(f.file, one.next!.input);
  assert.equal(two.page.count, 2);
  assert.equal(two.page.expiredSettledMessages, 1);
  assert.equal(two.next ?? null, null);
  assert.equal(two.before, one.before, 'pagination freezes the original expiry cutoff');
  assert.deepEqual(contents(db), before);
  for (const input of [{ limit: 0 }, { limit: 1001 }, { before: Date.now() + 60000 }, { afterId: -1 }, { delete: true }]) assert.throws(() => runtime.retention.report(f.file, input), JSON.stringify(input));
  const missing = join(f.workspace, 'absent.sqlite');
  assert.throws(() => runtime.retention.report(missing, {}));
  assert.equal(existsSync(missing), false);
});

test('compaction reclaims freed pages without removing any protocol record, and refuses symlinks and schema drift', async t => {
  const f = fixture(t);
  const session = await f.join('retention-owner');
  const db = new DatabaseSync(f.file);
  db.exec('PRAGMA foreign_keys=ON');
  t.after(() => db.close());
  const parent = message(db, session, 'retain-key');
  message(db, session, 'retain-reply', { replyTo: parent });
  const lease = db.prepare('INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning,pathKey) VALUES(?,?,?,?,?,?,?)');
  db.exec('BEGIN');
  for (let n = 0; n < 400; n++) lease.run(f.workspace, `${n}/${'p'.repeat(2048)}`, 'file', session, 1, 'Create expired fixture pages', `/${n}/${'p'.repeat(2048)}`);
  db.exec('COMMIT');
  while (only(db.prepare('SELECT count(*) n FROM leases').get()).n) await f.call(session, 'prune');
  db.exec('PRAGMA wal_checkpoint(TRUNCATE)');
  const before = contents(db);
  assert.ok(Number(only(db.prepare('PRAGMA freelist_count').get()).freelist_count) > 0);
  const result = runtime.retention.compact(f.file);
  assert.equal(result.compacted, true);
  assert.equal(result.deletedRecords, 0);
  assert.equal(result.integrity, 'ok');
  assert.ok(result.logicalReclaimedBytes > 0);
  assert.equal(result.after.freePages, 0);
  assert.deepEqual(contents(db), before);
  const link = join(f.workspace, 'alias.sqlite');
  symlinkSync(f.file, link);
  assert.throws(() => runtime.retention.compact(link), /regular database/);
  db.exec('CREATE TABLE unexpected(value TEXT)');
  assert.throws(() => runtime.retention.compact(f.file), /Incompatible/);
});

// --- Registry --------------------------------------------------------------------------------------------------

const registryDirectory = () => join(octocodeHome, 'agents-communication', 'active-databases');
const entries = () => existsSync(registryDirectory()) ? readdirSync(registryDirectory()).filter(name => name.endsWith('.json')).map(name => JSON.parse(readFileSync(join(registryDirectory(), name), 'utf8'))) : [];
const reset = () => rmSync(registryDirectory(), { recursive: true, force: true });

test('a writer on a non-default database registers with pid and start time; a reader announces nothing', async t => {
  reset();
  const f = fixture(t);
  assert.deepEqual(entries().map(entry => entry.database), [f.file]);
  const [entry] = entries();
  assert.equal(entry.version, 1);
  assert.ok(Number.isInteger(entry.pid) && entry.pid > 0);
  assert.ok(entry.started === null || /^\d+$/.test(entry.started));
  assert.ok(Math.abs(entry.updatedAt - Date.now()) < 60_000);
  reset();
  const reader = new Store(f.file, f.workspace, true, false);
  t.after(() => reader.close());
  await reader.call('', 'peers', {}).catch(() => {});
  assert.deepEqual(entries(), []);
});

test('a later registration prunes entries of processes that are gone', t => {
  reset();
  mkdirSync(registryDirectory(), { recursive: true });
  writeFileSync(join(registryDirectory(), '2147483646.json'), JSON.stringify({ version: 1, database: '/gone.sqlite', pid: 2147483646, started: null, updatedAt: Date.now() - 3_600_000 }));
  writeFileSync(join(registryDirectory(), 'broken.json'), 'not json');
  const f = fixture(t);
  assert.deepEqual(entries().map(entry => entry.database), [f.file]);
});

test('one peer snapshot and one view summary read a single instant', async t => {
  const f = fixture(t);
  const observer = await f.join('observer');
  await f.join('peer');
  const real = Date.now;
  const base = real();
  let reads = 0;
  // The clock jumps past every session expiry after its first reading: a second reading would change the answer.
  Date.now = () => base + (reads++ ? 400 * 86_400_000 : 0);
  t.after(() => { Date.now = real; });
  const snapshot = runtime.peers.peerSnapshot(f.store, observer);
  assert.equal(snapshot.items.length, 1);
  assert.equal(snapshot.summary.active, 1, 'items and the active count come from the same instant');
  reads = 0;
  const summary: JsonRecord = runtime.viewData.summary(f.store);
  assert.equal(summary.counts.activeAgents, 2);
  assert.equal(summary.at, base, 'the reported instant is the one the counts were read at');
});
