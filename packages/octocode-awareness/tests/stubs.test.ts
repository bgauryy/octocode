import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb, rebuildFts } from '../src/db.js';
import { preFlightIntent } from '../src/intents.js';
import { pruneStale, notifyGet, digest, getWorkspaceStatus, exportMemoryDoc } from '../src/maintenance.js';
import { insertMemory } from '../src/memory.js';
import { insertRefinement, updateRefinement } from '../src/refinements.js';
import { insertNotification } from '../src/notifications.js';
import { auditUnverified } from '../src/verify.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
function tempFile(): {
    dir: string;
    path: string;
    cleanup: () => void;
} {
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
      db.prepare('UPDATE locks SET expires_at = ? WHERE run_id = ?')
        .run(past, result.run.run_id);

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
      db.prepare('UPDATE locks SET expires_at = ? WHERE run_id = ?')
        .run(past, claim.run.run_id);

      pruneStale(db, {});
      const intent = db.prepare('SELECT status FROM task_runs WHERE run_id = ?')
        .get(claim.run.run_id) as { status: string };
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

describe('notifyGet', () => {
  it('returns ok=true and empty array when there is no briefing', () => {
    const db = freshDb();
    const result = notifyGet(db, {});
    expect(result.ok).toBe(true);
    expect(result.count).toBe(0);
    expect(result.notifications).toHaveLength(0);
  });

  it('returns a query-grounded memory briefing without db envelope noise', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'notify briefing',
      observation: 'Important gotcha should appear in briefing',
      importance: 8,
      label: 'GOTCHA',
    });
    const result = notifyGet(db, { format: 'hook', query: 'notify briefing' });
    expect(result.ok).toBe(true);
    expect(result.count).toBe(1);
    expect(result.notifications[0]?.kind).toBe('memory');
    expect('additionalContext' in result).toBe(true);
    expect('db_path' in result).toBe(false);
  });

  it('delivers an unread signal once per unchanged hook scope without acknowledging it', () => {
    const db = freshDb();
    insertNotification(db, {
      agentId: 'agent-a',
      toAgent: 'agent-b',
      kind: 'request',
      subject: 'please check locks',
      body: 'handoff detail',
      workspacePath: '/repo',
      importance: 8,
    });

    const first = notifyGet(db, { format: 'hook', agent_id: 'agent-b', workspace: '/repo' });
    expect(first.ok).toBe(true);
    expect(first.notifications[0]?.kind).toBe('notification');
    expect('additionalContext' in first && first.additionalContext).toContain('please check locks');

    const second = notifyGet(db, { format: 'hook', agent_id: 'agent-b', workspace: '/repo' });
    expect(second).toEqual({ ok: true, count: 0, notifications: [] });
    expect((db.prepare('SELECT COUNT(*) AS c FROM signal_reads').get() as { c: number }).c).toBe(0);
  });
});

describe('digest dry_run', () => {
  it('returns prediction fields without mutating anything', () => {
    const db = freshDb();
    // Insert memory with expired valid_to
    insertMemory(db, {
      taskContext: 'dry_run test',
      observation: 'this should be archived',
      importance: 7,
      label: 'GOTCHA',
      validFrom: new Date(Date.now() - 2000).toISOString(),
      validTo: new Date(Date.now() - 1000).toISOString(),
    });
    const before = (db.prepare("SELECT COUNT(*) AS c FROM memories WHERE state = 'ACTIVE'").get() as { c: number }).c;
    const result = digest(db, { dry_run: true });
    expect(result.dry_run).toBe(true);
    expect(result.would_archive).toBeGreaterThanOrEqual(1);
    expect(result.archived_memories).toBe(0); // nothing actually changed
    const after = (db.prepare("SELECT COUNT(*) AS c FROM memories WHERE state = 'ACTIVE'").get() as { c: number }).c;
    expect(after).toBe(before); // state unchanged
  });

  it('dry_run output keys match expected shape', () => {
    const db = freshDb();
    const result = digest(db, { dry_run: true });
    expect(Object.keys(result).sort()).toEqual([
      'archived_memories', 'candidate_ids', 'candidate_limit', 'dry_run', 'fts_rebuilt', 'ok', 'pressure_age_days',
      'pressure_samples', 'pruned_locks', 'pruned_old', 'pruned_refinements', 'pruned_runs',
      'stale_missing_refs', 'stale_open_signals', 'stale_pending_runs',
      'would_archive', 'would_prune_locks', 'would_prune_old', 'would_prune_refinements', 'would_prune_runs',
    ]);
    expect(result).toMatchObject({
      pressure_age_days: 1,
      stale_pending_runs: 0,
      stale_open_signals: 0,
      stale_missing_refs: 0,
      pressure_samples: { run_ids: [], signal_ids: [], memory_ids: [] },
      candidate_limit: 20,
      candidate_ids: {
        expire_memory_ids: [], purge_memory_ids: [], lock_ids: [], refinement_ids: [], run_ids: [],
      },
    });
  });
});

