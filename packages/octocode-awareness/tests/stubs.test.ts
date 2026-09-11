import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db-init.js';
import { rebuildFts } from '../src/db-maintenance.js';
import { preFlightIntent } from '../src/intents-preflight.js';
import { pruneStale } from '../src/maintenance-stale.js';
import { digest } from '../src/maintenance-digest.js';
import { insertMemory } from '../src/memory-write.js';
import { auditUnverified } from '../src/verify-audit.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
function tempFile(): { dir: string; path: string; cleanup: () => void } {
    const dir = mkdtempSync(join(tmpdir(), 'oc-stubs-test-'));
    const path = join(dir, 'f.txt');
    writeFileSync(path, 'seed');
    return { dir, path, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}
describe('pruneStale', () => {
  it('returns 0 when no expired locks', () => {
    const db = freshDb();
    const result = pruneStale(db, {});
    expect(result.pruned_locks).toBe(0);
    expect(result).toEqual({ pruned_locks: 0 });
  });
  it('prunes expired locks', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const result = preFlightIntent(db, {
        agentId: 'agent', targetFiles: [path], ttlMs: 60_000,
      });
      if (!result.ok) throw new Error('claim failed');
      // Age the lock to the past
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE awareness_locks SET expires_at = ? WHERE run_id = ?').run(past, result.run.run_id);
      const pruned = pruneStale(db, {});
      expect(pruned.pruned_locks).toBeGreaterThanOrEqual(1);
    } finally { cleanup(); }
  });
  it('keeps work ACTIVE after its exclusive lock expires', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, {
        agentId: 'agent', targetFiles: [path], ttlMs: 60_000,
      });
      if (!claim.ok) throw new Error('claim failed');
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE awareness_locks SET expires_at = ? WHERE run_id = ?').run(past, claim.run.run_id);

      pruneStale(db, {});
      const intent = db.prepare('SELECT status FROM task_runs WHERE run_id = ?').get(claim.run.run_id) as { status: string };
      expect(intent.status).toBe('ACTIVE');
    } finally { cleanup(); }
  });
});

describe('auditUnverified', () => {
  it('returns ok=true and empty array when no PENDING tasks exist', () => {
    const db = freshDb();
    const result = auditUnverified(db, {});
    expect(result.ok).toBe(true);
    expect(result.unverified).toHaveLength(0);
    expect(result.count).toBe(0);
  });
});

describe('digest dry_run', () => {
  it('returns prediction fields without mutating anything', async () => {
    const db = freshDb();
    // Insert memory with expired valid_to
    (await insertMemory(db, {
      taskContext: 'dry_run test',
      observation: 'this should be archived',
      importance: 7,
      label: 'GOTCHA',
      validFrom: new Date(Date.now() - 2000).toISOString(),
      validTo: new Date(Date.now() - 1000).toISOString(),
    }));
    const before = (db.prepare("SELECT COUNT(*) AS c FROM awareness_memories WHERE state = 'ACTIVE'").get() as { c: number }).c;
    const result = digest(db, { dry_run: true });
    expect(result.dry_run).toBe(true);
    expect(result.would_archive).toBeGreaterThanOrEqual(1);
    expect(result.archived_memories).toBe(0); // nothing actually changed
    const after = (db.prepare("SELECT COUNT(*) AS c FROM awareness_memories WHERE state = 'ACTIVE'").get() as { c: number }).c;
    expect(after).toBe(before); // state unchanged
  });

  it('dry_run output keys match expected shape', () => {
    const db = freshDb();
    const result = digest(db, { dry_run: true });
    expect(Object.keys(result).sort()).toEqual([
      'archived_memories', 'candidate_ids', 'candidate_limit', 'dry_run', 'failed_stale_active_runs', 'fts_rebuilt', 'ok', 'pressure_age_days',
      'pressure_samples', 'pruned_locks', 'pruned_old', 'pruned_runs', 'resolved_handoff_signals',
      'stale_active_runs', 'stale_handoff_signals', 'stale_missing_refs', 'stale_open_signals', 'stale_pending_runs',
      'would_archive', 'would_fail_stale_active_runs', 'would_prune_locks', 'would_prune_old', 'would_prune_runs',
      'would_resolve_handoff_signals',
    ]);
    expect(result).toMatchObject({
      pressure_age_days: 1,
      stale_pending_runs: 0,
      stale_active_runs: 0,
      stale_open_signals: 0,
      stale_handoff_signals: 0,
      stale_missing_refs: 0,
      pressure_samples: { run_ids: [], active_run_ids: [], signal_ids: [], handoff_signal_ids: [], memory_ids: [] },
      candidate_limit: 20,
      candidate_ids: {
        expire_memory_ids: [], purge_memory_ids: [], locks: [], run_ids: [], stale_active_run_ids: [],
      },
    });
  });
});

