import { test } from 'node:test';
import type { TestContext } from 'node:test';
import type { JsonRecord } from './json-record.ts';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { binary, jsonCall, only, tempWorkspace, withReasoning } from './helpers.ts';

// Session UUIDs are routing handles, not credentials (ARCHITECTURE.md, SERVICE_PROTOCOL.md): the
// database, argv and `peers` are all readable by the same OS user, so a per-session secret would
// sit where the ID already sits. These tests pin that model and the boundaries that do hold.
function fixture(t: TestContext) {
  const workspace = tempWorkspace(t, 'communication-trust-', { real: true }), database = join(workspace, 'trust.sqlite');
  const raw = jsonCall(binary, workspace, database, { stdio: 'pipe' });
  const call = (command: string, input: Record<string, unknown> = {}, session?: string): JsonRecord => raw(command, withReasoning(command, input), session);
  const alice = call('join', { name: 'alice', vendor: 'generic' }).id, bob = call('join', { name: 'bob', vendor: 'generic' }).id;
  const db = new DatabaseSync(database); t.after(() => db.close());
  return { workspace, database, call, alice, bob, db };
}

test('a peer-visible session ID is a routing handle: whoever presents it acts as that session', t => {
  const f = fixture(t);
  assert.ok(f.call('peers', {}, f.bob).items.some((peer: JsonRecord) => peer.id === f.alice), 'peers lists the exact ID peers address');
  const sent = f.call('send_message', {parts: [{ text: 'sent with alice\'s ID' }],  to: f.bob, }, f.alice);
  assert.equal(only(f.db.prepare('SELECT sender FROM messages WHERE id=?').get(sent.id)).sender, f.alice);
  // The ID alone is the identity: the audit trail records the presented session, not the caller.
  const lease = f.call('lock', { path: '.' }, f.alice).lease;
  assert.equal(only(f.db.prepare('SELECT owner FROM leases WHERE id=?').get(lease.id)).owner, f.alice);
});

test('no tool input names the acting session, so a bound agent cannot switch identity through arguments', t => {
  const f = fixture(t);
  for (const field of ['session', 'sender', 'from', 'owner']) {
    assert.throws(() => f.call('send_message', {parts: [{ text: 'forged' }],  to: f.bob, [field]: f.alice }, f.bob), Error, `send_message must reject ${field}`);
  }
  assert.equal(only(f.db.prepare('SELECT count(*) n FROM messages').get()).n, 0);
});

test('an ID acts only in its own workspace and only while its session is live', t => {
  const f = fixture(t);
  const elsewhere = tempWorkspace(t, 'communication-trust-other-', { real: true });
  assert.throws(() => jsonCall(binary, elsewhere, f.database, { stdio: 'pipe' })('heartbeat', {}, f.alice), /Unknown or expired session in this workspace/);
  f.db.prepare('UPDATE sessions SET expiresAt=0 WHERE id=?').run(f.alice);
  assert.throws(() => f.call('send_message', {parts: [{ text: 'late' }],  to: f.bob, }, f.alice), /Unknown or expired session/);
  assert.throws(() => f.call('lock', { path: '.' }, f.alice), /Unknown or expired session/);
});
