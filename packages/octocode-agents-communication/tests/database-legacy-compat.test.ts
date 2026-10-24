// A database written by the previous runtime (fixtures/legacy-database.sqlite; workspace /usr, every expiry in 2100) is
// read and extended by the TypeScript store, and the reads equal the outputs the previous runtime gave for it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { copyFileSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { JsonRecord } from './json-record.ts';
import { tempWorkspace } from './helpers.ts';
import { Store } from './store-support.ts';

const legacy = JSON.parse(readFileSync(new URL('./fixtures/legacy-database.json', import.meta.url), 'utf8'));

test('Given a database written by the previous runtime, when the TypeScript store reads and writes it, then reads equal the recorded outputs', async t => {
  const workspace = tempWorkspace(t, 'database-legacy-compat-', { real: true }), database = join(workspace, 'legacy.sqlite');
  copyFileSync(new URL('./fixtures/legacy-database.sqlite', import.meta.url), database);
  const { agent: alice, peer: bob } = legacy;

  const store = new Store(database, legacy.info.workspace, false, false);
  t.after(() => store.close());
  const strip = (value: unknown): JsonRecord => JSON.parse(JSON.stringify(value));
  assert.deepEqual(strip(await store.call(bob, 'inbox', {})), legacy.inbox);
  assert.deepEqual(strip(await store.call(alice, 'locks', {})), legacy.locks);
  assert.deepEqual(strip(await store.call(bob, 'binding', {})), legacy.binding);
  assert.deepEqual(strip(await store.call(alice, 'peers', {})), legacy.peers);
  const task = legacy.inbox.items.find((item: JsonRecord) => item.kind === 'task');
  const done: JsonRecord = await store.call(bob, 'complete', { message: task.id, outcome: 'done', reply: { conclusion: 'Looks right' }, reasoning: 'Verify TypeScript completion on a legacy database' });
  assert.equal(done.completed, true);
  const reply = (await store.call(alice, 'inbox', {})).items.find((item: JsonRecord) => item.id === done.id);
  assert.ok(reply);
  assert.equal(reply.kind, 'answer');
  assert.match(reply.body, /\nConclusion: Looks right$/);
  assert.equal((await store.call(bob, 'inbox', {})).items.length, legacy.inbox.items.length - 1);
});
