import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
const binary = fileURLToPath(new URL('../skills/octocode-agents-communication/scripts/agents-communication', import.meta.url));

test('identity updates cannot bypass staged-delivery or native attach checks', t => {
  const workspace = mkdtempSync(join(tmpdir(), 'communication-binding-')), database = join(workspace, 'db.sqlite');
  t.after(() => rmSync(workspace, {recursive:true, force:true}));
  const call = (command, input, session) => JSON.parse(execFileSync(binary, [...command.split(' '), JSON.stringify(input), '--workspace', workspace, '--database', database, ...(session ? ['--session',session] : [])], {encoding:'utf8', stdio:'pipe', timeout:10000}));
  const a = call('join', {name:'sender',vendor:'raw'}).id, b = call('join', {name:'receiver',vendor:'raw',vendorSession:'original'}).id;
  const message = call('send_message', {to:b,body:'Handle request',reasoning:'Test identity binding'}, a);
  const db = new DatabaseSync(database); t.after(() => db.close());
  db.prepare("INSERT INTO dispatches(message,recipient,token,transport,state,attemptedAt) VALUES(?,?,?,'raw','staged',?)").run(message.id,b,'test-staged',Date.now());
  assert.throws(() => call('attach', {transport:'raw'}, b), /staged delivery/, 'first attach must not bypass in-flight managed delivery');
  for (const command of ['heartbeat', `entity set session ${b}`]) {
    assert.throws(() => call(command, {vendorSession:'changed'}, b), /staged delivery/);
    assert.doesNotThrow(() => call(command, {vendorSession:'original'}, b));
  }
  assert.throws(() => call(`entity set session ${b}`, {name:'changed-name',vendorSession:null}, b));
  assert.equal(db.prepare('SELECT name FROM sessions WHERE id=?').get(b).name, 'receiver', 'partial entity update rolls back');
  db.prepare("UPDATE dispatches SET state='uncertain',error='fixture receipt unknown' WHERE token='test-staged'").run();
  assert.doesNotThrow(() => call('heartbeat', {vendorSession:'changed'}, b));
  call('attach', {transport:'codex',endpoint:'ws://127.0.0.1:19999',vendorSession:'native-thread'}, b);
  for (const command of ['heartbeat', `entity set session ${b}`]) {
    assert.throws(() => call(command, {vendorSession:'different-thread'}, b), /Use attach/);
    assert.doesNotThrow(() => call(command, {vendorSession:'native-thread'}, b));
  }
  assert.equal(db.prepare('SELECT vendorSession FROM sessions WHERE id=?').get(b).vendorSession, 'native-thread');
});
