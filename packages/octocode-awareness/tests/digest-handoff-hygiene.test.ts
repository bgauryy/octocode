import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db-init.js';
import { digest } from '../src/maintenance-digest.js';
import { insertNotification } from '../src/notifications-core.js';
import { auditUnverified } from '../src/verify-audit.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

const OLD = new Date(Date.now() - 30 * 86400000).toISOString();

function insertStaleActiveRun(db: DatabaseSync, runId = 'run_stale_active'): void {
  db.prepare(`INSERT INTO task_runs (
    run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at
  ) VALUES (?, 'WORK', 'old-agent', 'stale active session', 'maintenance digest', 'ACTIVE', '/repo', ?, ?)`).run(runId, OLD, OLD);
  db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
    VALUES (?, '/repo/stale.ts', 'EXPLICIT', ?, ?, ?)`).run(runId, OLD, OLD, OLD);
}

function insertLegacyHandoffRefinement(db: DatabaseSync, id: string, state: string): void {
  db.prepare(
    `INSERT INTO refinements (
       refinement_id, agent_id, workspace_path, files_json, reasoning, remember,
       quality, state, created_at, updated_at
     ) VALUES (?, 'old-agent', '/repo', '[]', 'r', 'm', 'handoff', ?, ?, ?)`
  ).run(id, state, OLD, OLD);
}

describe('digest handoff hygiene', () => {
  it('prunes legacy open handoff refinements past retention (dead letters)', () => {
    const db = freshDb();
    insertLegacyHandoffRefinement(db, 'ref_open', 'open');
    insertLegacyHandoffRefinement(db, 'ref_ongoing', 'ongoing');
    insertLegacyHandoffRefinement(db, 'ref_done', 'done');

    const preview = digest(db, { dry_run: true });
    expect(preview.would_prune_refinements).toBe(3);

    const applied = digest(db, {});
    expect(applied.pruned_refinements).toBe(3);
    const left = db.prepare("SELECT COUNT(*) AS c FROM refinements WHERE quality = 'handoff'").get() as { c: number };
    expect(left.c).toBe(0);
  });

  it('keeps fresh open handoff refinements until retention passes', () => {
    const db = freshDb();
    const now = new Date().toISOString();
    db.prepare(
      `INSERT INTO refinements (
         refinement_id, agent_id, workspace_path, files_json, reasoning, remember,
         quality, state, created_at, updated_at
       ) VALUES ('ref_fresh', 'a', '/repo', '[]', 'r', 'm', 'handoff', 'open', ?, ?)`
    ).run(now, now);
    const applied = digest(db, {});
    expect(applied.pruned_refinements).toBe(0);
  });

  it('auto-resolves open handoff signals older than retention (TTL), preserving others', () => {
    const db = freshDb();
    const stale = insertNotification(db, {
      agentId: 'a', kind: 'handoff', subject: 'stale handoff', workspacePath: '/repo',
    });
    db.prepare('UPDATE signals SET created_at = ? WHERE signal_id = ?').run(OLD, stale.signal_id);
    const fresh = insertNotification(db, {
      agentId: 'a', kind: 'handoff', subject: 'fresh handoff', workspacePath: '/repo',
    });
    const staleFyi = insertNotification(db, {
      agentId: 'a', kind: 'fyi', subject: 'old fyi stays open', workspacePath: '/repo',
    });
    db.prepare('UPDATE signals SET created_at = ? WHERE signal_id = ?').run(OLD, staleFyi.signal_id);

    const preview = digest(db, { dry_run: true });
    expect(preview.would_resolve_handoff_signals).toBe(1);

    const applied = digest(db, {});
    expect(applied.resolved_handoff_signals).toBe(1);
    const status = (id: string) =>
      (db.prepare('SELECT status FROM signals WHERE signal_id = ?').get(id) as { status: string }).status;
    expect(status(stale.signal_id)).toBe('resolved');
    expect(status(fresh.signal_id)).toBe('open');
    expect(status(staleFyi.signal_id)).toBe('open');
  });

  it('uses a separate short TTL for handoff signals without pruning legacy refinements early', () => {
    const db = freshDb();
    const twoDaysOld = new Date(Date.now() - 2 * 86400000).toISOString();
    const stale = insertNotification(db, {
      agentId: 'a', kind: 'handoff', subject: 'old broadcast handoff', workspacePath: '/repo',
    });
    db.prepare('UPDATE signals SET created_at = ? WHERE signal_id = ?').run(twoDaysOld, stale.signal_id);
    insertLegacyHandoffRefinement(db, 'ref_kept_for_legacy_retention', 'open');
    db.prepare('UPDATE refinements SET updated_at = ?, created_at = ? WHERE refinement_id = ?')
      .run(twoDaysOld, twoDaysOld, 'ref_kept_for_legacy_retention');

    const preview = digest(db, { dry_run: true });
    expect(preview.would_resolve_handoff_signals).toBe(1);
    expect(preview.would_prune_refinements).toBe(0);

    const applied = digest(db, {});
    expect(applied.resolved_handoff_signals).toBe(1);
    expect(applied.pruned_refinements).toBe(0);
  });

  it('marks stale ACTIVE runs failed with a maintenance receipt', () => {
    const db = freshDb();
    insertStaleActiveRun(db);

    const preview = digest(db, { workspace_path: '/repo', dry_run: true });
    expect(preview.stale_active_runs).toBe(1);
    expect(preview.would_fail_stale_active_runs).toBe(1);
    expect(preview.candidate_ids?.stale_active_run_ids).toEqual(['run_stale_active']);

    const applied = digest(db, { workspace_path: '/repo' });
    expect(applied.failed_stale_active_runs).toBe(1);
    expect(db.prepare("SELECT status FROM task_runs WHERE run_id = 'run_stale_active'").get())
      .toEqual({ status: 'FAILED' });
    expect(auditUnverified(db, { workspacePath: '/repo' }).count).toBe(0);
    expect((db.prepare("SELECT message FROM run_log WHERE run_id = 'run_stale_active'").get() as { message: string }).message)
      .toContain('maintenance digest: stale ACTIVE run');
  });

  it('can preview stale ACTIVE runs without applying that recovery', () => {
    const db = freshDb();
    insertStaleActiveRun(db);

    const preview = digest(db, { workspace_path: '/repo', dry_run: true, fail_stale_active_runs: false });
    expect(preview.stale_active_runs).toBe(1);
    expect(preview.would_fail_stale_active_runs).toBe(0);

    const applied = digest(db, { workspace_path: '/repo', fail_stale_active_runs: false });
    expect(applied.failed_stale_active_runs).toBe(0);
    expect(db.prepare("SELECT status FROM task_runs WHERE run_id = 'run_stale_active'").get())
      .toEqual({ status: 'ACTIVE' });
  });
});
