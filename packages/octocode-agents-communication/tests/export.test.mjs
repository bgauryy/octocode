import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { once } from 'node:events';

const root = fileURLToPath(new URL('../', import.meta.url));
const binary = process.env.COMMUNICATION_BINARY ?? join(root, 'skills/octocode-agents-communication/scripts/agents-communication');
const digest = path => createHash('sha256').update(readFileSync(path)).digest('hex');
function fixture(t) {
 const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'communication-export-')));
 t.after(() => rmSync(workspace, { recursive: true, force: true }));
 const database = join(workspace, 'source.sqlite');
 const args = values => [...values, '--database', database, '--workspace', workspace];
 const cli = values => JSON.parse(execFileSync(binary, args(values), { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
 const session = cli(['join', '{"name":"export-owner","vendor":"generic"}']).id;
 return { workspace, database, args, cli, session, export: path => cli(['db', 'export', JSON.stringify({ path })]) };
}
function openWriter(f) {
 const db = new DatabaseSync(f.database); db.exec('PRAGMA foreign_keys=ON; PRAGMA wal_autocheckpoint=0; PRAGMA busy_timeout=5000');
 return db;
}
function insert(db, session, key) {
 const id = db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning) VALUES(?,?,?,?,?,?)').run(session, session, `record ${key}`, key, Date.now() + 60000, 'Verify export consistency').lastInsertRowid;
 db.prepare('INSERT INTO deliveries(message,recipient) VALUES(?,?)').run(id, session);
 return Number(id);
}

test('export includes committed WAL and all workspaces, preserves source, verifies restore/hash/privacy', t => {
 const f = fixture(t), writer = openWriter(f);
 try {
  writer.exec('BEGIN IMMEDIATE'); insert(writer, f.session, 'wal-only');
  writer.prepare('INSERT INTO sessions(id,workspace,name,vendor,expiresAt) VALUES(?,?,?,?,?)').run('other-workspace', '/separate-workspace', 'other', 'raw', Date.now() + 60000);
  writer.exec('COMMIT');
  assert.ok(statSync(`${f.database}-wal`).size > 0);
  const sourceHash = digest(f.database), walHash = digest(`${f.database}-wal`);
  const destination = join(f.workspace, 'archive.sqlite'), result = f.export(destination);
  assert.equal(result.path, destination); assert.equal(result.source, f.database);
  assert.equal(result.scope, 'all-workspaces'); assert.equal(result.schemaVersion, 6);
  assert.equal(result.includesWorkspaceDocuments, false); assert.match(result.documents, /Preserve referenced workspace/);
  assert.equal(result.sha256, digest(destination)); assert.equal(result.bytes, statSync(destination).size);
  assert.equal(result.integrity, 'ok'); assert.equal(typeof result.directorySynced, 'boolean');
  assert.equal(digest(f.database), sourceHash); assert.equal(digest(`${f.database}-wal`), walHash);
  if (process.platform !== 'win32') assert.equal(statSync(destination).mode & 0o777, 0o600);
  const snapshot = new DatabaseSync(destination, { readOnly: true });
  assert.equal(snapshot.prepare('SELECT count(*) n FROM messages').get().n, 1);
  assert.equal(snapshot.prepare('SELECT count(*) n FROM sessions').get().n, 2);
  assert.equal(snapshot.prepare('PRAGMA integrity_check').get().integrity_check, 'ok'); snapshot.close();
  const restored = JSON.parse(execFileSync(binary, ['db', 'info', '--database', destination, '--workspace', f.workspace], { encoding: 'utf8' }));
  assert.equal(restored.compatible, true);
  assert.equal(readdirSync(f.workspace).some(name => name.startsWith('.communication-export-')), false);
 } finally { writer.close(); }
});

test('export never overwrites existing destinations, the source, or symlinks', t => {
 const f = fixture(t), destination = join(f.workspace, 'existing.sqlite');
 for (const body of ['', 'keep this content']) {
  writeFileSync(destination, body); assert.throws(() => f.export(destination), /already exists/);
  assert.equal(readFileSync(destination, 'utf8'), body);
 }
 const source = digest(f.database); assert.throws(() => f.export(f.database), /already exists/); assert.equal(digest(f.database), source);
 if (process.platform !== 'win32') {
  const link = join(f.workspace, 'alias.sqlite'); symlinkSync(destination, link);
  assert.throws(() => f.export(link), /already exists/); assert.equal(readFileSync(destination, 'utf8'), 'keep this content');
  const dangling = join(f.workspace, 'dangling.sqlite'); symlinkSync(join(f.workspace, 'missing.sqlite'), dangling);
  assert.throws(() => f.export(dangling), /already exists/); assert.equal(existsSync(join(f.workspace, 'missing.sqlite')), false);
 }
 assert.throws(() => f.export('relative.sqlite'), /absolute/);
 assert.throws(() => f.export(join(f.workspace, 'missing', 'archive.sqlite')));
 assert.equal(readdirSync(f.workspace).some(name => name.startsWith('.communication-export-')), false);
});

test('invalid schema and corrupted foreign keys fail without publishing a completed artifact', t => {
 const f = fixture(t), db = new DatabaseSync(f.database), destination = join(f.workspace, 'invalid.sqlite');
 try {
  db.exec('PRAGMA foreign_keys=OFF');
  db.prepare('INSERT INTO deliveries(message,recipient) VALUES(?,?)').run(999, f.session);
  assert.throws(() => f.export(destination), /foreign key check failed/);
  assert.equal(existsSync(destination), false);
  assert.equal(readdirSync(f.workspace).some(name => name.startsWith('.communication-export-')), false);
  db.exec('CREATE TABLE unexpected(value TEXT)');
  assert.throws(() => f.export(destination), /Incompatible/);
  assert.equal(existsSync(destination), false);
 } finally { db.close(); }
});

test('two racing exports publish one complete snapshot without clobbering', async t => {
 const f = fixture(t), destination = join(f.workspace, 'racing.sqlite');
 const run = () => new Promise((resolve, reject) => {
  const child = spawn(binary, f.args(['db', 'export', JSON.stringify({ path: destination })]));
  let output = '', stderr = ''; child.stdout.on('data', chunk => output += chunk); child.stderr.on('data', chunk => stderr += chunk);
  child.on('error', reject); child.on('close', code => resolve({ code, output, stderr }));
 });
 const results = await Promise.all([run(), run()]);
 assert.equal(results.filter(result => result.code === 0).length, 1);
 const winner = JSON.parse(results.find(result => result.code === 0).output);
 assert.equal(winner.sha256, digest(destination));
 assert.equal(readdirSync(f.workspace).some(name => name.startsWith('.communication-export-')), false);
});

test('export remains transactionally consistent while a WAL writer commits batches', async t => {
 const f = fixture(t), stop = join(f.workspace, 'stop'), destination = join(f.workspace, 'live.sqlite');
 const source = `const {DatabaseSync}=require('node:sqlite');const fs=require('fs');const db=new DatabaseSync(${JSON.stringify(f.database)});db.exec('PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA wal_autocheckpoint=0');let n=0;const insert=db.prepare('INSERT INTO messages(sender,target,body,key,expiresAt,reasoning) VALUES(?,?,?,?,?,?)');const delivery=db.prepare('INSERT INTO deliveries(message,recipient) VALUES(?,?)');function batch(){db.exec('BEGIN IMMEDIATE');for(let i=0;i<10;i++){const id=insert.run(${JSON.stringify(f.session)},${JSON.stringify(f.session)},'payload'.repeat(100),'batch-'+n++,Date.now()+60000,'Verify consistent live snapshot').lastInsertRowid;delivery.run(id,${JSON.stringify(f.session)});}db.exec('COMMIT');}batch();process.stdout.write('READY\\n');function loop(){if(fs.existsSync(${JSON.stringify(stop)})){db.close();return;}batch();setTimeout(loop,1);}loop();`;
 const writer = spawn(process.execPath, ['-e', source]); let error = ''; writer.stderr.on('data', chunk => error += chunk);
 const closed = once(writer, 'close');
 try {
  await Promise.race([once(writer.stdout, 'data'), new Promise((_, reject) => setTimeout(() => reject(Error('writer startup timeout')), 5000).unref())]);
  assert.equal(writer.exitCode, null, error);
  const result = f.export(destination); assert.equal(result.sha256, digest(destination));
  const db = new DatabaseSync(destination, { readOnly: true });
  const count = db.prepare('SELECT count(*) n FROM messages').get().n;
  assert.ok(count >= 10); assert.equal(count % 10, 0);
  assert.equal(db.prepare('SELECT count(*) n FROM deliveries').get().n, count);
  assert.equal(db.prepare("SELECT count(*) n FROM audit WHERE kind='message.created'").get().n, count);
  assert.equal(db.prepare('PRAGMA foreign_key_check').all().length, 0); db.close();
 } finally {
  writeFileSync(stop, 'stop');
  const timer = setTimeout(() => writer.kill('SIGKILL'), 5000);
  await closed; clearTimeout(timer);
  assert.equal(writer.exitCode, 0, error);
 }
});