describe('digest', () => {
  it('rebuilds memories_fts from memories source of truth', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'digest fts source',
      observation: 'fresh digest term survives rebuild',
      importance: 7,
      label: 'GOTCHA',
    });

    db.exec('DELETE FROM memories_fts');
    expect(db.prepare('SELECT count(*) AS count FROM memories_fts').get()).toMatchObject({ count: 0 });

    const result = digest(db, {});
    expect(result.fts_rebuilt).toBe(true);
    const row = db.prepare('SELECT memory_id FROM memories_fts WHERE memories_fts MATCH ?').get('digest') as Record<string, unknown> | undefined;
    expect(row?.['memory_id']).toBeTruthy();
  });

  it('uses the same rebuild semantics as rebuildFts', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 'digest stale row',
      observation: 'stale term cleanup',
      importance: 7,
      label: 'GOTCHA',
    });
    rebuildFts(db);
    db.prepare('DELETE FROM memories WHERE memory_id = ?').run(memoryId);

    const result = digest(db, {});
    expect(result.fts_rebuilt).toBe(true);
    const stale = db.prepare('SELECT memory_id FROM memories_fts WHERE memories_fts MATCH ?').get('stale') as Record<string, unknown> | undefined;
    expect(stale).toBeUndefined();
  });
  it('prunes terminal handoffs and completed refinements while keeping active repo fixes', () => {
    const db = freshDb();
    const old = new Date(Date.now() - 45 * 86400000).toISOString();
    const fresh = new Date().toISOString();
    const handoff = insertRefinement(db, { reasoning: 'handoff', remember: 'Review session handoff for agent', quality: 'handoff' }).refinementId;
    updateRefinement(db, { refinementId: handoff, state: 'done', actorAgentId: 'tester', checkReceipt: 'handoff consumed' });
    const done = insertRefinement(db, { reasoning: 'done', remember: 'done fix', quality: 'bad', state: 'open' }).refinementId;
    updateRefinement(db, { refinementId: done, state: 'done', actorAgentId: 'tester', checkReceipt: 'fixture verified' });
    const active = insertRefinement(db, { reasoning: 'active', remember: 'active fix', quality: 'bad', state: 'open' }).refinementId;
    db.prepare('UPDATE refinements SET created_at = ?, updated_at = ? WHERE refinement_id IN (?, ?)').run(old, old, handoff, done);
    db.prepare('UPDATE refinements SET created_at = ?, updated_at = ? WHERE refinement_id = ?').run(fresh, fresh, active);

    const dry = digest(db, { dry_run: true });
    expect(dry.would_prune_refinements).toBe(2);
    const result = digest(db, {});
    expect(result.pruned_refinements).toBe(2);
    const remaining = db.prepare('SELECT refinement_id FROM refinements').all() as Array<{ refinement_id: string }>;
    expect(remaining.map(r => r.refinement_id)).toEqual([active]);
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
    db.prepare(`INSERT INTO run_log(event_id, run_id, agent_id, event_type, message, created_at)
      VALUES ('evt_receipt', 'run_old_terminal', 'agent', 'VERIFIED', 'focused test passed', ?)`)
      .run(old);

    expect(digest(db, { dry_run: true, workspace: '/repo', operational_retention_days: 1 }).would_prune_runs).toBe(1);
    expect(digest(db, { workspace: '/repo', operational_retention_days: 1 }).pruned_runs).toBe(1);
    expect(db.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE run_id = 'run_old_terminal'").get()).toEqual({ count: 0 });
    expect(db.prepare("SELECT run_id, message FROM run_log WHERE event_id = 'evt_receipt'").get())
      .toEqual({ run_id: null, message: 'focused test passed' });
  });

});

describe('getWorkspaceStatus', () => {
  it('returns ok:true with counts and locks', () => {
    const db = freshDb();
    const result = getWorkspaceStatus(db, {});
    expect(result.ok).toBe(true);
    expect(typeof result.active_memories).toBe('number');
    expect(typeof result.pending_runs).toBe('number');
    expect(typeof result.active_runs).toBe('number');
    expect(typeof result.actionable_refinements).toBe('number');
    expect(typeof result.all_open_refinements).toBe('number');
    expect(Array.isArray(result.locks)).toBe(true);
  });

  it('reflects memory counts accurately', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'workspace status test',
      observation: 'a test memory',
      importance: 7,
      label: 'GOTCHA',
    });
    const result = getWorkspaceStatus(db, {});
    expect(result.active_memories).toBe(1);
  });

  it('shows active file locks', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const intent = preFlightIntent(db, { agentId: 'agent-a', targetFiles: [path] });
      expect(intent.ok).toBe(true);
      const result = getWorkspaceStatus(db, {});
      expect(result.locks.length).toBeGreaterThanOrEqual(1);
      expect(result.locks[0]).toHaveProperty('file_path');
      expect(result.locks[0]).toHaveProperty('agent_id');
    } finally { cleanup(); }
  });
});

describe('exportMemoryDoc', () => {
  it('returns a non-empty markdown string', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'export doc test',
      observation: 'a memorable observation for the report',
      importance: 8,
      label: 'DECISION',
      tags: ['export', 'test'],
    });
    const doc = exportMemoryDoc(db, {});
    expect(typeof doc).toBe('string');
    expect(doc).toContain('# Memory Store Report');
    expect(doc).toContain('DECISION');
    expect(doc).toContain('a memorable observation for the report');
    expect(doc).toContain('export, test');
  });

  it('includes stats header with counts and labels', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 'c1', observation: 'o1', importance: 7, label: 'GOTCHA' });
    insertMemory(db, { taskContext: 'c2', observation: 'o2', importance: 6, label: 'DECISION' });
    const doc = exportMemoryDoc(db, {});
    expect(doc).toContain('**Total active memories:** 2');
    expect(doc).toContain('GOTCHA(1)');
    expect(doc).toContain('DECISION(1)');
  });

  it('includes provenance references', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'reference export',
      observation: 'doc export should keep provenance visible',
      importance: 8,
      references: ['file:/tmp/provenance.ts', 'pr:owner/repo#456'],
    });
    const doc = exportMemoryDoc(db, {});
    expect(doc).toContain('**References:** file:/tmp/provenance.ts, pr:owner/repo#456');
  });

  it('returns empty report when no memories exist', () => {
    const db = freshDb();
    const doc = exportMemoryDoc(db, {});
    expect(doc).toContain('**Total active memories:** 0');
  });
});
