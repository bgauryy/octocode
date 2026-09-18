import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { connectDb } from '../src/db-runtime.js';
import { initDb } from '../src/db-init.js';
import { maintainInteractions } from '../src/interaction-retention.js';
import { pruneRetainedEvents } from '../src/event-retention.js';
import { pruneTerminalRuns, recoverStaleActiveRuns } from '../src/run-retention.js';
import { maintainMemories } from '../src/memory-retention.js';
import { maintainExpiredHistoryPreviews } from '../src/history-retention.js';
import { pruneExpiredLocks } from '../src/lock-retention.js';
import { latestRunVerification } from '../src/event-outbox.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true })));

function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'aw-retention-')));
  roots.push(workspace);
  const db = connectDb(join(workspace, 'awareness.sqlite3'));
  initDb(db);
  return { db, workspace };
}

const OLD = '2025-01-01T00:00:00.000Z';
const CUTOFF = '2025-06-01T00:00:00.000Z';
const NOW = '2026-01-01T00:00:00.000Z';

describe('retention domain owners', () => {
  it('expires interactions before pruning old terminal rows and preserves authorization evidence', () => {
    const { db, workspace } = fixture();
    const insert = db.prepare(`INSERT INTO pending_interactions(
      interaction_id, workspace_path, session_id, correlation_id, kind, request_json,
      status, created_at, expires_at, resolved_at
    ) VALUES (?, ?, 'session', ?, ?, '{}', ?, ?, ?, ?)`);
    insert.run('elapsed', workspace, 'corr-elapsed', 'question', 'pending', OLD, OLD, null);
    insert.run('answered', workspace, 'corr-answered', 'question', 'answered', OLD, null, OLD);
    insert.run('authorized', workspace, 'corr-authorized', 'authorization', 'answered', OLD, null, OLD);
    insert.run('fresh', workspace, 'corr-fresh', 'question', 'pending', NOW, null, null);
    db.prepare(`INSERT INTO authorization_receipts(
      receipt_id, interaction_id, workspace_path, session_id, plan_id, revision,
      scope_json, actor_json, provenance_json, created_at
    ) VALUES ('receipt', 'authorized', ?, 'session', 'plan', 'revision', '[]', '{}', '{}', ?)`)
      .run(workspace, OLD);

    expect(maintainInteractions(db, {
      workspacePath: workspace, terminalBefore: CUTOFF, now: NOW, limit: 10, dryRun: true,
    })).toEqual({ expired: 1, matched: 2, deleted: 0, partial: false });
    expect(db.prepare("SELECT status FROM pending_interactions WHERE interaction_id = 'elapsed'").get())
      .toEqual({ status: 'pending' });

    expect(maintainInteractions(db, {
      workspacePath: workspace, terminalBefore: CUTOFF, now: NOW, limit: 10, dryRun: false,
    })).toEqual({ expired: 1, matched: 2, deleted: 2, partial: false });
    expect(db.prepare('SELECT interaction_id FROM pending_interactions ORDER BY interaction_id').all())
      .toEqual([{ interaction_id: 'authorized' }, { interaction_id: 'fresh' }]);
    db.close();
  });

  it('prunes only elapsed delivery and operational events behind every consumer cursor', () => {
    const { db, workspace } = fixture();
    const insert = db.prepare(`INSERT INTO event_outbox(
      event_id, workspace_path, event_type, aggregate_kind, actor_json,
      provenance_json, payload_json, created_at, expires_at, retention_class
    ) VALUES (?, ?, 'test', ?, '{}', '{}', '{}', ?, ?, ?)`);
    insert.run('delivery', workspace, null, OLD, null, 'delivery');
    insert.run('operational', workspace, null, OLD, null, 'operational');
    insert.run('audit', workspace, null, OLD, OLD, 'audit');
    insert.run('experience', workspace, 'experience', OLD, OLD, 'delivery');
    db.prepare(`INSERT INTO event_consumers(workspace_path, consumer_id, sequence, updated_at)
      VALUES (?, 'slow', 1, ?), (?, 'fast', 4, ?)`).run(workspace, NOW, workspace, NOW);

    expect(pruneRetainedEvents(db, {
      workspacePath: workspace, now: NOW, deliveryBefore: CUTOFF, operationalBefore: CUTOFF,
      limit: 10, dryRun: true,
    })).toEqual({ matched: 1, deleted: 0, boundary: 1, partial: false });
    db.prepare("UPDATE event_consumers SET sequence = 4 WHERE consumer_id = 'slow'").run();
    expect(pruneRetainedEvents(db, {
      workspacePath: workspace, now: NOW, deliveryBefore: CUTOFF, operationalBefore: CUTOFF,
      limit: 10, dryRun: false,
    })).toEqual({ matched: 2, deleted: 2, boundary: 4, partial: false });
    expect(db.prepare('SELECT event_id FROM event_outbox ORDER BY sequence').all())
      .toEqual([{ event_id: 'audit' }, { event_id: 'experience' }]);
    db.close();
  });

  it('prunes only old terminal standalone work and hook runs', () => {
    const { db, workspace } = fixture();
    const insert = db.prepare(`INSERT INTO task_runs(
      run_id, task_id, origin, agent_id, rationale, test_plan, status,
      workspace_path, created_at, updated_at
    ) VALUES (?, NULL, ?, 'agent', 'reason', 'test', ?, ?, ?, ?)`);
    insert.run('old-work', 'WORK', 'SUCCESS', workspace, OLD, OLD);
    insert.run('old-hook', 'HOOK', 'FAILED', workspace, OLD, OLD);
    insert.run('active', 'WORK', 'ACTIVE', workspace, OLD, OLD);
    insert.run('task-origin', 'TASK', 'SUCCESS', workspace, OLD, OLD);

    expect(pruneTerminalRuns(db, { workspacePath: workspace, before: CUTOFF, limit: 10, dryRun: true }))
      .toEqual({ matched: 2, deleted: 0, partial: false });
    expect(pruneTerminalRuns(db, { workspacePath: workspace, before: CUTOFF, limit: 10, dryRun: false }))
      .toEqual({ matched: 2, deleted: 2, partial: false });
    expect(db.prepare('SELECT run_id FROM task_runs ORDER BY run_id').all())
      .toEqual([{ run_id: 'active' }, { run_id: 'task-origin' }]);
    db.close();
  });

  it('archives elapsed memories, deletes old superseded rows, and rebuilds derived search state', () => {
    const { db, workspace } = fixture();
    const insert = db.prepare(`INSERT INTO awareness_memories(
      memory_id, agent_id, task_context, observation, importance, state,
      tags_json, workspace_path, valid_to, created_at, updated_at
    ) VALUES (?, 'agent', ?, ?, 5, ?, '[]', ?, ?, ?, ?)`);
    insert.run('elapsed', 'elapsed context', 'elapsed observation', 'ACTIVE', workspace, OLD, OLD, OLD);
    insert.run('old', 'old context', 'old observation', 'SUPERSEDED', workspace, null, OLD, OLD);
    insert.run('fresh', 'fresh context', 'fresh observation', 'SUPERSEDED', workspace, null, NOW, NOW);
    db.prepare("INSERT INTO memory_refs(memory_id, reference, kind, ordinal) VALUES ('old', 'file:old.ts', 'file', 0)").run();
    for (const id of ['elapsed', 'old', 'fresh']) {
      db.prepare('INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)')
        .run(id, `${id} context`, `${id} observation`, '');
    }

    expect(maintainMemories(db, {
      workspacePath: workspace, terminalBefore: CUTOFF, now: NOW, limit: 10, dryRun: true,
    })).toEqual({ archived: 1, matched: 1, deleted: 0, ftsRebuilt: false, partial: false });
    expect(maintainMemories(db, {
      workspacePath: workspace, terminalBefore: CUTOFF, now: NOW, limit: 10, dryRun: false,
    })).toEqual({ archived: 1, matched: 1, deleted: 1, ftsRebuilt: true, partial: false });
    expect(db.prepare('SELECT memory_id, state FROM awareness_memories ORDER BY memory_id').all()).toEqual([
      { memory_id: 'elapsed', state: 'SUPERSEDED' },
      { memory_id: 'fresh', state: 'SUPERSEDED' },
    ]);
    expect(db.prepare("SELECT COUNT(*) AS count FROM memory_refs WHERE memory_id = 'old'").get()).toEqual({ count: 0 });
    expect(db.prepare('SELECT memory_id FROM memories_fts ORDER BY memory_id').all())
      .toEqual([{ memory_id: 'elapsed' }, { memory_id: 'fresh' }]);
    db.close();
  });

  it('prunes expired ready restore previews in bounded pages and retains receipts', () => {
    const { db, workspace } = fixture();
    db.prepare(`INSERT INTO local_history_operations(
      operation_id, workspace_path, agent_id, kind, status, request_hash, created_at, updated_at
    ) VALUES ('operation', ?, 'agent', 'edit', 'complete', 'hash', ?, ?)`)
      .run(workspace, OLD, OLD);
    const insert = db.prepare(`INSERT INTO local_history_restores(
      preview_id, workspace_path, agent_id, source_operation_id, side,
      files_json, expected_json, target_json, status, expires_at, created_at
    ) VALUES (?, ?, 'agent', 'operation', 'after', '[]', '{}', '{}', ?, ?, ?)`);
    insert.run('expired-a', workspace, 'ready', OLD, OLD);
    insert.run('expired-b', workspace, 'ready', OLD, OLD);
    insert.run('receipt', workspace, 'applied', OLD, OLD);

    expect(maintainExpiredHistoryPreviews(db, {
      workspacePath: workspace, now: NOW, limit: 1, dryRun: true,
    })).toEqual({ matched: 1, deleted: 0, preview_ids: ['expired-a'], partial: true });
    expect(maintainExpiredHistoryPreviews(db, {
      workspacePath: workspace, now: NOW, limit: 10, dryRun: false,
    })).toEqual({ matched: 2, deleted: 2, preview_ids: ['expired-a', 'expired-b'], partial: false });
    expect(db.prepare('SELECT preview_id FROM local_history_restores').all()).toEqual([{ preview_id: 'receipt' }]);
    db.close();
  });

  it('prunes expired locks in bounded deterministic batches', () => {
    const { db, workspace } = fixture();
    const run = db.prepare(`INSERT INTO task_runs(
      run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at
    ) VALUES (?, 'WORK', 'agent', 'reason', 'test', 'ACTIVE', ?, ?, ?)`);
    const lock = db.prepare(`INSERT INTO awareness_locks(lock_id, file_path, run_id, acquired_at, expires_at)
      VALUES (?, ?, ?, ?, ?)`);
    for (const suffix of ['a', 'b']) {
      run.run(`run-${suffix}`, workspace, OLD, OLD);
      lock.run(`lock-${suffix}`, `${workspace}/${suffix}.ts`, `run-${suffix}`, OLD, OLD);
    }

    expect(pruneExpiredLocks(db, { workspacePath: workspace, now: NOW, limit: 1, dryRun: true }))
      .toEqual({ matched: 2, deleted: 0, partial: false });
    expect(pruneExpiredLocks(db, { workspacePath: workspace, now: NOW, limit: 1, dryRun: false }))
      .toEqual({ matched: 1, deleted: 1, partial: true });
    expect(pruneExpiredLocks(db, { workspacePath: workspace, now: NOW, limit: 1, dryRun: false }))
      .toEqual({ matched: 1, deleted: 1, partial: false });
    db.close();
  });

  it('reports stale ACTIVE runs and recovers them only after explicit opt-in', () => {
    const { db, workspace } = fixture();
    db.prepare(`INSERT INTO task_runs(
      run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at
    ) VALUES ('stale-active', 'WORK', 'agent', 'abandoned', 'test', 'ACTIVE', ?, ?, ?)`)
      .run(workspace, OLD, OLD);
    db.prepare(`INSERT INTO run_files(run_id, file_path, source, started_at, heartbeat_at, expires_at)
      VALUES ('stale-active', ?, 'EXPLICIT', ?, ?, ?)`).run(`${workspace}/stale.ts`, OLD, OLD, OLD);

    expect(recoverStaleActiveRuns(db, {
      workspacePath: workspace, before: CUTOFF, now: NOW, limit: 10, apply: false, dryRun: false,
    })).toEqual({ matched: 1, failed: 0, partial: false });
    expect(db.prepare("SELECT status FROM task_runs WHERE run_id = 'stale-active'").get()).toEqual({ status: 'ACTIVE' });
    expect(recoverStaleActiveRuns(db, {
      workspacePath: workspace, before: CUTOFF, now: NOW, limit: 10, apply: true, dryRun: false,
    })).toEqual({ matched: 1, failed: 1, partial: false });
    expect(db.prepare("SELECT status FROM task_runs WHERE run_id = 'stale-active'").get()).toEqual({ status: 'FAILED' });
    expect(latestRunVerification(db, 'stale-active')?.message).toContain('no live claim or file presence');
    db.close();
  });
});
