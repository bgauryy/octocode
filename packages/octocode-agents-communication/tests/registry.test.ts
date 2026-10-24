import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, writeFileSync, mkdirSync, utimesSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, execFileSync, jsonCall, octocodeHome, only, registryDirectory, registryEntries, resetRegistry, tempWorkspace } from './helpers.ts';

test('a session on --database <file> is registered with pid and start time, and one on the default database is not', t => {
  resetRegistry();
  const workspace = tempWorkspace(t, 'communication-registry-', { real: true }), database = join(workspace, 'custom.sqlite');
  const call = jsonCall(binary, workspace, database, { stdio: 'pipe' });
  const { id } = call('join', { name: 'registered', vendor: 'generic' });
  assert.deepEqual(registryEntries().map(entry => entry.database), [database]);
  const [entry] = registryEntries();
  assert.equal(entry.version, 1);
  assert.ok(Number.isInteger(entry.pid) && entry.pid > 0);
  assert.ok(entry.started === null || /^\d+$/.test(entry.started));
  assert.ok(Math.abs(entry.updatedAt - Date.now()) < 60_000);
  // Reads announce nothing.
  resetRegistry();
  call('peers', {}, id);
  call('fetch', {}, id);
  assert.deepEqual(registryEntries(), []);
  // The default database is found without a registry.
  const home = join(octocodeHome, 'agents-communication');
  mkdirSync(home, { recursive: true });
  const fallback = JSON.parse(execFileSync(binary, ['join', JSON.stringify({ name: 'default-db', vendor: 'generic' }), '--workspace', workspace], { encoding: 'utf8', stdio: 'pipe' }));
  const fallbackDatabase = join(home, 'communication.sqlite');
  const db = new DatabaseSync(fallbackDatabase, { readOnly: true });
  t.after(() => db.close());
  assert.equal(only(db.prepare('SELECT id FROM sessions WHERE name=?').get('default-db')).id, fallback.id);
  assert.deepEqual(registryEntries(), []);
});

test('a later write prunes entries of processes that are gone', t => {
  resetRegistry();
  mkdirSync(registryDirectory, { recursive: true });
  const old = Date.now() - 3_600_000;
  writeFileSync(join(registryDirectory, '2147483646.json'), JSON.stringify({ version: 1, database: '/gone.sqlite', pid: 2147483646, started: null, updatedAt: old }));
  writeFileSync(join(registryDirectory, 'broken.json'), 'not json');
  const workspace = tempWorkspace(t, 'communication-registry-prune-', { real: true }), database = join(workspace, 'custom.sqlite');
  jsonCall(binary, workspace, database, { stdio: 'pipe' })('join', { name: 'pruner', vendor: 'generic' });
  assert.deepEqual(registryEntries().map(entry => entry.database), [database]);
});

test('a later write removes temporary files a crashed writer left, but not a fresh one in flight', t => {
  resetRegistry();
  mkdirSync(registryDirectory, { recursive: true });
  const stale = join(registryDirectory, '.entry-2147483646-1.tmp'), fresh = join(registryDirectory, '.entry-2147483645-2.tmp');
  for (const file of [stale, fresh]) writeFileSync(file, '{}');
  utimesSync(stale, new Date(Date.now() - 3_600_000), new Date(Date.now() - 3_600_000));
  const workspace = tempWorkspace(t, 'communication-registry-tmp-', { real: true }), database = join(workspace, 'custom.sqlite');
  jsonCall(binary, workspace, database, { stdio: 'pipe' })('join', { name: 'pruner', vendor: 'generic' });
  assert.equal(existsSync(stale), false);
  assert.equal(existsSync(fresh), true);
});
