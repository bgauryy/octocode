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
import { randomUUID } from 'node:crypto';
import { initDb, replaceMemoryReferences, connectDb, checkpointWal } from '../src/db.js';
import { journalModeForSqliteVersion } from '../src/sqlite-runtime.js';
import {
  pruneStale,
  notifyGet,
  exportHarness,
  exportMemoryDoc,
  getWorkspaceStatus,
  sessionCapture,
  parseGitStatusShortLines,
  digest,
} from '../src/maintenance.js';

// ─── Helpers ──────────────────────────────────────────────────────────────────

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

/** Insert a memory using the memories table. */
function insertMem(
  db: DatabaseSync,
  opts: {
    memoryId?: string;
    importance?: number;
    label?: string;
    tags?: string[];
    failureSig?: string;
    observation?: string;
    workspacePath?: string | null;
  } = {},
): string {
  const memoryId = opts.memoryId ?? 'mem_' + randomUUID().replace(/-/g, '');
  const now = new Date().toISOString();
  db.prepare(`
    INSERT INTO memories (
      memory_id, agent_id, task_context, observation, importance,
      label, tags_json, workspace_path, failure_signature, created_at
    ) VALUES (?, 'agent-test', 'test context', ?, ?, ?, ?, ?, ?, ?)
  `).run(
    memoryId,
    opts.observation ?? 'test observation',
    opts.importance ?? 5,
    opts.label ?? 'OTHER',
    JSON.stringify(opts.tags ?? []),
    opts.workspacePath ?? null,
    opts.failureSig ?? null,
    now,
  );
  return memoryId;
}

/** Insert an ACTIVE task and return its run_id. */
function insertTask(
  db: DatabaseSync,
  opts: { agentId?: string; workspacePath?: string; sessionId?: string | null; planDocRef?: string | null } = {},
): string {
  const runId = 'task_' + randomUUID().replace(/-/g, '');
  const now = new Date().toISOString();
  db.prepare(`
    INSERT INTO task_runs (run_id, origin, agent_id, rationale, test_plan, context_ref, status, workspace_path, created_at, updated_at)
    VALUES (?, 'WORK', ?, 'test rationale', 'yarn test', ?, 'ACTIVE', ?, ?, ?)
  `).run(runId, opts.agentId ?? 'agent-test', opts.planDocRef ?? null, opts.workspacePath ?? '/ws', now, now);
  return runId;
}

/** Insert a lock for a task. */
function insertLock(
  db: DatabaseSync,
  opts: { runId: string; filePath?: string; agentId?: string; expiresAt?: string | null },
): string {
  const lockId = 'lock_' + randomUUID().replace(/-/g, '');
  const now = new Date().toISOString();
  db.prepare(`
    INSERT INTO locks (lock_id, file_path, run_id, acquired_at, expires_at)
    VALUES (?, ?, ?, ?, ?)
  `).run(
    lockId,
    opts.filePath ?? '/ws/a.ts',
    opts.runId,
    now,
    opts.expiresAt ?? null,
  );
  return lockId;
}

// ─── 1. pruneStale — uses locks + tasks ──────────────────────────────────────

describe('pruneStale — locks + tasks', () => {
  it('dry_run returns would_prune without deleting', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: past });

    const res = pruneStale(db, { dry_run: true });
    expect(res.dry_run).toBe(true);
    expect(res.would_prune).toBeGreaterThanOrEqual(1);
    expect(res.pruned_locks).toBe(0);

    // Nothing deleted
    const lockCount = (db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c;
    expect(lockCount).toBe(1);
  });

  it('prunes expired locks from the locks table', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: past });

    const res = pruneStale(db, {});
    expect(res.pruned_locks).toBeGreaterThanOrEqual(1);

    const lockCount = (db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c;
    expect(lockCount).toBe(0);
  });

  it('does not end independent work when its expired exclusive lock is pruned', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: past });

    pruneStale(db, {});

    const task = db.prepare(
      'SELECT status FROM task_runs WHERE run_id = ?'
    ).get(runId) as { status: string } | undefined;
    expect(task?.status).toBe('ACTIVE');
  });

  it('normalizes relative target_file filters against workspace', () => {
    const db = freshDb();
    const runId = insertTask(db, { workspacePath: '/repo' });
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, filePath: '/repo/src/a.ts', expiresAt: past });

    const dry = pruneStale(db, { workspace: '/repo', target_file: 'src/a.ts', dry_run: true });
    expect(dry.would_prune).toBe(1);

    const res = pruneStale(db, { workspace: '/repo', target_file: 'src/a.ts' });
    expect(res.pruned_locks).toBe(1);
    const task = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
    expect(task.status).toBe('ACTIVE');
  });

  it('does not prune non-expired locks', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const future = new Date(Date.now() + 10 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: future });

    const res = pruneStale(db, {});
    expect(res.pruned_locks).toBe(0);

    const lockCount = (db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c;
    expect(lockCount).toBe(1);
  });
});

