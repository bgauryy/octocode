import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb, rebuildFts } from '../src/db.js';
import { preFlightIntent } from '../src/intents.js';
// Import via stubs.js shim to verify the re-export chain still works.
import { pruneStale, notifyGet, sessionCapture, waitForLock, digest, getWorkspaceStatus, exportMemoryDoc, exportHarness } from '../src/maintenance.js';
import { insertMemory } from '../src/memory.js';
import { insertRefinement } from '../src/refinements.js';
import { insertNotification } from '../src/notifications.js';
import { auditUnverified } from '../src/verify.js';

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
    expect(result.updated_tasks).toBe(0);
  });

  it('prunes expired locks', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const result = preFlightIntent(db, {
        agentId: 'agent', targetFiles: [path], ttlMs: 1000,
      });
      if (!result.ok) throw new Error('claim failed');
      // Age the lock to the past
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE task_id = ?')
        .run(past, result.task.task_id);

      const pruned = pruneStale(db, {});
      expect(pruned.pruned_locks).toBeGreaterThanOrEqual(1);
    } finally { cleanup(); }
  });

  it('sets intent to PENDING after lock expiry', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, {
        agentId: 'agent', targetFiles: [path], ttlMs: 1000,
      });
      if (!claim.ok) throw new Error('claim failed');
      const past = new Date(Date.now() - 5000).toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare('UPDATE locks SET expires_at = ? WHERE task_id = ?')
        .run(past, claim.task.task_id);

      pruneStale(db, {});
      const intent = db.prepare('SELECT status FROM tasks WHERE task_id = ?')
        .get(claim.task.task_id) as { status: string };
      expect(intent.status).toBe('PENDING');
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

  it('returns a smart memory briefing without db envelope noise', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'notify briefing',
      observation: 'Important gotcha should appear in briefing',
      importance: 8,
      label: 'GOTCHA',
    });
    const result = notifyGet(db, { format: 'hook' });
    expect(result.ok).toBe(true);
    expect(result.count).toBe(1);
    expect(result.notifications[0]?.kind).toBe('memory');
    expect('additionalContext' in result).toBe(true);
    expect('db_path' in result).toBe(false);
  });

  it('delivers unread agent notifications in hook briefing without acking on fetch', () => {
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
    expect(second.notifications.some((n) => n.kind === 'notification')).toBe(true);
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
    expect(Object.keys(result).sort()).toEqual(['archived_memories','dry_run','fts_rebuilt','ok','pruned_locks','pruned_old','pruned_refinements','would_archive','would_prune_locks','would_prune_old','would_prune_refinements']);
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
  it('prunes old handoffs and completed refinements while keeping active repo fixes', () => {
    const db = freshDb();
    const old = new Date(Date.now() - 45 * 86400000).toISOString();
    const fresh = new Date().toISOString();
    const handoff = insertRefinement(db, { reasoning: 'handoff', remember: 'Review session handoff for agent', quality: 'handoff' }).refinementId;
    const done = insertRefinement(db, { reasoning: 'done', remember: 'done fix', quality: 'bad', state: 'done' }).refinementId;
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

});

describe('getWorkspaceStatus', () => {
  it('returns ok:true with counts and locks', () => {
    const db = freshDb();
    const result = getWorkspaceStatus(db, {});
    expect(result.ok).toBe(true);
    expect(typeof result.active_memories).toBe('number');
    expect(typeof result.pending_tasks).toBe('number');
    expect(typeof result.active_tasks).toBe('number');
    expect(typeof result.open_refinements).toBe('number');
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

describe('sessionCapture', () => {
  it('returns captured=false when there is no unresolved session state', () => {
    const db = freshDb();
    const { dir, cleanup } = tempFile();
    try {
      const result = sessionCapture(db, { workspace: dir, agent_id: 'agent' });
      expect(result.ok).toBe(true);
      expect(result.captured).toBe(false);
      expect(result.refinement_id).toBeNull();
    } finally {
      cleanup();
    }
  });

  it('records unresolved intents as an open handoff refinement', () => {
    const db = freshDb();
    const { path, cleanup } = tempFile();
    try {
      const claim = preFlightIntent(db, {
        agentId: 'agent-a',
        workspacePath: process.cwd(),
        targetFiles: [path],
        rationale: 'session work in progress',
        testPlan: 'run focused verification',
      });
      expect(claim.ok).toBe(true);

      const result = sessionCapture(db, {
        agent_id: 'agent-a',
        workspace: process.cwd(),
        reason: 'quit',
      });

      expect(result.ok).toBe(true);
      expect(result.captured).toBe(true);
      expect(result.refinement_id).toMatch(/^ref_/);
      expect(result.active_tasks).toBe(1);
      expect(result.files).toContain(path);

      const refinement = db.prepare(
        'SELECT remember, quality, state, files_json FROM refinements WHERE refinement_id = ?'
      ).get(result.refinement_id) as { remember: string; quality: string; state: string; files_json: string };
      expect(refinement.quality).toBe('handoff');
      expect(refinement.state).toBe('open');
      expect(refinement.remember).toContain('Review session handoff for agent-a');
      expect(JSON.parse(refinement.files_json)).toContain(path);
    } finally {
      cleanup();
    }
  });
});

describe('waitForLock', () => {
  it('returns ok=true and immediate', () => {
    const db = freshDb();
    const result = waitForLock(db, {});
    expect(result.ok).toBe(true);
    expect(result.waited_ms).toBe(0);
    expect(result.lock_free).toBe(true);
  });

  it('returns lock_free=false when conflicts remain after timeout', () => {
    const db = freshDb();
    preFlightIntent(db, { agentId: 'holder', targetFiles: ['/tmp/locked.ts'] });
    const result = waitForLock(db, {
      agent_id: 'waiter',
      target_files: ['/tmp/locked.ts'],
      wait_ms: 0,
      retry_interval_ms: 1,
    });
    expect(result.lock_free).toBe(false);
    expect(result.conflicts?.[0]?.agent_id).toBe('holder');
  });

  it('polls until the bounded wait expires when conflicts remain', () => {
    const db = freshDb();
    preFlightIntent(db, { agentId: 'holder', targetFiles: ['/tmp/polling-locked.ts'] });

    const result = waitForLock(db, {
      agent_id: 'waiter',
      target_files: ['/tmp/polling-locked.ts'],
      wait_ms: 5,
      retry_interval_ms: 2,
    });

    expect(result.lock_free).toBe(false);
    expect(result.waited_ms).toBeGreaterThanOrEqual(1);
    expect(result.conflicts?.[0]?.file_path).toBe('/tmp/polling-locked.ts');
  });
});

describe('exportHarness', () => {
  it('returns empty when no high-importance memories exist', () => {
    const db = freshDb();
    const result = exportHarness(db, { min_importance: 7 });
    expect(result.count).toBe(0);
    expect(result.memories).toEqual([]);
    expect(result.markdown).toContain('No harness or high-importance memories');
  });

  it('returns memories at or above min_importance threshold', () => {
    const db = freshDb();
    insertMemory(db, {
      agentId: 'agent1',
      taskContext: 'ctx',
      observation: 'high-imp lesson',
      label: 'GOTCHA',
      importance: 9,
    });
    insertMemory(db, {
      agentId: 'agent1',
      taskContext: 'ctx2',
      observation: 'low-imp lesson',
      label: 'OTHER',
      importance: 3,
    });
    const result = exportHarness(db, { min_importance: 7, limit: 10 });
    expect(result.count).toBe(1);
    expect(result.memories[0]?.observation).toBe('high-imp lesson');
    expect(result.memories[0]?.importance).toBe(9);
    expect(result.markdown).toContain('[GOTCHA:9]');
    expect(result.markdown).not.toContain('low-imp lesson');
  });

  it('respects limit parameter', () => {
    const db = freshDb();
    for (let i = 0; i < 5; i++) {
      insertMemory(db, {
        agentId: 'agent1',
        taskContext: `ctx${i}`,
        observation: `lesson ${i}`,
        label: 'DECISION',
        importance: 8,
      });
    }
    const result = exportHarness(db, { min_importance: 7, limit: 2 });
    expect(result.count).toBe(2);
    expect(result.memories).toHaveLength(2);
  });

  it('markdown block contains auto-generated header', () => {
    const db = freshDb();
    insertMemory(db, {
      agentId: 'a',
      taskContext: 'ctx',
      observation: 'test observation',
      label: 'BUG',
      importance: 8,
    });
    const result = exportHarness(db, { min_importance: 7 });
    expect(result.markdown).toContain('Agent lessons (generated by octocode-awareness');
    expect(result.markdown).toContain('[BUG:8]');
  });
});
