import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { preFlightIntent } from '../src/intents.js';
import { sessionCapture, waitForLock, exportHarness } from '../src/maintenance.js';
import { insertMemory } from '../src/memory.js';
import { canonicalizePath } from '../src/git.js';
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
      expect(result.active_runs).toBe(1);
      expect(result.files).toContain(canonicalizePath(path));

      const refinement = db.prepare(
        'SELECT remember, quality, state, files_json FROM refinements WHERE refinement_id = ?'
      ).get(result.refinement_id) as { remember: string; quality: string; state: string; files_json: string };
      expect(refinement.quality).toBe('handoff');
      expect(refinement.state).toBe('open');
      expect(refinement.remember).toContain('Review session handoff for agent-a');
      expect(JSON.parse(refinement.files_json)).toContain(canonicalizePath(path));
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
    expect(result.conflicts?.[0]?.file_path).toBe(canonicalizePath('/tmp/polling-locked.ts'));
  });

  it('treats non-finite direct wait values as an immediate bounded check', () => {
    const db = freshDb();
    preFlightIntent(db, { agentId: 'holder', targetFiles: ['/tmp/non-finite-wait.ts'] });

    const result = waitForLock(db, {
      agent_id: 'waiter',
      target_files: ['/tmp/non-finite-wait.ts'],
      wait_ms: Number.POSITIVE_INFINITY,
      retry_interval_ms: 1,
    });

    expect(result.lock_free).toBe(false);
    expect(result.waited_ms).toBeLessThan(100);
    expect(result.conflicts?.[0]?.agent_id).toBe('holder');
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