// ─── 2. getWorkspaceStatus — uses memories + tasks + locks ────────────────────

describe('getWorkspaceStatus — current schema', () => {
  it('returns active_memories count from the memories table', () => {
    const db = freshDb();
    insertMem(db);
    insertMem(db);

    const status = getWorkspaceStatus(db, {});
    expect(status.ok).toBe(true);
    expect(status.active_memories).toBeGreaterThanOrEqual(2);
  });

  it('returns pending_runs count from the tasks table', () => {
    const db = freshDb();
    const runId = insertTask(db);
    db.prepare("UPDATE task_runs SET status = 'PENDING' WHERE run_id = ?").run(runId);

    const status = getWorkspaceStatus(db, {});
    expect(status.pending_runs).toBeGreaterThanOrEqual(1);
  });

  it('returns active_runs count from the tasks table', () => {
    const db = freshDb();
    insertTask(db);

    const status = getWorkspaceStatus(db, {});
    expect(status.active_runs).toBeGreaterThanOrEqual(1);
  });

  it('returns active locks from locks table', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const future = new Date(Date.now() + 10 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: future });

    const status = getWorkspaceStatus(db, {});
    expect(status.lock_count).toBe(1);
    expect(status.locks.length).toBeGreaterThanOrEqual(1);
    expect(status.locks[0]).toHaveProperty('file_path');
    expect(status.locks[0]).toHaveProperty('agent_id');
  });

  it('filters expired locks without mutating status during a read', () => {
    const db = freshDb();
    const runId = insertTask(db);
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    insertLock(db, { runId, expiresAt: past });

    const status = getWorkspaceStatus(db, {});
    expect(status.lock_count).toBe(0);
    expect(status.locks).toHaveLength(0);
    expect(status.pending_runs).toBe(0);
    expect(status.active_runs).toBe(1);
    const lockCount = db.prepare('SELECT COUNT(*) AS count FROM locks').get() as { count: number };
    expect(lockCount.count).toBe(1);
    const task = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(runId) as { status: string };
    expect(task.status).toBe('ACTIVE');
  });
});

// ─── 3. notifyGet — reads from memories ──────────────────────────────────────

