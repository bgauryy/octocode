/**
 * harness-log.test.ts — Behavioural tests for harness_log table operations.
 *
 * Verifies:
 *  1. insertHarnessLog creates a row with event_type='mine' and payload
 *  2. insertHarnessLog with memory_id links to memories table
 *  3. insertHarnessLog with run_id links to tasks table
 *  4. queryHarnessLog filters by session_id
 *  5. queryHarnessLog filters by event_type
 *  6. queryHarnessLog filters by agent_id
 *  7. harness_log.session_id SET NULL on session delete
 *  8. harness_log.memory_id SET NULL on memory delete
 *  9. Full harness cycle: mine → propose → validate → apply events, then query all
 */
import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { initDb } from '../src/db.js';
import { insertHarnessLog, queryHarnessLog } from '../src/audit.js';
// ─── Helpers ──────────────────────────────────────────────────────────────────
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
/** Insert a session row and return its session_id. */
function insertSession(db: DatabaseSync, agentId = 'agent-test'): string {
    const sessionId = 'sess_' + randomUUID().replace(/-/g, '');
    db.prepare(`
    INSERT INTO sessions (session_id, agent_id, started_at)
    VALUES (?, ?, ?)
  `).run(sessionId, agentId, new Date().toISOString());
    return sessionId;
}
/** Insert a memory row and return its memory_id. */
function insertMemoryRow(db: DatabaseSync, agentId = 'agent-test'): string {
    const memoryId = 'mem_' + randomUUID().replace(/-/g, '');
    db.prepare(`
    INSERT INTO memories (memory_id, agent_id, task_context, observation, importance, created_at)
    VALUES (?, ?, ?, ?, ?, ?)
  `).run(memoryId, agentId, 'test task context', 'test observation', 5, new Date().toISOString());
    return memoryId;
}
/** Insert a task row and return its run_id. */
function insertTaskRow(db: DatabaseSync, agentId = 'agent-test'): string {
    const runId = 'task_' + randomUUID().replace(/-/g, '');
    db.prepare(`
    INSERT INTO task_runs (run_id, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
  `).run(runId, agentId, 'test rationale', 'yarn test', 'ACTIVE', '/workspace', new Date().toISOString(), new Date().toISOString());
    return runId;
}

// ─── 9. Full harness cycle ─────────────────────────────────────────────────────

describe('Full harness cycle: mine → propose → validate → apply', () => {
  it('inserts all four events and queryHarnessLog returns them all for the session', () => {
    const db = freshDb();
    const sessionId = insertSession(db, 'agent-cycle');
    const memoryId = insertMemoryRow(db, 'agent-cycle');
    const runId = insertTaskRow(db, 'agent-cycle');

    const mineId = insertHarnessLog(db, {
      agentId: 'agent-cycle',
      sessionId,
      eventType: 'mine',
      payload: { step: 1, context: 'initial scan' },
    });

    const proposeId = insertHarnessLog(db, {
      agentId: 'agent-cycle',
      sessionId,
      eventType: 'propose',
      payload: { step: 2, proposal: 'edit src/auth.ts' },
    });

    const validateId = insertHarnessLog(db, {
      agentId: 'agent-cycle',
      sessionId,
      eventType: 'validate',
      runId,
      payload: { step: 3, verdict: 'ok' },
    });

    const applyId = insertHarnessLog(db, {
      agentId: 'agent-cycle',
      sessionId,
      eventType: 'apply',
      runId,
      memoryId,
      payload: { step: 4, files: ['src/auth.ts'] },
    });

    const rows = queryHarnessLog(db, { sessionId });
    expect(rows).toHaveLength(4);

    const ids = rows.map(r => r.harness_id);
    expect(ids).toContain(mineId);
    expect(ids).toContain(proposeId);
    expect(ids).toContain(validateId);
    expect(ids).toContain(applyId);

    // Verify event type distribution
    const eventTypes = rows.map(r => r.event_type).sort();
    expect(eventTypes).toEqual(['apply', 'mine', 'propose', 'validate']);

    // Verify the apply row carries both run_id and memory_id
    const applyRow = rows.find(r => r.harness_id === applyId)!;
    expect(applyRow.run_id).toBe(runId);
    expect(applyRow.memory_id).toBe(memoryId);

    // Verify payload round-trip on the mine event
    const mineRow = rows.find(r => r.harness_id === mineId)!;
    const minePayload = JSON.parse(mineRow.payload_json ?? 'null');
    expect(minePayload).toEqual({ step: 1, context: 'initial scan' });
  });

  it('queryHarnessLog with no filters returns all rows across all agents', () => {
    const db = freshDb();

    insertHarnessLog(db, { agentId: 'agent-1', eventType: 'mine' });
    insertHarnessLog(db, { agentId: 'agent-2', eventType: 'reflect' });

    const all = queryHarnessLog(db, {});
    expect(all.length).toBeGreaterThanOrEqual(2);
  });
});
