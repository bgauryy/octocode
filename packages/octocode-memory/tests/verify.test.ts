import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { auditUnverified, markVerified } from '../src/verify.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

/** Create a PENDING intent: claim then immediately release with status PENDING. */
function makePending(
  db: DatabaseSync,
  agentId: string,
  workspacePath: string,
  testPlan = 'verify edits',
): string {
  const claim = preFlightIntent(db, {
    agentId,
    workspacePath,
    targetFiles: [`/tmp/${agentId}-target.txt`],
    testPlan,
  });
  if (!claim.ok) throw new Error('claim failed');
  releaseFileLock(db, { agentId, intentId: claim.intent.intent_id, status: 'PENDING' });
  return claim.intent.intent_id;
}

describe('auditUnverified', () => {
  it('returns empty on a fresh DB', () => {
    const db = freshDb();
    const result = auditUnverified(db);
    expect(result.ok).toBe(true);
    expect(result.count).toBe(0);
    expect(result.unverified).toEqual([]);
  });

  it('ignores ACTIVE intents — only PENDING is unverified', () => {
    const db = freshDb();
    // Claim a lock but do NOT release it → intent stays ACTIVE
    preFlightIntent(db, {
      agentId: 'agent-a',
      workspacePath: '/tmp/ws-a',
      targetFiles: ['/tmp/active.txt'],
    });
    const result = auditUnverified(db);
    expect(result.count).toBe(0);
  });

  it('returns PENDING intents with intent_id, status, and test_plan', () => {
    const db = freshDb();
    const intentId = makePending(db, 'agent-a', '/tmp/ws-a', 'run vitest + lint');
    const result = auditUnverified(db);
    expect(result.count).toBe(1);
    expect(result.unverified[0]).toMatchObject({
      intent_id: intentId,
      status: 'PENDING',
      test_plan: 'run vitest + lint',
    });
  });

  it('filters by agentId — only returns that agent\'s PENDING intents', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a', 'a-plan');
    makePending(db, 'agent-b', '/tmp/ws-b', 'b-plan');

    const result = auditUnverified(db, { agentId: 'agent-a' });
    expect(result.count).toBe(1);
    expect(result.unverified[0]!.intent_id).toBe(aId);
  });

  it('filters by workspacePath — only returns that workspace\'s PENDING intents', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a', 'a-plan');
    makePending(db, 'agent-b', '/tmp/ws-b', 'b-plan');

    const result = auditUnverified(db, { workspacePath: '/tmp/ws-a' });
    expect(result.count).toBe(1);
    expect(result.unverified[0]!.intent_id).toBe(aId);
  });

  it('filters by both agentId and workspacePath', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a', 'a-plan');
    makePending(db, 'agent-b', '/tmp/ws-b', 'b-plan');

    const result = auditUnverified(db, { agentId: 'agent-a', workspacePath: '/tmp/ws-a' });
    expect(result.count).toBe(1);
    expect(result.unverified[0]!.intent_id).toBe(aId);
  });

  it('returns all PENDING when no filter given', () => {
    const db = freshDb();
    const aId = makePending(db, 'agent-a', '/tmp/ws-a');
    const bId = makePending(db, 'agent-b', '/tmp/ws-b');
    const result = auditUnverified(db);
    expect(result.count).toBe(2);
    expect(result.unverified.map(u => u.intent_id).sort()).toEqual([aId, bId].sort());
  });
});

describe('markVerified', () => {
  it('transitions a PENDING intent to SUCCESS and clears it from auditUnverified', () => {
    const db = freshDb();
    const intentId = makePending(db, 'agent-a', '/tmp/ws-a');
    expect(auditUnverified(db).count).toBe(1);

    const result = markVerified(db, { intentId, agentId: 'agent-a', status: 'SUCCESS' });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.intent_id).toBe(intentId);
      expect(result.status).toBe('SUCCESS');
    }
    expect(auditUnverified(db).count).toBe(0);
  });

  it('transitions a PENDING intent to FAILED', () => {
    const db = freshDb();
    const intentId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { intentId, agentId: 'agent-a', status: 'FAILED' });
    expect(result.ok).toBe(true);
    const row = db.prepare('SELECT status FROM agent_intents WHERE intent_id = ?').get(intentId);
    expect((row as { status: string }).status).toBe('FAILED');
  });

  it('defaults to SUCCESS when status is omitted', () => {
    const db = freshDb();
    const intentId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { intentId, agentId: 'agent-a' });
    expect(result.ok).toBe(true);
    const row = db.prepare('SELECT status FROM agent_intents WHERE intent_id = ?').get(intentId);
    expect((row as { status: string }).status).toBe('SUCCESS');
  });

  it('returns ok=false for an unknown intent_id — not silent ok', () => {
    const db = freshDb();
    const result = markVerified(db, {
      intentId: 'intent_does-not-exist',
      agentId: 'agent-a',
      status: 'SUCCESS',
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toBeTruthy();
  });

  it('returns ok=false when the intent belongs to a different agent', () => {
    const db = freshDb();
    const intentId = makePending(db, 'agent-a', '/tmp/ws-a');
    const result = markVerified(db, { intentId, agentId: 'agent-b', status: 'SUCCESS' });
    expect(result.ok).toBe(false);
  });

  it('returns ok=false for an invalid status value', () => {
    const db = freshDb();
    const intentId = makePending(db, 'agent-a', '/tmp/ws-a');
    // PENDING is not a valid verify status (can't "verify" into PENDING)
    const result = markVerified(db, {
      intentId,
      agentId: 'agent-a',
      status: 'PENDING' as 'SUCCESS',
    });
    expect(result.ok).toBe(false);
  });

  it('returns ok=false when verifying an already-SUCCESS intent — not PENDING', () => {
    const db = freshDb();
    const intentId = makePending(db, 'agent-a', '/tmp/ws-a');
    const first = markVerified(db, { intentId, agentId: 'agent-a', status: 'SUCCESS' });
    expect(first.ok).toBe(true);
    // Second verify attempt: intent is now SUCCESS, not PENDING
    const second = markVerified(db, { intentId, agentId: 'agent-a', status: 'SUCCESS' });
    expect(second.ok).toBe(false);
  });
});