describe('notifyGet — smart briefing from memories table', () => {
  it('returns empty briefing when no memories exist', () => {
    const db = freshDb();
    const res = notifyGet(db, { agent_id: 'agent-a', workspace: '/ws' });
    expect(res.ok).toBe(true);
  });

  it('surfaces high-importance memories from memories table using importance column', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 8,
      label: 'GOTCHA',
      observation: 'always check token expiry',
      workspacePath: '/ws',
    });

    const res = notifyGet(db, { agent_id: 'agent-b', workspace: '/ws' }) as {
      ok: true; count: number; notifications: Array<{ kind: string; text: string }>;
    };
    expect(res.ok).toBe(true);
    // The briefing should surface the GOTCHA memory
    expect(res.count).toBeGreaterThanOrEqual(1);
    expect(res.notifications.some(n => n.text.includes('GOTCHA'))).toBe(true);
  });

  it('surfaces weakness cluster when failure_signature is present', () => {
    const db = freshDb();
    const sig = 'mechanism:test-timeout|cause:slow-io';
    insertMem(db, { failureSig: sig, importance: 6, workspacePath: '/ws' });
    insertMem(db, { failureSig: sig, importance: 6, workspacePath: '/ws' });

    const res = notifyGet(db, { agent_id: 'agent-c', workspace: '/ws' }) as {
      ok: true; count: number; notifications: Array<{ kind: string; text: string }>;
    };
    expect(res.notifications.some(n => n.kind === 'weakness')).toBe(true);
  });

  it('silences unchanged hook briefs per agent, session, and scope without acknowledging signals', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 8,
      label: 'GOTCHA',
      observation: 'brief only when the actionable set changes',
      workspacePath: '/ws',
    });
    db.prepare(`INSERT INTO signals (
      signal_id, workspace_path, from_agent, to_agent, kind, subject, files_json, refs_json,
      thread_id, importance, status, created_at
    ) VALUES ('signal-1', '/ws', 'agent-a', 'agent-b', 'request', 'review', '[]', '[]',
      'signal-1', 8, 'open', strftime('%Y-%m-%dT%H:%M:%fZ','now'))`).run();

    const params = { agent_id: 'agent-b', session_id: 'sess-1', workspace: '/ws', format: 'hook' };
    const first = notifyGet(db, params) as { count: number; additionalContext?: string };
    const second = notifyGet(db, params) as { count: number; additionalContext?: string };

    expect(first.additionalContext).toContain('review');
    expect(second).toEqual({ ok: true, count: 0, notifications: [] });
    expect((db.prepare('SELECT COUNT(*) AS c FROM signal_reads').get() as { c: number }).c).toBe(0);

    const otherSession = notifyGet(db, { ...params, session_id: 'sess-2' }) as { additionalContext?: string };
    expect(otherSession.additionalContext).toContain('review');
  });
});

// ─── 4. exportHarness — JSON tag matching ────────────────────────────────────

describe('exportHarness — tag matching', () => {
  it('surfaces harness-tagged memories using tags_json', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 8,
      label: 'GOTCHA',
      tags: ['reflection', 'harness'],
      observation: 'run mine-weakness before export-harness',
    });

    const res = exportHarness(db, {});
    expect(res.count).toBeGreaterThanOrEqual(1);
    expect(res.memories.some(m => m.tier === 'harness')).toBe(true);
    expect(res.next).toContain('Human review required');
    expect(res.next).toContain('octocode-awareness reflect record');
  });

  it('does not include non-harness memories in tier-1', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 9,
      label: 'DECISION',
      tags: ['architecture'],
      observation: 'use SQLite for local memory',
    });

    const res = exportHarness(db, { harness_only: true });
    // harness_only=true → only tier-1; DECISION without 'harness' tag must be absent
    const harnessCount = res.memories.filter(m => m.tier === 'harness').length;
    expect(harnessCount).toBe(0);
  });

  it('surfaces high-importance general memories in tier-2', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 8,
      label: 'DECISION',
      tags: [],
      observation: 'always validate before conclude',
    });

    const res = exportHarness(db, { min_importance: 7 });
    expect(res.memories.some(m => m.tier === 'general')).toBe(true);
  });

  it('returns empty markdown when no qualifying memories exist', () => {
    const db = freshDb();
    const res = exportHarness(db, {});
    expect(res.count).toBe(0);
    expect(res.markdown).toContain('No harness');
    expect(res.next).toContain('octocode-awareness reflect record --fix-harness');
  });
});

// ─── 5. sessionCapture — uses tasks table ────────────────────────────────────

