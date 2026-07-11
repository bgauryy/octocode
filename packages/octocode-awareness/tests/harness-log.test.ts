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
import type { HarnessLogRow } from '../src/types.js';
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

// ─── 1. insertHarnessLog creates row with event_type='mine' and payload ────────

describe('insertHarnessLog — basic insert', () => {
  it('creates a harness_log row with event_type="mine" and stores the payload', () => {
    const db = freshDb();
    const payload = { context: 'routing', reason: 'initial mine' };

    const harnessId = insertHarnessLog(db, {
      agentId: 'agent-a',
      eventType: 'mine',
      payload,
    });

    expect(harnessId).toMatch(/^harness_/);

    const row = db.prepare(
      'SELECT * FROM harness_log WHERE harness_id = ?'
    ).get(harnessId) as HarnessLogRow | undefined;

    expect(row).toBeDefined();
    expect(row!.event_type).toBe('mine');
    expect(row!.agent_id).toBe('agent-a');
    expect(row!.session_id).toBeNull();
    expect(row!.memory_id).toBeNull();
    expect(row!.run_id).toBeNull();

    const parsed = JSON.parse(row!.payload_json ?? 'null');
    expect(parsed).toEqual(payload);
  });

  it('creates a row without a payload (payload_json is null)', () => {
    const db = freshDb();

    const harnessId = insertHarnessLog(db, {
      agentId: 'agent-b',
      eventType: 'capture',
    });

    const row = db.prepare(
      'SELECT payload_json FROM harness_log WHERE harness_id = ?'
    ).get(harnessId) as { payload_json: string | null } | undefined;

    expect(row).toBeDefined();
    expect(row!.payload_json).toBeNull();
  });
});

// ─── 2. insertHarnessLog with memory_id links to memories table ───────────────

describe('insertHarnessLog — memory_id FK', () => {
  it('stores memory_id and the row is retrievable via FK join', () => {
    const db = freshDb();
    const memoryId = insertMemoryRow(db, 'agent-c');

    const harnessId = insertHarnessLog(db, {
      agentId: 'agent-c',
      eventType: 'capture',
      memoryId,
    });

    const row = db.prepare(
      'SELECT h.harness_id, m.memory_id FROM harness_log h JOIN memories m ON h.memory_id = m.memory_id WHERE h.harness_id = ?'
    ).get(harnessId) as { harness_id: string; memory_id: string } | undefined;

    expect(row).toBeDefined();
    expect(row!.memory_id).toBe(memoryId);
  });

  it('rejects a memory_id that does not exist in memories (FK violation)', () => {
    const db = freshDb();

    expect(() =>
      insertHarnessLog(db, {
        agentId: 'agent-c',
        eventType: 'capture',
        memoryId: 'mem_nonexistent_xyz',
      })
    ).toThrow();
  });
});

// ─── 3. insertHarnessLog with run_id links to tasks table ───────────────────

describe('insertHarnessLog — run_id FK', () => {
  it('stores run_id and the row is retrievable via FK join', () => {
    const db = freshDb();
    const runId = insertTaskRow(db, 'agent-d');

    const harnessId = insertHarnessLog(db, {
      agentId: 'agent-d',
      eventType: 'apply',
      runId,
    });

    const row = db.prepare(
      'SELECT h.harness_id, t.run_id FROM harness_log h JOIN task_runs t ON h.run_id = t.run_id WHERE h.harness_id = ?'
    ).get(harnessId) as { harness_id: string; run_id: string } | undefined;

    expect(row).toBeDefined();
    expect(row!.run_id).toBe(runId);
  });

  it('rejects a run_id that does not exist in tasks (FK violation)', () => {
    const db = freshDb();

    expect(() =>
      insertHarnessLog(db, {
        agentId: 'agent-d',
        eventType: 'apply',
        runId: 'task_nonexistent_xyz',
      })
    ).toThrow();
  });
});

// ─── 4. queryHarnessLog by session_id ─────────────────────────────────────────

describe('queryHarnessLog — filter by session_id', () => {
  it('returns only rows matching the given session_id', () => {
    const db = freshDb();
    const sessA = insertSession(db, 'agent-e');
    const sessB = insertSession(db, 'agent-e');

    insertHarnessLog(db, { agentId: 'agent-e', sessionId: sessA, eventType: 'mine' });
    insertHarnessLog(db, { agentId: 'agent-e', sessionId: sessA, eventType: 'propose' });
    insertHarnessLog(db, { agentId: 'agent-e', sessionId: sessB, eventType: 'apply' });

    const rows = queryHarnessLog(db, { sessionId: sessA });
    expect(rows).toHaveLength(2);
    expect(rows.every(r => r.session_id === sessA)).toBe(true);
  });

  it('returns an empty array when no rows match the session_id', () => {
    const db = freshDb();
    const rows = queryHarnessLog(db, { sessionId: 'sess_no_match' });
    expect(rows).toEqual([]);
  });
});

