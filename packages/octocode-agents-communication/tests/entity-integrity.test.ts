import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, jsonCall, tempWorkspace } from './helpers.ts';

function fixture(t: Parameters<typeof tempWorkspace>[0]) {
  const workspace = tempWorkspace(t, 'communication-entities-'), database = join(workspace, 'mail.sqlite');
  const call = jsonCall(binary, workspace, database);
  const identity = call('join', { name: 'known-agent', vendor: 'provider', vendorSession: 'native-id' });
  const db = new DatabaseSync(database, { enableForeignKeyConstraints: true });
  t.after(() => db.close());
  return { workspace, call, identity, db };
}

test('all operational entities use strict SQLite types and enforce foreign keys', t => {
  const { db } = fixture(t);
  const tables = db.prepare("SELECT name,strict FROM pragma_table_list WHERE schema='main' AND type='table' AND name NOT LIKE 'sqlite_%'").all();
  assert.ok(tables.length > 0);
  for (const table of tables) assert.equal(table.strict, 1, `${String(table.name)} must enforce its declared types`);
  assert.equal(db.prepare('PRAGMA foreign_keys').get()!.foreign_keys, 1);
  assert.deepEqual(db.prepare('PRAGMA foreign_key_check').all(), []);
});

test('an agent cannot change its durable ID, provider or workspace', t => {
  const { db, identity } = fixture(t);
  for (const field of ['vendor', 'workspace', 'id']) {
    assert.throws(() => db.prepare(`UPDATE sessions SET ${field}=? WHERE id=?`).run('different', identity.id), /immutable|FOREIGN KEY/i);
  }
});

test('session entities require a registered workspace and nonblank identity', t => {
  const { db, workspace } = fixture(t);
  const insert = db.prepare('INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES(?,?,?,?,?)');
  assert.throws(() => insert.run('orphan', '/unregistered', 'peer', 'provider', Date.now() + 60000), /FOREIGN KEY/);
  for (const [id, name, vendor] of [['', 'peer', 'provider'], ['blank-name', ' ', 'provider'], ['blank-vendor', 'peer', ' ']]) {
    assert.throws(() => insert.run(id, workspace, name, vendor, Date.now() + 60000), /CHECK/);
  }
});

test('path ownership cannot be stored under a workspace other than its owner', t => {
  const { db, workspace, identity } = fixture(t);
  const other = `${workspace}/other`;
  db.prepare('INSERT INTO workspaces VALUES(?,?)').run(other, other);
  assert.throws(() => db.prepare('INSERT INTO leases(workspace,path,kind,owner,expiresAt,reasoning,pathKey) VALUES(?,?,?,?,?,?,?)')
    .run(other, `${other}/file`, 'file', identity.id, Date.now() + 60000, 'Verify ownership scope', '/file'), /FOREIGN KEY/);
  assert.throws(() => db.prepare('INSERT INTO lease_waits(workspace,owner,targets,reasoning,leaseTtlMs,createdAt,expiresAt) VALUES(?,?,?,?,?,?,?)')
    .run(other, identity.id, '[{"path":"file"}]', 'Verify queued ownership scope', 60000, Date.now(), Date.now() + 60000), /FOREIGN KEY/);
});

test('delivery recipients must share the message sender repository scope', t => {
  const { db, workspace, identity, call } = fixture(t);
  const peer = call('join', { name: 'peer', vendor: 'other-provider' }).id;
  const sent = call('send_message', { to: peer, parts: [{ text: 'Scoped' }], reasoning: 'Verify recipient scope' }, identity.id);
  const other = `${workspace}/other`;
  db.prepare('INSERT INTO workspaces VALUES(?,?)').run(other, other);
  db.prepare('INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES(?,?,?,?,?)').run('outside', other, 'outsider', 'other', Date.now() + 60000);
  assert.throws(() => db.prepare('INSERT INTO deliveries(message,recipient) VALUES(?,?)').run(sent.id, 'outside'), /repository scope/i);
});