describe('sessionCapture — tasks table', () => {
  it('returns captured=false when no active/pending tasks exist', () => {
    const db = freshDb();
    const res = sessionCapture(db, { agent_id: 'agent-cap', workspace: '/ws' });
    expect(res.ok).toBe(true);
    expect(res.captured).toBe(false);
    expect(res.refinement_id).toBeNull();
  });

  it('captures active tasks from the tasks table and creates a handoff refinement', () => {
    const db = freshDb();
    insertTask(db, { agentId: 'agent-cap', workspacePath: '/ws' });

    const res = sessionCapture(db, { agent_id: 'agent-cap', workspace: '/ws' });
    expect(res.ok).toBe(true);
    expect(res.captured).toBe(true);
    expect(res.active_runs).toBeGreaterThanOrEqual(1);
    expect(res.refinement_id).toBeTruthy();

    // Verify the refinement was written to the refinements table
    const ref = db.prepare(
      "SELECT quality, state FROM refinements WHERE refinement_id = ?"
    ).get(res.refinement_id!) as { quality: string; state: string } | undefined;
    expect(ref?.quality).toBe('handoff');
    expect(ref?.state).toBe('open');
  });

  it('includes context_ref in handoff task details', () => {
    const db = freshDb();
    insertTask(db, {
      agentId: 'agent-cap',
      workspacePath: '/ws',
      planDocRef: 'docs/plans/session.md',
    });

    const res = sessionCapture(db, { agent_id: 'agent-cap', workspace: '/ws' });
    expect(res.captured).toBe(true);

    const ref = db.prepare(
      'SELECT reasoning FROM refinements WHERE refinement_id = ?'
    ).get(res.refinement_id!) as { reasoning: string } | undefined;
    expect(ref?.reasoning).toContain('plan=docs/plans/session.md');
  });

  it('bounds handoff file arrays and visible task text', () => {
    const db = freshDb();
    const runId = insertTask(db, { agentId: 'agent-cap', workspacePath: '/ws' });
    const files = Array.from({ length: 60 }, (_, i) => `/ws/src/file-${i}.ts`);
    db.prepare(
      `UPDATE task_runs SET rationale = ?, test_plan = ? WHERE run_id = ?`
    ).run('rationale '.repeat(80), 'test plan '.repeat(80), runId);
    const now = new Date().toISOString();
    const insertFile = db.prepare(
      `INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
       VALUES (?, ?, 'EXPLICIT', ?, ?, ?)`
    );
    for (const file of files) insertFile.run(runId, file, now, now, new Date(Date.now() + 60_000).toISOString());

    const res = sessionCapture(db, { agent_id: 'agent-cap', workspace: '/ws' });
    expect(res.captured).toBe(true);
    expect(res.files).toHaveLength(20);
    expect(res.file_count).toBe(60);
    expect(res.omitted_files).toBe(40);

    const ref = db.prepare(
      'SELECT reasoning, remember, files_json FROM refinements WHERE refinement_id = ?'
    ).get(res.refinement_id!) as { reasoning: string; remember: string; files_json: string };
    expect(JSON.parse(ref.files_json)).toHaveLength(20);
    expect(ref.reasoning).toContain('(+57 more)');
    expect(ref.remember).toContain('showing 10 of 60');
    expect(ref.remember).toContain('50 omitted');
    expect(ref.reasoning).not.toContain('file-59.ts');
  });

  it('does not create a duplicate handoff when unresolved state is unchanged', () => {
    const db = freshDb();
    insertTask(db, { agentId: 'agent-cap', workspacePath: '/ws' });

    const first = sessionCapture(db, { agent_id: 'agent-cap', workspace: '/ws' });
    const second = sessionCapture(db, { agent_id: 'agent-cap', workspace: '/ws' });

    expect(first.captured).toBe(true);
    expect(second).toMatchObject({ captured: false, deduplicated: true, refinement_id: first.refinement_id });
    const count = db.prepare(
      "SELECT COUNT(*) AS c FROM refinements WHERE agent_id = 'agent-cap' AND quality = 'handoff' AND state = 'open'"
    ).get() as { c: number };
    expect(count.c).toBe(1);
  });
});

describe('exportMemoryDoc — reference joins', () => {
  it('includes references beyond SQLite placeholder limits', { timeout: 20_000 }, () => {
    const db = freshDb();
    for (let i = 0; i < 1050; i++) {
      const id = insertMem(db, {
        memoryId: `mem_export_${i}`,
        observation: `export observation ${i}`,
        importance: 5,
      });
      replaceMemoryReferences(db, id, [`file:/tmp/late-export-${i}.ts`]);
    }

    const doc = exportMemoryDoc(db, {});
    expect(doc).toContain('file:/tmp/late-export-1049.ts');
  });
});

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

    // Nothing deleted in dry_run
    const row = db.prepare("SELECT state FROM memories WHERE memory_id = 'mem_old'").get() as { state: string } | undefined;
    expect(row?.state).toBe('SUPERSEDED');
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