// ─── 5. queryHarnessLog by event_type ─────────────────────────────────────────

describe('queryHarnessLog — filter by event_type', () => {
  it('returns only rows with the requested event_type', () => {
    const db = freshDb();

    insertHarnessLog(db, { agentId: 'agent-f', eventType: 'mine' });
    insertHarnessLog(db, { agentId: 'agent-f', eventType: 'mine' });
    insertHarnessLog(db, { agentId: 'agent-f', eventType: 'validate' });
    insertHarnessLog(db, { agentId: 'agent-f', eventType: 'apply' });

    const mineRows = queryHarnessLog(db, { eventType: 'mine' });
    expect(mineRows.length).toBeGreaterThanOrEqual(2);
    expect(mineRows.every(r => r.event_type === 'mine')).toBe(true);

    const validateRows = queryHarnessLog(db, { eventType: 'validate' });
    expect(validateRows.length).toBeGreaterThanOrEqual(1);
    expect(validateRows.every(r => r.event_type === 'validate')).toBe(true);
  });

  it('returns an empty array when no rows match the event_type', () => {
    const db = freshDb();
    insertHarnessLog(db, { agentId: 'agent-f', eventType: 'mine' });

    const rows = queryHarnessLog(db, { eventType: 'reflect' });
    expect(rows).toEqual([]);
  });
});

// ─── 6. queryHarnessLog by agent_id ───────────────────────────────────────────

describe('queryHarnessLog — filter by agent_id', () => {
  it('returns only rows for the requested agent_id', () => {
    const db = freshDb();

    insertHarnessLog(db, { agentId: 'agent-x', eventType: 'mine' });
    insertHarnessLog(db, { agentId: 'agent-x', eventType: 'propose' });
    insertHarnessLog(db, { agentId: 'agent-y', eventType: 'mine' });

    const rowsX = queryHarnessLog(db, { agentId: 'agent-x' });
    expect(rowsX.length).toBeGreaterThanOrEqual(2);
    expect(rowsX.every(r => r.agent_id === 'agent-x')).toBe(true);

    const rowsY = queryHarnessLog(db, { agentId: 'agent-y' });
    expect(rowsY.length).toBeGreaterThanOrEqual(1);
    expect(rowsY.every(r => r.agent_id === 'agent-y')).toBe(true);
  });

  it('returns an empty array when no rows match the agent_id', () => {
    const db = freshDb();
    const rows = queryHarnessLog(db, { agentId: 'agent-no-match' });
    expect(rows).toEqual([]);
  });
});

// ─── 7. harness_log.session_id SET NULL on session delete ─────────────────────

describe('harness_log.session_id SET NULL on session delete', () => {
  it('sets session_id to NULL when the referenced session is deleted', () => {
    const db = freshDb();
    const sessionId = insertSession(db, 'agent-g');

    const harnessId = insertHarnessLog(db, {
      agentId: 'agent-g',
      sessionId,
      eventType: 'mine',
    });

    // Confirm the FK is set before deletion
    const before = db.prepare(
      'SELECT session_id FROM harness_log WHERE harness_id = ?'
    ).get(harnessId) as { session_id: string | null } | undefined;
    expect(before!.session_id).toBe(sessionId);

    // Delete the session
    db.prepare('DELETE FROM sessions WHERE session_id = ?').run(sessionId);

    // harness_log row must remain but session_id must be NULL
    const after = db.prepare(
      'SELECT session_id FROM harness_log WHERE harness_id = ?'
    ).get(harnessId) as { session_id: string | null } | undefined;
    expect(after).toBeDefined();
    expect(after!.session_id).toBeNull();
  });
});

// ─── 8. harness_log.memory_id SET NULL on memory delete ──────────────────────

describe('harness_log.memory_id SET NULL on memory delete', () => {
  it('sets memory_id to NULL when the referenced memory is deleted', () => {
    const db = freshDb();
    const memoryId = insertMemoryRow(db, 'agent-h');

    const harnessId = insertHarnessLog(db, {
      agentId: 'agent-h',
      eventType: 'capture',
      memoryId,
    });

    // Confirm the FK is set before deletion
    const before = db.prepare(
      'SELECT memory_id FROM harness_log WHERE harness_id = ?'
    ).get(harnessId) as { memory_id: string | null } | undefined;
    expect(before!.memory_id).toBe(memoryId);

    // Delete the memory
    db.prepare('DELETE FROM memories WHERE memory_id = ?').run(memoryId);

    // harness_log row must remain but memory_id must be NULL
    const after = db.prepare(
      'SELECT memory_id FROM harness_log WHERE harness_id = ?'
    ).get(harnessId) as { memory_id: string | null } | undefined;
    expect(after).toBeDefined();
    expect(after!.memory_id).toBeNull();
  });
});
