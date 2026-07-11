import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
/**
 * maintenance.test.ts — Behavioural tests for maintenance functions against the current schema.
 *
 * Core tables: memories, tasks, locks.
 * Core columns: importance, run_id, tags_json, memory_refs.
 */
import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb, connectDb, checkpointWal } from '../src/db.js';
import { journalModeForSqliteVersion } from '../src/sqlite-runtime.js';
import { sessionCapture, parseGitStatusShortLines, digest } from '../src/maintenance.js';
// ─── Helpers ──────────────────────────────────────────────────────────────────
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}

// ─── 6. digest — works with new schema tables ──────────────────────────────────

describe('digest — dry_run with new schema', () => {
  it('dry_run returns counts without mutating', () => {
    const db = freshDb();

    // Add a SUPERSEDED memory older than 90d
    const oldDate = new Date(Date.now() - 91 * 86400000).toISOString();
    db.prepare(`
      INSERT INTO memories (memory_id, agent_id, task_context, observation, importance, state, created_at, updated_at)
      VALUES ('mem_old', 'agent-x', 'old task', 'old observation', 3, 'SUPERSEDED', ?, ?)
    `).run(oldDate, oldDate);

    const res = digest(db, { dry_run: true });
    expect(res.ok).toBe(true);
    expect(res.dry_run).toBe(true);
    expect(typeof res.would_prune_old).toBe('number');
    expect(res.candidate_limit).toBe(20);
    expect(res.candidate_ids?.purge_memory_ids).toEqual(['mem_old']);

    // Nothing deleted in dry_run
    const row = db.prepare("SELECT state FROM memories WHERE memory_id = 'mem_old'").get() as { state: string } | undefined;
    expect(row?.state).toBe('SUPERSEDED');
  });

  it('rejects invalid retention windows before mutating', () => {
    const db = freshDb();
    const oldDate = new Date(Date.now() - 91 * 86400000).toISOString();
    db.prepare(`
      INSERT INTO memories (memory_id, agent_id, task_context, observation, importance, state, created_at, updated_at)
      VALUES ('mem_retention_sentinel', 'agent-x', 'sentinel', 'must survive invalid config', 3, 'SUPERSEDED', ?, ?)
    `).run(oldDate, oldDate);

    for (const key of [
      'retention_days',
      'refinement_handoff_retention_days',
      'refinement_done_retention_days',
      'operational_retention_days',
      'pressure_age_days',
    ]) {
      for (const value of [0, -1, 3651, 1.5, Number.NaN]) {
        expect(() => digest(db, { [key]: value })).toThrow(/1\.\.3650/);
        expect(db.prepare('SELECT state FROM memories WHERE memory_id = ?').get('mem_retention_sentinel'))
          .toEqual({ state: 'SUPERSEDED' });
      }
    }
  });

  it('scopes cleanup to the requested workspace', () => {
    const db = freshDb();
    const oldDate = new Date(Date.now() - 91 * 86400000).toISOString();
    db.prepare(`
      INSERT INTO memories (memory_id, agent_id, task_context, observation, importance, state, workspace_path, created_at, updated_at)
      VALUES ('mem_ws_a_old', 'agent-x', 'old a', 'old a observation', 3, 'SUPERSEDED', '/ws-a', ?, ?)
    `).run(oldDate, oldDate);
    db.prepare(`
      INSERT INTO memories (memory_id, agent_id, task_context, observation, importance, state, workspace_path, created_at, updated_at)
      VALUES ('mem_ws_b_old', 'agent-x', 'old b', 'old b observation', 3, 'SUPERSEDED', '/ws-b', ?, ?)
    `).run(oldDate, oldDate);

    const dry = digest(db, { workspace: '/ws-a', dry_run: true });
    expect(dry.would_prune_old).toBe(1);

    const res = digest(db, { workspace: '/ws-a' });
    expect(res.pruned_old).toBe(1);
    const remaining = db.prepare('SELECT memory_id FROM memories ORDER BY memory_id').all() as Array<{ memory_id: string }>;
    expect(remaining.map(row => row.memory_id)).toEqual(['mem_ws_b_old']);
  });

  it('rolls back all cleanup when FTS reconciliation fails', () => {
    const db = freshDb();
    const oldDate = new Date(Date.now() - 91 * 86400000).toISOString();
    db.prepare(`
      INSERT INTO memories (memory_id, agent_id, task_context, observation, importance, state, created_at, updated_at)
      VALUES ('mem_digest_rollback_old', 'agent-x', 'old', 'old observation', 3, 'SUPERSEDED', ?, ?)
    `).run(oldDate, oldDate);
    db.prepare(`
      INSERT INTO memories (memory_id, agent_id, task_context, observation, importance, state, created_at, updated_at)
      VALUES ('mem_digest_rollback_active', 'agent-x', 'active', 'active observation', 8, 'ACTIVE', ?, ?)
    `).run(oldDate, oldDate);
    db.exec(`
      DROP TABLE memories_fts;
      CREATE TABLE memories_fts (
        memory_id TEXT CHECK(memory_id = 'never-allowed'),
        task_context TEXT,
        observation TEXT,
        tags TEXT
      );
    `);

    expect(() => digest(db, {})).toThrow();
    expect(db.prepare('SELECT state FROM memories WHERE memory_id = ?')
      .get('mem_digest_rollback_old')).toEqual({ state: 'SUPERSEDED' });
  });

  it('checkpoint and digest complete on the runtime-safe file journal', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-wal-'));
    try {
      const dbPath = join(dir, 'awareness.sqlite3');
      const db = connectDb(dbPath);
      const mode = db.prepare('PRAGMA journal_mode').get() as { journal_mode: string };
      const sqliteVersion = (db.prepare('SELECT sqlite_version() AS version').get() as { version: string }).version;
      expect(String(mode.journal_mode).toUpperCase()).toBe(journalModeForSqliteVersion(sqliteVersion));
      expect(() => checkpointWal(db)).not.toThrow();
      const oldDate = new Date(Date.now() - 91 * 86400000).toISOString();
      db.prepare(`
        INSERT INTO memories (memory_id, agent_id, task_context, observation, importance, state, created_at, updated_at)
        VALUES ('mem_wal_old', 'agent-x', 'old', 'old observation', 3, 'SUPERSEDED', ?, ?)
      `).run(oldDate, oldDate);
      const res = digest(db, {});
      expect(res.ok).toBe(true);
      expect(res.pruned_old).toBe(1);
      expect(() => checkpointWal(db)).not.toThrow();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe('parseGitStatusShortLines', () => {
  it('keeps leading-space modified paths intact', () => {
    expect(parseGitStatusShortLines(' M file1.txt\n')).toEqual(['file1.txt']);
  });
  it('parses untracked and deleted', () => {
    expect(parseGitStatusShortLines('?? new.ts\nD  gone.ts\n')).toEqual(['new.ts', 'gone.ts']);
  });
  it('keeps rename destination', () => {
    expect(parseGitStatusShortLines('R  old.ts -> new.ts\n')).toEqual(['new.ts']);
  });
});

describe('sessionCapture dirty git paths', () => {
  it('captures dirty git paths without truncating porcelain columns', () => {
    const db = freshDb();
    const dir = mkdtempSync(join(tmpdir(), 'oc-session-dirty-'));
    try {
      execFileSync('git', ['init'], { cwd: dir, stdio: 'ignore' });
      execFileSync('git', ['config', 'user.email', 'test@example.com'], { cwd: dir, stdio: 'ignore' });
      execFileSync('git', ['config', 'user.name', 'test'], { cwd: dir, stdio: 'ignore' });
      const tracked = join(dir, 'tracked.txt');
      writeFileSync(tracked, 'v1\n');
      execFileSync('git', ['add', 'tracked.txt'], { cwd: dir, stdio: 'ignore' });
      execFileSync('git', ['commit', '-m', 'init'], { cwd: dir, stdio: 'ignore' });
      writeFileSync(tracked, 'v2\n'); // unstaged modify → " M tracked.txt"
      writeFileSync(join(dir, 'fresh.txt'), 'new\n'); // untracked

      const res = sessionCapture(db, { agent_id: 'agent-cap', workspace: dir });
      expect(res.ok).toBe(true);
      expect(res.captured).toBe(true);
      expect(res.dirty_files).toEqual(expect.arrayContaining(['tracked.txt', 'fresh.txt']));
      expect(res.dirty_files?.some((f) => f.includes('racked.txt') && !f.startsWith('t'))).toBe(false);
      expect(res.dirty_files).not.toContain('racked.txt');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
