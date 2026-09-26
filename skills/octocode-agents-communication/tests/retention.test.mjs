import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { existsSync, symlinkSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { launcherCommand as binary, tempWorkspace } from './helpers.mjs';

const exec = promisify(execFile);
const tables = ['sessions', 'subscriptions', 'leases', 'messages', 'deliveries', 'attachments', 'dispatches', 'audit', 'sqlite_sequence'];
const contents = db => Object.fromEntries(tables.map(table => [table, db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all()]));
function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-retention-', { real: true });
  const database = join(workspace, 'source.sqlite');
  const args = values => [...values, '--database', database, '--workspace', workspace];
  const call = values => JSON.parse(execFileSync(binary, args(values), { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
  const session = call(['join', '{"name":"retention-owner","vendor":"generic"}']).id;
  const db = new DatabaseSync(database); db.exec('PRAGMA foreign_keys=ON');
  t.after(() => db.close());
  return { workspace, database, args, call, session, db };
}
function message(f, key, { acknowledged = true, uncertain = false, expiresAt = 1, replyTo = null } = {}) {
  const id = Number(f.db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning,replyTo) VALUES(?,?,?,?,?,?,?)')
    .run(f.session, f.session, `audit body ${key}`, key, expiresAt, 'Keep complete protocol evidence', replyTo).lastInsertRowid);
  f.db.prepare('INSERT INTO deliveries(message,recipient,acknowledgedAt) VALUES(?,?,?)').run(id, f.session, acknowledged ? Date.now() : null);
  if (uncertain) f.db.prepare('INSERT INTO dispatches(message,recipient,token,transport,state,attemptedAt) VALUES(?,?,?,?,?,?)').run(id, f.session, 'attempt', 'raw', 'uncertain', Date.now());
  return id;
}

test('retention report is bounded, paginated and preserves pending/uncertain/correlated history', t => {
  const f = fixture(t), parent = message(f, 'settled');
  message(f, 'pending', { acknowledged: false });
  message(f, 'uncertain', { uncertain: true });
  message(f, 'future', { expiresAt: Date.now() + 60000 });
  message(f, 'reply', { replyTo: parent });
  const before = contents(f.db);
  const one = f.call(['db', 'retention', '{"limit":3}']);
  assert.equal(one.readOnly, true); assert.equal(one.deletionSupported, false);
  assert.equal(one.page.count, 3); assert.equal(one.page.hasMore, true);
  assert.equal(one.page.expiredSettledMessages, 1);
  assert.equal(one.page.unacknowledgedMessages, 1); assert.equal(one.page.unresolvedDispatchMessages, 1);
  const two = f.call(['db', 'retention', JSON.stringify(one.next.input)]);
  assert.equal(two.page.count, 2); assert.equal(two.page.expiredSettledMessages, 1); assert.equal(two.next ?? null, null);
  assert.equal(two.before, one.before, 'Pagination freezes the original expiry cutoff');
  assert.deepEqual(contents(f.db), before);
});

test('retention validates boundaries and never creates a missing database', t => {
  const f = fixture(t);
  for (const input of [{ limit: 0 }, { limit: 1001 }, { before: Date.now() + 60000 }, { afterId: -1 }, { delete: true }]) {
    assert.throws(() => f.call(['db', 'retention', JSON.stringify(input)]));
  }
  const missing = join(f.workspace, 'absent.sqlite');
  assert.throws(() => execFileSync(binary, ['db', 'retention', '{}', '--database', missing, '--workspace', f.workspace], { stdio: 'pipe' }));
  assert.equal(existsSync(missing), false);
});

test('compaction reclaims pages freed by expired lease pruning without removing any protocol record', t => {
  const f = fixture(t);
  const parent = message(f, 'retain-key'); message(f, 'retain-reply', { replyTo: parent });
  message(f, 'retain-uncertain', { acknowledged: false, uncertain: true });
  const lease = f.db.prepare('INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning,pathKey) VALUES(?,?,?,?,?,?,?)');
  f.db.exec('BEGIN');
  for (let n = 0; n < 400; n++) lease.run(f.workspace, `${n}/${'p'.repeat(2048)}`, 'file', f.session, 1, 'Create expired fixture pages', `/${n}/${'p'.repeat(2048)}`);
  f.db.exec('COMMIT');
  while (f.db.prepare('SELECT count(*) n FROM leases').get().n) f.call(['prune', '{}']);
  f.db.exec('PRAGMA wal_checkpoint(TRUNCATE)');
  const before = contents(f.db);
  assert.ok(f.db.prepare('PRAGMA freelist_count').get().freelist_count > 0);
  const started = performance.now();
  const result = f.call(['db', 'compact', '{}']);
  t.diagnostic(JSON.stringify({ fixture: '400-expired-leases', cliElapsedMs: performance.now() - started,
    retainedAuditRows: before.audit.length, logicalBytesBefore: result.before.logicalBytes,
    logicalBytesAfter: result.after.logicalBytes, reclaimedBytes: result.reclaimedBytes }));
  assert.equal(result.compacted, true); assert.equal(result.deletedRecords, 0); assert.equal(result.integrity, 'ok');
  assert.ok(result.logicalReclaimedBytes > 0); assert.ok(result.reclaimedBytes >= 0);
  assert.equal(result.after.freePages, 0);
  assert.deepEqual(contents(f.db), before, 'Includes audit rows, message bodies, keys, replyTo, dispatches and IDs');
  assert.throws(() => f.db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning) VALUES(?,?,?,?,?,?)')
    .run(f.session, f.session, 'changed', 'retain-key', 1, 'Cannot reuse historical key'), /UNIQUE/);
  assert.equal(f.db.prepare('SELECT replyTo FROM messages WHERE key=?').get('retain-reply').replyTo, parent);
});

test('compaction fails boundedly under a writer lock and keeps all records', async t => {
  const f = fixture(t); message(f, 'locked');
  const before = contents(f.db);
  f.db.exec('BEGIN IMMEDIATE');
  try {
    const started = performance.now();
    await assert.rejects(exec(binary, f.args(['db', 'compact', '{}']), { timeout: 9000 }), /locked|busy/i);
    assert.ok(performance.now() - started < 8500);
  } finally { f.db.exec('ROLLBACK'); }
  assert.deepEqual(contents(f.db), before);
});

test('compaction rejects schema drift and symlinks instead of repairing or replacing databases', t => {
  const f = fixture(t);
  if (process.platform !== 'win32') {
    const link = join(f.workspace, 'alias.sqlite'); symlinkSync(f.database, link);
    assert.throws(() => execFileSync(binary, ['db', 'compact', '{}', '--database', link, '--workspace', f.workspace], { stdio: 'pipe' }), /regular database/);
  }
  f.db.exec('CREATE TABLE unexpected(value TEXT)');
  assert.throws(() => f.call(['db', 'compact', '{}']), /Incompatible/);
});