describe('digest', () => {
  it('rebuilds memories_fts from awareness_memories source of truth', async () => {
    const db = freshDb();
    (await insertMemory(db, {
      taskContext: 'digest fts source',
      observation: 'fresh digest term survives rebuild',
      importance: 7,
      label: 'GOTCHA',
    }));

    db.exec('DELETE FROM memories_fts');
    expect(db.prepare('SELECT count(*) AS count FROM memories_fts').get()).toMatchObject({ count: 0 });

    const result = digest(db, {});
    expect(result.fts_rebuilt).toBe(true);
    const row = db.prepare('SELECT memory_id FROM memories_fts WHERE memories_fts MATCH ?').get('digest') as Record<string, unknown> | undefined;
    expect(row?.['memory_id']).toBeTruthy();
  });

  it('uses the same rebuild semantics as rebuildFts', async () => {
    const db = freshDb();
    const { memoryId } = (await insertMemory(db, {
      taskContext: 'digest stale row',
      observation: 'stale term cleanup',
      importance: 7,
      label: 'GOTCHA',
    }));
    rebuildFts(db);
    db.prepare('DELETE FROM awareness_memories WHERE memory_id = ?').run(memoryId);

    const result = digest(db, {});
    expect(result.fts_rebuilt).toBe(true);
    const stale = db.prepare('SELECT memory_id FROM memories_fts WHERE memories_fts MATCH ?').get('stale') as Record<string, unknown> | undefined;
    expect(stale).toBeUndefined();
  });
  it('compacts old terminal standalone runs while retaining verification receipts', () => {
    const db = freshDb();
    const old = '2020-01-01T00:00:00Z';
    db.prepare(`INSERT INTO task_runs
      (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES ('run_old_terminal', 'HOOK', 'agent', 'old aggregate', 'focused test', 'SUCCESS', '/repo', ?, ?)`)
      .run(old, old);
    db.prepare(`INSERT INTO run_files
      (run_id, file_path, source, started_at, heartbeat_at, expires_at, ended_at)
      VALUES ('run_old_terminal', '/repo/src/a.ts', 'HOOK', ?, ?, ?, ?)`)
      .run(old, old, old, old);
    db.prepare(`INSERT INTO event_outbox(event_id,workspace_path,event_type,schema_version,retention_class,
      aggregate_kind,aggregate_id,actor_json,provenance_json,payload_json,created_at)
      VALUES ('evt_receipt','/repo','run.verified',1,'audit','run','run_old_terminal',
        '{"kind":"agent","id":"agent"}','{"source":"tool","trust":"attributed-data"}',
        '{"message":"focused test passed"}',?)`).run(old);

    expect(digest(db, { dry_run: true, workspace_path: '/repo', operational_retention_days: 1 }).would_prune_runs).toBe(1);
    expect(digest(db, { workspace_path: '/repo', operational_retention_days: 1 }).pruned_runs).toBe(1);
    expect(db.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE run_id = 'run_old_terminal'").get()).toEqual({ count: 0 });
    expect(db.prepare("SELECT aggregate_id, payload_json FROM event_outbox WHERE event_id = 'evt_receipt'").get()).toEqual(
      { aggregate_id: 'run_old_terminal', payload_json: '{"message":"focused test passed"}' });
  });
});
