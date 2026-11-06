import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { catalog, validation } from '../dist/runtime.js';
import { binary, execFileSync, jsonCall, tempWorkspace } from './helpers.ts';

test('default discovery is messaging with complete evidence recovery and no workflow controls', () => {
  const tools = catalog.selectedTools();
  assert.deepEqual(tools, catalog.selectedTools('messaging'));
  assert.deepEqual(tools.map(tool => tool.name), ['agents', 'messages', 'records']);
  const records = tools.find(tool => tool.name === 'records')!;
  assert.deepEqual(validation.asObject(validation.asObject(records.inputSchema.properties, 'properties').type, 'type').enum, ['read']);
  assert.throws(() => catalog.toolCommand('leases', { type: 'acquire', input: {} }), /Unknown|selected|enabled/i);
});

test('the communication CLI exposes no turn-completion gate', () => {
  const schema = JSON.parse(execFileSync(binary, ['schema'], { encoding: 'utf8' }));
  assert.equal(schema.commands.some((command: { name: string }) => command.name === 'completion-check'), false);
});

test('selective inbox recovery preserves recipient visibility and message expiry', t => {
  const workspace = tempWorkspace(t, 'communication-boundary-');
  const database = join(workspace, 'mail.sqlite');
  const call = jsonCall(binary, workspace, database);
  const sender = call('join', { name: 'sender', vendor: 'provider-a' }).id;
  const recipient = call('join', { name: 'recipient', vendor: 'provider-b' }).id;
  const send = () => call('send_message', { to: recipient, parts: [{ text: 'private evidence' }],
    reasoning: 'Verify selective message recovery' }, sender).id;
  const first = send(), second = send();
  assert.deepEqual(call('inbox', { message: second }, recipient).items.map((item: { id: number }) => item.id), [second]);
  assert.deepEqual(call('inbox', { message: first }, sender).items, []);
  assert.throws(() => call('inbox', { message: first, after: 0 }, recipient));
  const db = new DatabaseSync(database);
  t.after(() => db.close());
  db.prepare('UPDATE messages SET expiresAt=0 WHERE id=?').run(first);
  assert.deepEqual(call('inbox', { message: first }, recipient).items, []);
});
