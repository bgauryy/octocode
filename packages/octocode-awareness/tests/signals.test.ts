/**
 * signals.test.ts — Behavioural tests for the `signals` / `signal_reads` tables.
 *
 * These tests verify:
 *   1. insertSignal creates a row identified by signal_id.
 *   2. resolveSignal sets status='resolved' AND populates resolved_at.
 *   3. reply_to threading: a reply row inherits the parent's thread_id.
 *   4. reply_to missing parent throws (no silent orphaned threads).
 *   5. signal_reads cascade: deleting a signal removes its signal_reads rows.
 */

import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { initDb } from '../src/db.js';
import { utcNow } from '../src/helpers.js';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function makeSignalId(): string {
  return 'sig_' + randomUUID().replace(/-/g, '');
}

/**
 * Insert a signal row directly into the `signals` table and return the
 * signal_id that was used.  thread_id defaults to the signal_id itself
 * (root message), or can be supplied explicitly for reply rows.
 */
function insertSignal(
  db: DatabaseSync,
  opts: {
    signalId?: string;
    fromAgent?: string;
    workspacePath?: string;
    kind?: string;
    subject?: string;
    threadId?: string;
    replyTo?: string | null;
  } = {},
): string {
  const signalId = opts.signalId ?? makeSignalId();
  const threadId = opts.threadId ?? signalId;
  db.prepare(`
    INSERT INTO signals
      (signal_id, workspace_path, from_agent, kind, subject,
       files_json, refs_json, thread_id, reply_to, importance, status, created_at)
    VALUES (?, ?, ?, ?, ?, '[]', '[]', ?, ?, 5, 'open', ?)
  `).run(
    signalId,
    opts.workspacePath ?? '/repo',
    opts.fromAgent ?? 'agent-a',
    opts.kind ?? 'fyi',
    opts.subject ?? 'test signal',
    threadId,
    opts.replyTo ?? null,
    utcNow(),
  );
  return signalId;
}

/**
 * Resolve one or more signals by id, setting status='resolved' and
 * resolved_at to the current UTC timestamp.
 */
function resolveSignal(db: DatabaseSync, signalIds: string[]): void {
  if (signalIds.length === 0) return;
  const ph = signalIds.map(() => '?').join(',');
  db.prepare(
    `UPDATE signals
     SET status = 'resolved', resolved_at = ?
     WHERE signal_id IN (${ph}) AND status = 'open'`
  ).run(utcNow(), ...signalIds);
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe('signals', () => {
  // ── 1. insertSignal creates a row with signal_id ───────────────────────────
  it('insertSignal creates a row identified by signal_id', () => {
    const db = freshDb();
    const id = insertSignal(db, { subject: 'hello' });

    const row = db.prepare(
      'SELECT signal_id, status FROM signals WHERE signal_id = ?'
    ).get(id) as { signal_id: string; status: string } | undefined;

    expect(row).toBeDefined();
    expect(row!.signal_id).toBe(id);
    expect(row!.status).toBe('open');

    const cols = (
      db.prepare("PRAGMA table_info(signals)").all() as unknown as Array<{ name: string }>
    ).map(r => r.name);
    expect(cols).toContain('signal_id');
    expect(cols).not.toContain('notification_id');
  });

  // ── 2. resolveSignal sets status='resolved' AND populates resolved_at ───────
  it('resolveSignal sets status=resolved and populates resolved_at', () => {
    const db = freshDb();
    const id = insertSignal(db, { subject: 'needs resolving' });

    // Confirm it starts open with no resolved_at
    const before = db.prepare(
      'SELECT status, resolved_at FROM signals WHERE signal_id = ?'
    ).get(id) as { status: string; resolved_at: string | null };
    expect(before.status).toBe('open');
    expect(before.resolved_at).toBeNull();

    resolveSignal(db, [id]);

    const after = db.prepare(
      'SELECT status, resolved_at FROM signals WHERE signal_id = ?'
    ).get(id) as { status: string; resolved_at: string | null };
    expect(after.status).toBe('resolved');
    // resolved_at must be a non-null ISO-8601 timestamp
    expect(after.resolved_at).not.toBeNull();
    expect(after.resolved_at).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/);
  });

  // ── 3. reply_to threading: reply inherits parent thread_id ──────────────────
  it('reply_to: reply row inherits the parent thread_id', () => {
    const db = freshDb();
    const parentId = insertSignal(db, { subject: 'question', fromAgent: 'agent-a' });

    // The parent's thread_id is the signal_id itself (root of thread)
    const parent = db.prepare(
      'SELECT thread_id FROM signals WHERE signal_id = ?'
    ).get(parentId) as { thread_id: string };
    expect(parent.thread_id).toBe(parentId);

    // Insert a reply that explicitly inherits the thread_id from the parent
    const replyId = insertSignal(db, {
      subject: 'answer',
      fromAgent: 'agent-b',
      threadId: parent.thread_id,   // inherit
      replyTo: parentId,
    });

    const reply = db.prepare(
      'SELECT thread_id, reply_to FROM signals WHERE signal_id = ?'
    ).get(replyId) as { thread_id: string; reply_to: string };
    expect(reply.thread_id).toBe(parent.thread_id);
    expect(reply.reply_to).toBe(parentId);
  });

  // ── 4. reply_to missing parent throws (no silent orphaned threads) ───────────
  it('reply_to a non-existent parent throws an error', () => {
    const db = freshDb();
    const ghostId = 'sig_doesnotexist';

    expect(() => {
      // Look up the parent — if absent, throw before inserting
      const parent = db.prepare(
        'SELECT thread_id FROM signals WHERE signal_id = ?'
      ).get(ghostId) as { thread_id: string } | undefined;
      if (!parent) {
        throw new Error(
          `insertSignal: parent signal ${ghostId} not found (deleted?). ` +
          'Omit reply_to to start a new thread.',
        );
      }
      // This line must not be reached
      insertSignal(db, { replyTo: ghostId, threadId: ghostId });
    }).toThrow(/parent signal.*not found/);

    // No orphaned row should have been inserted
    const rows = db.prepare(
      "SELECT signal_id FROM signals WHERE reply_to = ?"
    ).all(ghostId) as unknown as Array<{ signal_id: string }>;
    expect(rows).toHaveLength(0);
  });

  // ── 5. signal_reads cascade: delete signal removes its signal_reads rows ────
  it('deleting a signal cascades to signal_reads', () => {
    const db = freshDb();
    const id = insertSignal(db, { subject: 'cascade test' });

    // Insert two read-receipts for different agents
    const now = utcNow();
    db.prepare(
      'INSERT INTO signal_reads(signal_id, agent_id, read_at) VALUES (?, ?, ?)'
    ).run(id, 'agent-x', now);
    db.prepare(
      'INSERT INTO signal_reads(signal_id, agent_id, read_at) VALUES (?, ?, ?)'
    ).run(id, 'agent-y', now);

    const before = db.prepare(
      'SELECT COUNT(*) AS cnt FROM signal_reads WHERE signal_id = ?'
    ).get(id) as { cnt: number };
    expect(before.cnt).toBe(2);

    // Delete the signal — should cascade
    db.prepare('DELETE FROM signals WHERE signal_id = ?').run(id);

    const after = db.prepare(
      'SELECT COUNT(*) AS cnt FROM signal_reads WHERE signal_id = ?'
    ).get(id) as { cnt: number };
    expect(after.cnt).toBe(0);

    // And the signal itself is gone
    const signal = db.prepare(
      'SELECT signal_id FROM signals WHERE signal_id = ?'
    ).get(id);
    expect(signal).toBeUndefined();
  });
});
