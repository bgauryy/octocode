/**
 * maintenance.test.ts — Behavioural tests for maintenance functions against the current schema.
 *
 * Core tables: memories, tasks, locks.
 * Core columns: importance, run_id, tags_json, memory_refs.
 */
import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { initDb, replaceMemoryReferences } from '../src/db.js';
import { exportHarness, exportMemoryDoc, sessionCapture } from '../src/maintenance.js';
// ─── Helpers ──────────────────────────────────────────────────────────────────
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
/** Insert a memory using the memories table. */
function insertMem(db: DatabaseSync, opts: {
    memoryId?: string;
    importance?: number;
    label?: string;
    tags?: string[];
    failureSig?: string;
    observation?: string;
    workspacePath?: string | null;
} = {}): string {
    const memoryId = opts.memoryId ?? 'mem_' + randomUUID().replace(/-/g, '');
    const now = new Date().toISOString();
    db.prepare(`
    INSERT INTO memories (
      memory_id, agent_id, task_context, observation, importance,
      label, tags_json, workspace_path, failure_signature, created_at
    ) VALUES (?, 'agent-test', 'test context', ?, ?, ?, ?, ?, ?, ?)
  `).run(memoryId, opts.observation ?? 'test observation', opts.importance ?? 5, opts.label ?? 'OTHER', JSON.stringify(opts.tags ?? []), opts.workspacePath ?? null, opts.failureSig ?? null, now);
    return memoryId;
}
/** Insert an ACTIVE task and return its run_id. */
function insertTask(db: DatabaseSync, opts: {
    agentId?: string;
    workspacePath?: string;
    sessionId?: string | null;
    planDocRef?: string | null;
} = {}): string {
    const runId = 'task_' + randomUUID().replace(/-/g, '');
    const now = new Date().toISOString();
    db.prepare(`
    INSERT INTO task_runs (run_id, origin, agent_id, rationale, test_plan, context_ref, status, workspace_path, created_at, updated_at)
    VALUES (?, 'WORK', ?, 'test rationale', 'yarn test', ?, 'ACTIVE', ?, ?, ?)
  `).run(runId, opts.agentId ?? 'agent-test', opts.planDocRef ?? null, opts.workspacePath ?? '/ws', now, now);
    return runId;
}

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
