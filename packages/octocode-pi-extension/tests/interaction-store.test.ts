import { DatabaseSync } from '../src/runtime/sqlite.js';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, expect, test } from 'vitest';
import { InteractionStore } from '../src/tools/interaction-store.js';
import type { InteractionRequestV1, InteractionAnswerV1, AuthorizationReceiptV1 } from '../src/runtime/continuity-contracts.js';
const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'pi-interaction-store-')); roots.push(root);
  const workspace = join(root, 'project'); mkdirSync(workspace);
  const database = join(root, 'interactions.sqlite3');
  const store = new InteractionStore(workspace, database);
  const stamp = new Date().toISOString();
  const request: InteractionRequestV1 = { version: 1, interactionId: 'request', workspace: store.workspace, sessionId: 'session', correlationId: 'correlation', kind: 'authorization', question: 'Start?', options: [{ id: 'start', label: 'Start' }], status: 'pending', createdAt: stamp };
  const answer: InteractionAnswerV1 = { version: 1, interactionId: request.interactionId, sessionId: request.sessionId, correlationId: request.correlationId, actor: { kind: 'user', id: 'human' }, provenance: { source: 'session-operator', trust: 'authority' }, optionIds: ['start'], createdAt: stamp };
  const receipt: AuthorizationReceiptV1 = { version: 1, receiptId: 'receipt', interactionId: request.interactionId, workspace: store.workspace, sessionId: request.sessionId, planId: 'plan', revision: 'revision', scope: ['plan.start'], actor: answer.actor, provenance: answer.provenance, createdAt: stamp };
  return { root, workspace, database, store, request, answer, receipt };
}
test('receipts survive reopen and consume once with exact revision, scope and workspace', () => {
  const f = fixture(); f.store.createInteraction(f.request); f.store.answerInteraction(f.answer); f.store.createAuthorizationReceipt(f.receipt); f.store.close();
  const reopened = new InteractionStore(f.workspace, f.database);
  const other = join(f.root, 'other'); mkdirSync(other); const foreign = new InteractionStore(other, f.database);
  const consume = { receiptId: 'receipt', planId: 'plan', revision: 'revision', scope: 'plan.start' };
  try {
    expect(() => foreign.consumeAuthorizationReceipt(consume)).toThrow(/not found/);
    expect(() => reopened.consumeAuthorizationReceipt({ ...consume, revision: 'wrong' })).toThrow(/revision/);
    expect(() => reopened.consumeAuthorizationReceipt({ ...consume, scope: 'workspace' })).toThrow(/scope/);
    expect(reopened.consumeAuthorizationReceipt(consume).consumedAt).toBeTruthy();
    expect(() => reopened.consumeAuthorizationReceipt(consume)).toThrow(/already consumed/);
    expect(() => reopened.createAuthorizationReceipt({ ...f.receipt, receiptId: 'replayed' })).toThrow(/already issued/);
  } finally { reopened.close(); foreign.close(); }
});
test('answers validate attribution and unacknowledged delivery survives reopen', () => {
  const f = fixture(); f.store.createInteraction(f.request);
  expect(() => f.store.answerInteraction({ ...f.answer, sessionId: 'other' })).toThrow(/session/);
  expect(() => f.store.answerInteraction({ ...f.answer, correlationId: 'other' })).toThrow(/correlation/);
  expect(() => f.store.answerInteraction({ ...f.answer, optionIds: ['unknown'] })).toThrow(/option/);
  f.store.answerInteraction(f.answer); const first = f.store.listEvents({ consumerId: 'host' }); expect(first).toHaveLength(1); f.store.close();
  const reopened = new InteractionStore(f.workspace, f.database);
  try {
    expect(reopened.listEvents({ consumerId: 'host' })).toEqual(first);
    reopened.acknowledgeEvent({ consumerId: 'host', eventId: first[0]!.eventId, decision: 'accept' });
    expect(reopened.listEvents({ consumerId: 'host' })).toEqual([]);
    expect(() => reopened.answerInteraction(f.answer)).toThrow(/answered/);
  } finally { reopened.close(); }
});
test('expired interactions and receipts cannot authorize work', () => {
  const f = fixture();
  try {
    f.store.createInteraction({ ...f.request, expiresAt: '2000-01-01T00:00:00.000Z' });
    expect(() => f.store.answerInteraction(f.answer)).toThrow(/expired/);
    f.store.createInteraction({ ...f.request, interactionId: 'fresh' });
    f.store.answerInteraction({ ...f.answer, interactionId: 'fresh' });
    f.store.createAuthorizationReceipt({ ...f.receipt, interactionId: 'fresh', expiresAt: '2000-01-01T00:00:00.000Z' });
    expect(() => f.store.consumeAuthorizationReceipt({ receiptId: 'receipt', planId: 'plan', revision: 'revision', scope: 'plan.start' })).toThrow(/expired/);
  } finally { f.store.close(); }
});

test.each(['version', 'schema', 'foreign'])('rejects %s drift without recreating or changing the store', kind => {
  const f = fixture(); f.store.close();
  const db = new DatabaseSync(f.database);
  if (kind === 'version') db.exec('PRAGMA user_version = 2');
  if (kind === 'schema') db.exec('DROP TABLE receipts');
  if (kind === 'foreign') db.exec('PRAGMA application_id = 99');
  db.close();
  expect(() => new InteractionStore(f.workspace, f.database)).toThrow(/version|schema|foreign/);
  if (kind === 'schema') {
    const reopened = new DatabaseSync(f.database);
    try { expect(reopened.prepare("SELECT name FROM sqlite_schema WHERE name = 'receipts'").get()).toBeUndefined(); } finally { reopened.close(); }
  }
});
