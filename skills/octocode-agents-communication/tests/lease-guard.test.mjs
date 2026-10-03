import {test} from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from './helpers.mjs';
import {promisify} from 'node:util';
import {mkdirSync, symlinkSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import { launcherCommand as binary, tempWorkspace, commandArgs, jsonCall } from './helpers.mjs';

const exec = promisify(execFile);
function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-guard-'), database = join(workspace, 'coord.sqlite');
  const args = commandArgs(workspace, database);
  const call = jsonCall(binary, workspace, database, {stdio: 'pipe'});
  const owner = call('join', {name: 'owner', vendor: 'raw'}).id;
  const other = call('join', {name: 'other', vendor: 'raw'}).id;
  const db = new DatabaseSync(database); t.after(() => db.close());
  const lock = (path, kind = 'file', session = owner) => call('lock', {path, kind, reasoning: 'Exercise structured edit lease admission'}, session).lease;
  const check = paths => call('check_write', {paths: paths.map(path => ({path}))}, owner);
  return {workspace, database, args, call, owner, other, db, lock, check};
}
test('declared file targets require every own live lease, including tree coverage and absent files', t => {
  const f = fixture(t); f.lock('file.txt'); f.lock('src', 'tree'); f.lock('other.txt', 'file', f.other);
  const covered = f.check(['file.txt', 'src/new.txt']);
  assert.equal(covered.ok, true); assert.equal(covered.advisory, true);
  assert.ok(covered.checks.every(row => row.covered && row.lease.expiresAt > covered.checkedAt));
  const mixed = f.check(['file.txt', 'src-extra/new.txt', 'other.txt', 'unleased.txt']);
  assert.equal(mixed.ok, false); assert.deepEqual(mixed.checks.map(row => row.covered), [true, false, false, false]);
  assert.ok(f.check(['FILE.TXT']).ok, 'Use the same conservative lease namespace as acquisition');
});
test('check_write is read-only under a held writer and does not renew presence or leases', async t => {
  const f = fixture(t); f.lock('file.txt');
  const before = f.db.prepare('PRAGMA data_version').get().data_version;
  const snapshot = JSON.stringify(f.db.prepare('SELECT * FROM leases').all());
  const expires = f.db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(f.owner).expiresAt;
  f.db.exec('BEGIN IMMEDIATE');
  try {
    const result = await exec(binary, f.args('check_write', {paths: [{path:'file.txt'}]}, f.owner), {timeout: 1500});
    assert.equal(JSON.parse(result.stdout).ok, true);
  } finally { f.db.exec('ROLLBACK'); }
  assert.equal(f.db.prepare('PRAGMA data_version').get().data_version, before);
  assert.equal(JSON.stringify(f.db.prepare('SELECT * FROM leases').all()), snapshot);
  assert.equal(f.db.prepare('SELECT expiresAt FROM sessions WHERE id=?').get(f.owner).expiresAt, expires);
});
test('expired leases and owner presence cannot authorize a write', t => {
  const f = fixture(t), lease = f.lock('file.txt');
  f.db.prepare('UPDATE leases SET expiresAt=? WHERE id=?').run(Date.now() - 1, lease.id);
  assert.equal(f.check(['file.txt']).ok, false);
  f.lock('other-live.txt');
  f.db.prepare('UPDATE sessions SET expiresAt=? WHERE id=?').run(Date.now() - 1, f.owner);
  assert.throws(() => f.check(['other-live.txt']), /expired/i);
});
test('guard rejects workspace escape, directories, malformed and oversized path sets', t => {
  const f = fixture(t); mkdirSync(join(f.workspace, 'directory'));
  for (const paths of [[], ['../escape.txt'], ['directory'], [null], Array.from({length: 33}, (_, i) => `${i}.txt`)]) assert.throws(() => f.check(paths));
  if (process.platform !== 'win32') {
    symlinkSync(tmpdir(), join(f.workspace, 'escape'));
    assert.throws(() => f.check(['escape/outside.txt']), /escapes/);
  }
});

test('host session identity and lease coverage are checked in one read snapshot', t => {
  const f = fixture(t); f.lock('file.txt');
  f.call('heartbeat', {vendorSession:'actual-host'}, f.owner);
  assert.equal(f.call('check_write', {paths:[{path:'file.txt'}],vendorSession:'actual-host'},f.owner).ok,true);
  for (const vendorSession of ['other-host',' ',null]) assert.throws(() => f.call('check_write', {paths:[{path:'file.txt'}],vendorSession},f.owner));
  assert.equal(f.check(['file.txt']).ok,true,'generic agents may omit native identity');
});

test('path checks share object inputs with file defaults while write admission remains file-only', t => {
  const f = fixture(t); f.lock('src/file.txt'); f.lock('other/file.txt', 'file', f.other);
  const paths = [{path:'src/file.txt'}, {path:'src/file.txt',kind:'file'}];
  assert.equal(f.call('check_paths', {paths}, f.owner).ok, true);
  assert.equal(f.call('check_write', {paths}, f.owner).ok, true);
  const conflicts = f.call('check_paths', {paths:[{path:'other',kind:'tree'}]}, f.owner);
  assert.equal(conflicts.ok, false); assert.equal(conflicts.conflicts.length, 1);
  for (const command of ['check_paths','check_write']) {
    for (const paths of [['src/file.txt'], [{path:'src/file.txt',kind:'directory'}], [{path:'src/file.txt',extra:true}]]) {
      assert.throws(() => f.call(command, {paths}, f.owner), /Invalid input/);
    }
  }
  assert.throws(() => f.call('check_write', {paths:[{path:'src',kind:'tree'}]}, f.owner), /Invalid input/);
});
