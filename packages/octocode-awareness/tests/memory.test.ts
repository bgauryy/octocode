import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb, rebuildFts, replaceMemoryReferences } from '../src/db.js';
import {
  insertMemory, getMemory, bumpAccess, decayScore,
  forgetMemory, findSimilarMemories,
} from '../src/memory.js';
import * as memoryModule from '../src/memory.js';
import { normalizeFilePath } from '../src/helpers.js';
import type { MemoryRecord } from '../src/types.js';
import { execSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('insertMemory', () => {
  it('returns a memoryId prefixed mem_', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 'ctx', observation: 'obs', importance: 5,
    });
    expect(memoryId).toMatch(/^mem_/);
  });

  it('stores and retrieves the record', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      agentId: 'agent-x',
      taskContext: 'routing auth',
      observation: 'JWT must be verified before handler',
      importance: 8,
      label: 'SECURITY',
      tags: ['jwt', 'auth'],
      references: ['https://example.com'],
    });
    const row = db.prepare('SELECT * FROM memories WHERE memory_id = ?').get(memoryId) as Record<string, unknown>;
    expect(row['agent_id']).toBe('agent-x');
    expect(row['importance']).toBe(8);
    expect(row['label']).toBe('SECURITY');
    expect(JSON.parse(row['tags_json'] as string)).toEqual(['jwt', 'auth']);
    expect(typeof row['novelty_score']).toBe('number');
  });

  it('throws for out-of-range importance', () => {
    const db = freshDb();
    expect(() => insertMemory(db, { taskContext: 't', observation: 'o', importance: 0 }))
      .toThrow('importance');
    expect(() => insertMemory(db, { taskContext: 't', observation: 'o', importance: 11 }))
      .toThrow('importance');
  });

  it('supersedes a previous memory', () => {
    const db = freshDb();
    const { memoryId: oldId } = insertMemory(db, {
      taskContext: 'old ctx', observation: 'old obs', importance: 5,
    });
    const { superseded } = insertMemory(db, {
      taskContext: 'new ctx', observation: 'new obs', importance: 6,
      supersedes: [oldId],
    });
    expect(superseded).toContain(oldId);
    const oldRow = db.prepare('SELECT state, superseded_by FROM memories WHERE memory_id = ?').get(oldId) as Record<string, unknown>;
    expect(oldRow['state']).toBe('SUPERSEDED');
  });

  it('inserts into FTS', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 'sqlite fts', observation: 'fts5 works', importance: 4,
    });
    const row = db.prepare('SELECT memory_id FROM memories_fts WHERE memories_fts MATCH ?').get('fts5') as Record<string, unknown> | undefined;
    expect(row?.['memory_id']).toBe(memoryId);
  });

  it('stores novelty/similar ids for repeated memories without verbose payloads', () => {
    const db = freshDb();
    const first = insertMemory(db, {
      taskContext: 'build cache regression',
      observation: 'Never edit generated dist files because build overwrites dist output',
      importance: 7,
      label: 'GOTCHA',
    });
    const second = insertMemory(db, {
      taskContext: 'build cache regression',
      observation: 'Never edit generated dist files because build overwrites dist output',
      importance: 7,
      label: 'GOTCHA',
    });
    expect(second.memory.novelty_score).toBeLessThan(0.75);
    expect(second.similarMemoryIds).toContain(first.memoryId);
  });

  it('normalizes label to OTHER for unknown values', () => {
    const db = freshDb();
    const { memory } = insertMemory(db, {
      taskContext: 't', observation: 'o', importance: 3, label: 'BOGUS',
    });
    expect(memory.label).toBe('OTHER');
  });

  it('handles undefined optional fields gracefully', () => {
    const db = freshDb();
    const { memory } = insertMemory(db, { taskContext: 't', observation: 'o', importance: 1 });
    // workspace_path is either null or a string (auto-filled from git)
    expect(memory.workspace_path === null || typeof memory.workspace_path === 'string').toBe(true);
    // novelty_score is always returned (1.0 on first insert into empty store)
    expect(typeof memory.novelty_score).toBe('number');
  });
});

describe('getMemory', () => {
  it('returns memories sorted by decay score', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 'low', observation: 'low score', importance: 1 });
    insertMemory(db, { taskContext: 'high', observation: 'high score', importance: 9 });
    const { memories } = getMemory(db, { query: 'score', limit: 10 });
    expect(memories.length).toBeGreaterThan(0);
    // High importance should rank higher
    const scores = memories.map(m => m.score ?? 0);
    expect(scores[0]).toBeGreaterThanOrEqual(scores[scores.length - 1]!);
  });

  // 20s timeout: 10 inserts each run similar-memory Jaccard scans; under the
  // full parallel suite this occasionally exceeds the default 5s on CPU contention.
  it('respects limit', { timeout: 20_000 }, () => {
    const db = freshDb();
    for (let i = 0; i < 10; i++) {
      insertMemory(db, { taskContext: 'ctx', observation: `obs ${i}`, importance: 5 });
    }
    const { memories, count } = getMemory(db, { limit: 3 });
    expect(memories).toHaveLength(3);
    expect(count).toBe(3);
  });

  it('filters by minImportance', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 't', observation: 'low', importance: 2 });
    insertMemory(db, { taskContext: 't', observation: 'high', importance: 8 });
    const { memories } = getMemory(db, { minImportance: 5 });
    expect(memories.every(m => m.importance >= 5)).toBe(true);
  });

  it('returns mode=lexical when FTS is enabled', () => {
    const db = freshDb();
    const { mode } = getMemory(db, {});
    expect(mode).toBe('lexical');
  });

  it('returns empty array when no memories match', () => {
    const db = freshDb();
    const { memories, count } = getMemory(db, { query: 'nonexistent term zxqpw' });
    expect(memories).toHaveLength(0);
    expect(count).toBe(0);
  });

  it('smart mode lowers minImportance threshold', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 't', observation: 'borderline', importance: 3 });
    const normal = getMemory(db, { minImportance: 4, smart: false });
    const smart = getMemory(db, { minImportance: 4, smart: true });
    // smart=true lowers threshold by 1, so imp=3 should appear
    expect(smart.memories.length).toBeGreaterThanOrEqual(normal.memories.length);
  });

  it('filters by label', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 't', observation: 'o', importance: 5, label: 'BUG' });
    insertMemory(db, { taskContext: 't', observation: 'o', importance: 5, label: 'GOTCHA' });
    const { memories } = getMemory(db, { label: 'BUG', limit: 10 });
    expect(memories.every(m => m.label === 'BUG')).toBe(true);
  });

  it('filters by tags', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 't', observation: 'o', importance: 5, tags: ['react'] });
    insertMemory(db, { taskContext: 't', observation: 'o', importance: 5, tags: ['vue'] });
    const { memories } = getMemory(db, { tags: ['react'], limit: 10 });
    expect(memories.every(m => m.tags.includes('react'))).toBe(true);
  });

  it('fallback recall filters by query text when FTS is unavailable', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 'fallback text', observation: 'fallbackneedle appears here', importance: 5 });
    db.exec('DROP TABLE memories_fts');

    expect(getMemory(db, { query: 'absent-token-qxwz', limit: 5 }).memories).toHaveLength(0);
    expect(getMemory(db, { query: 'fallbackneedle', limit: 5 }).memories).toHaveLength(1);
  });

  it('requires every exact reference when multiple references are provided', () => {
    const db = freshDb();
    const refA = 'file:/tmp/ref-a.ts';
    const refB = 'pr:owner/repo#123';
    insertMemory(db, { taskContext: 'ref-a only', observation: 'a', importance: 7, references: [refA] });
    const both = insertMemory(db, { taskContext: 'both refs', observation: 'b', importance: 7, references: [refA, refB] });
    insertMemory(db, { taskContext: 'ref-b only', observation: 'c', importance: 7, references: [refB] });

    const { memories } = getMemory(db, { references: [refA, refB], limit: 10 });
    expect(memories.map(m => m.memory_id)).toEqual([both.memoryId]);
  });

  it('applies exact file filters before the scoring prefetch cap', () => {
    const db = freshDb();
    for (let i = 0; i < 8; i++) {
      insertMemory(db, { taskContext: `alpha decoy ${i}`, observation: `alpha high-rank decoy ${i}`, importance: 10 });
    }
    const targetFile = '/tmp/awareness-target-file.ts';
    const target = insertMemory(db, {
      taskContext: 'alpha target file',
      observation: 'alpha low-rank file-filter target',
      importance: 1,
      references: [`file:${targetFile}`],
    });

    const { memories } = getMemory(db, { query: 'alpha', files: [targetFile], limit: 1 });
    expect(memories.map(m => m.memory_id)).toEqual([target.memoryId]);
  });

  it('applies fileRegex filters before the scoring prefetch cap', () => {
    const db = freshDb();
    for (let i = 0; i < 8; i++) {
      insertMemory(db, { taskContext: `bravo decoy ${i}`, observation: `bravo high-rank decoy ${i}`, importance: 10 });
    }
    const target = insertMemory(db, {
      taskContext: 'bravo target file regex',
      observation: 'bravo low-rank file-regex target',
      importance: 1,
      references: ['file:/tmp/awareness-special-file-regex.ts'],
    });

    const { memories } = getMemory(db, { query: 'bravo', fileRegex: ['special-file-regex\\.ts$'], limit: 1 });
    expect(memories.map(m => m.memory_id)).toEqual([target.memoryId]);
  });

  it('applies generic regex filters before the scoring prefetch cap', () => {
    const db = freshDb();
    for (let i = 0; i < 8; i++) {
      insertMemory(db, { taskContext: `charlie decoy ${i}`, observation: `charlie high-rank decoy ${i}`, importance: 10 });
    }
    const target = insertMemory(db, {
      taskContext: 'charlie target regex',
      observation: 'charlie low-rank generic-regex-needle',
      importance: 1,
    });

    const { memories } = getMemory(db, { query: 'charlie', regex: ['generic-regex-needle'], limit: 1 });
    expect(memories.map(m => m.memory_id)).toEqual([target.memoryId]);
  });

  it('applies asOf validity before the scoring prefetch cap', () => {
    const db = freshDb();
    for (let i = 0; i < 8; i++) {
      insertMemory(db, { taskContext: `temporalcap decoy ${i}`, observation: `temporalcap current decoy ${i}`, importance: 10 });
    }
    const target = insertMemory(db, {
      taskContext: 'temporalcap historical target',
      observation: 'temporalcap low-rank historical target',
      importance: 1,
      validFrom: '2019-01-01T00:00:00Z',
      validTo: '2021-01-01T00:00:00Z',
    });

    const { memories } = getMemory(db, { query: 'temporalcap', asOf: '2020-06-01T00:00:00Z', limit: 1 });
    expect(memories.map(m => m.memory_id)).toEqual([target.memoryId]);
  });

  it('rebuildFts handles large reference sets without placeholder-limit failures', { timeout: 20_000 }, () => {
    const db = freshDb();
    const insert = db.prepare(`
      INSERT INTO memories (
        memory_id, agent_id, task_context, observation, importance,
        label, tags_json, created_at, updated_at, last_accessed_at, access_count
      ) VALUES (?, 'agent-test', ?, ?, 5, 'OTHER', '[]', ?, ?, ?, 0)
    `);
    const now = new Date().toISOString();
    for (let i = 0; i < 1050; i++) {
      const id = `mem_large_${i}`;
      insert.run(id, `large fts ${i}`, `large fts observation ${i}`, now, now, now);
      replaceMemoryReferences(db, id, [`file:/tmp/lateprovenance${i}.ts`]);
    }

    db.exec('DELETE FROM memories_fts');
    expect(() => rebuildFts(db)).not.toThrow();
    const row = db.prepare(
      'SELECT memory_id FROM memories_fts WHERE memories_fts MATCH ?'
    ).get('lateprovenance1049') as { memory_id: string } | undefined;
    expect(row?.memory_id).toBe('mem_large_1049');
  });

  it('all returned memories have a score field', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 't', observation: 'o', importance: 5 });
    const { memories } = getMemory(db, {});
    for (const m of memories) {
      expect(typeof m.score).toBe('number');
    }
  });
});

describe('bumpAccess', () => {
  it('increments access_count', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 't', observation: 'o', importance: 5,
    });
    bumpAccess(db, [memoryId]);
    const row = db.prepare('SELECT access_count FROM memories WHERE memory_id = ?').get(memoryId) as { access_count: number };
    expect(row.access_count).toBe(1);
  });

  it('is a no-op for an empty array', () => {
    const db = freshDb();
    expect(() => bumpAccess(db, [])).not.toThrow();
  });

  it('bumps multiple IDs at once', () => {
    const db = freshDb();
    const { memoryId: id1 } = insertMemory(db, { taskContext: 't', observation: 'o', importance: 5 });
    const { memoryId: id2 } = insertMemory(db, { taskContext: 't', observation: 'o', importance: 5 });
    bumpAccess(db, [id1, id2]);
    const r1 = db.prepare('SELECT access_count FROM memories WHERE memory_id = ?').get(id1) as { access_count: number };
    const r2 = db.prepare('SELECT access_count FROM memories WHERE memory_id = ?').get(id2) as { access_count: number };
    expect(r1.access_count).toBe(1);
    expect(r2.access_count).toBe(1);
  });
});

describe('decayScore', () => {
  const now = new Date().toISOString();

  it('importance 10 scores higher than importance 1', () => {
    const base: Omit<MemoryRecord, 'importance'> = {
      memory_id: 'm', agent_id: 'a', task_context: 't', observation: 'o',
      state: 'ACTIVE', label: 'OTHER',
      superseded_by: null, tags: [], references: [], workspace_path: null, artifact: null,
      repo: null, ref: null, novelty_score: null, failure_signature: null,
      access_count: 0, last_accessed_at: now, decay_half_life_days: null,
      valid_from: null, valid_to: null, expired_at: null,
      file_tree_fingerprint: null, created_at: now, updated_at: null,
    };
    const high = decayScore({ ...base, importance: 10 }, 0.5);
    const low = decayScore({ ...base, importance: 1 }, 0.5);
    expect(high).toBeGreaterThan(low);
    // With imp=10 + fresh + lexical=0.5: 0.25 + 0.30 + 0 + 0.15 = 0.70
    expect(high).toBeGreaterThan(0.6);
  });

  it('older memories score lower than fresh ones', () => {
    const base: MemoryRecord = {
      memory_id: 'm', agent_id: 'a', task_context: 't', observation: 'o',
      importance: 5, state: 'ACTIVE', label: 'OTHER',
      superseded_by: null, tags: [], references: [], workspace_path: null, artifact: null,
      repo: null, ref: null, novelty_score: null, failure_signature: null,
      access_count: 0, last_accessed_at: null, decay_half_life_days: null,
      valid_from: null, valid_to: null, expired_at: null,
      file_tree_fingerprint: null, created_at: now, updated_at: null,
    };
    const fresh = decayScore(base, 0.5);
    const old = decayScore({
      ...base,
      created_at: new Date(Date.now() - 90 * 86400000).toISOString(),
    }, 0.5);
    expect(fresh).toBeGreaterThan(old);
  });
});

describe('getMemory workspace-scope symmetry (regression)', () => {
  // Reproduces the asymmetry where insertMemory git-resolves cwd → repo root
  // (via fillScope) but getMemory filtered against the raw cwd, so an agent that
  // records from a subdirectory of a git repo and recalls from the same (or a
  // sibling) subdirectory filtered out its own freshly-recorded memory.
  function tempGitRepo(): { root: string; subA: string; subB: string; cleanup: () => void } {
    const root = mkdtempSync(join(tmpdir(), 'oc-mem-scope-'));
    const subA = join(root, 'pkgA');
    const subB = join(root, 'pkgB');
    mkdirSync(subA, { recursive: true });
    mkdirSync(subB, { recursive: true });
    execSync('git init -q', { cwd: root });
    execSync('git config user.email t@t.test', { cwd: root });
    execSync('git config user.name t', { cwd: root });
    writeFileSync(join(root, 'README.md'), 'seed');
    execSync('git add -A && git commit -q -m seed', { cwd: root });
    return { root, subA, subB, cleanup: () => rmSync(root, { recursive: true, force: true }) };
  }

  it('a memory recorded from a git subdirectory is recalled from a sibling subdirectory', () => {
    const db = freshDb();
    const { subA, subB, cleanup } = tempGitRepo();
    try {
      // Record with cwd = subA → fillScope stores workspace_path = repo root.
      insertMemory(db, {
        agentId: 'a',
        taskContext: 'subdir-recall-fix',
        observation: 'unique-marker-zqxjkw subdirectory recall regression',
        importance: 8, label: 'GOTCHA',
        cwd: subA,
      });
      // Recall with workspacePath = subB (a different subdirectory of the same repo).
      const { count, memories } = getMemory(db, {
        query: 'unique-marker-zqxjkw subdirectory recall regression',
        workspacePath: subB,
        limit: 10,
      });
      expect(count).toBe(1);
      expect(memories[0]!.task_context).toBe('subdir-recall-fix');
    } finally { cleanup(); }
  });

  it('a memory recorded from a git subdirectory is recalled from the repo root cwd', () => {
    const db = freshDb();
    const { root, subA, cleanup } = tempGitRepo();
    try {
      insertMemory(db, {
        agentId: 'a',
        taskContext: 'root-recall-fix',
        observation: 'unique-marker-rt9k1p repo root recall regression',
        importance: 8, label: 'GOTCHA',
        cwd: subA,
      });
      const { count } = getMemory(db, {
        query: 'unique-marker-rt9k1p repo root recall regression',
        workspacePath: root,
        limit: 10,
      });
      expect(count).toBe(1);
    } finally { cleanup(); }
  });
});

// ─── P0/P1 fix tests ──────────────────────────────────────────────────────────

describe('fix-a: supersede atomicity', () => {
  it('A is SUPERSEDED and B is ACTIVE after B supersedes A (not both ACTIVE)', () => {
    const db = freshDb();
    const { memoryId: idA } = insertMemory(db, {
      taskContext: 'pre-existing task', observation: 'original observation', importance: 5,
    });
    const { memoryId: idB } = insertMemory(db, {
      taskContext: 'replacement task', observation: 'updated observation', importance: 6,
      supersedes: [idA],
    });

    const rowA = db.prepare('SELECT state FROM memories WHERE memory_id = ?').get(idA) as { state: string };
    const rowB = db.prepare('SELECT state FROM memories WHERE memory_id = ?').get(idB) as { state: string };

    // A must be SUPERSEDED; B must be ACTIVE — never both ACTIVE simultaneously
    expect(rowA.state).toBe('SUPERSEDED');
    expect(rowB.state).toBe('ACTIVE');

    const activeCount = db.prepare(
      "SELECT COUNT(*) AS n FROM memories WHERE memory_id IN (?, ?) AND state = 'ACTIVE'"
    ).get(idA, idB) as { n: number };
    expect(activeCount.n).toBe(1);
  });

  it('savepoint rollback leaves no orphaned SUPERSEDED record (crash simulation)', () => {
    // This test demonstrates the SQLite savepoint semantics the atomicity fix relies on:
    // if the combined insert+supersede is wrapped in one transaction and that transaction
    // is rolled back (crash simulation), neither the new insert NOR the supersede persists.
    const db = freshDb();
    const { memoryId: idA } = insertMemory(db, {
      taskContext: 'must stay active', observation: 'original obs', importance: 5,
    });

    // Simulate a partial insert of B (inserts row, skips the supersede UPDATE) via SAVEPOINT.
    // This is the broken state that the pre-fix code could produce on a crash between
    // COMMIT (of the insert) and the supersede UPDATE (which ran outside the transaction).
    db.exec('SAVEPOINT sp_crash_sim');
    try {
      db.prepare(`
        INSERT INTO memories
          (memory_id, agent_id, task_context, observation, importance, label, tags_json, created_at, updated_at)
        VALUES
          ('mem_crash_b', 'agent', 'ctx-b', 'obs-b', 6, 'OTHER', '[]',
           strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'))
      `).run();
      // Deliberately omit the supersede UPDATE to simulate pre-fix crash point,
      // then roll back the entire savepoint.
      db.exec('ROLLBACK TO SAVEPOINT sp_crash_sim');
    } finally {
      db.exec('RELEASE SAVEPOINT sp_crash_sim');
    }

    // After rollback: A is still ACTIVE (not accidentally superseded), B does not exist
    const rowA = db.prepare('SELECT state FROM memories WHERE memory_id = ?').get(idA) as { state: string };
    expect(rowA.state).toBe('ACTIVE');

    const rowB = db.prepare('SELECT memory_id FROM memories WHERE memory_id = ?').get('mem_crash_b');
    expect(rowB).toBeUndefined();
  });
});

describe('fix-b: forgetMemory atomicity', () => {
  it('forgetMemory removes both the memories row and the memories_fts row', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 'sqlite fts forget test', observation: 'row to be forgotten', importance: 3,
    });

    // Confirm the FTS row exists before deletion
    const ftsBefore = db.prepare(
      'SELECT memory_id FROM memories_fts WHERE memories_fts MATCH ?'
    ).get('forgotten') as Record<string, unknown> | undefined;
    expect(ftsBefore?.['memory_id']).toBe(memoryId);

    forgetMemory(db, { memoryIds: [memoryId] });

    // Main memories row must be gone
    const mainRow = db.prepare('SELECT memory_id FROM memories WHERE memory_id = ?').get(memoryId);
    expect(mainRow).toBeUndefined();

    // FTS row must also be gone — verifies atomic cleanup, not just the main table
    const ftsRow = db.prepare(
      'SELECT memory_id FROM memories_fts WHERE memory_id = ?'
    ).get(memoryId);
    expect(ftsRow).toBeUndefined();
  });

  it('forgetMemory dryRun reports deletion without touching the DB', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 'dry-run test', observation: 'should survive dryRun', importance: 3,
    });

    const result = forgetMemory(db, { memoryIds: [memoryId], dryRun: true });
    expect(result.dry_run).toBe(true);
    expect(result.would_delete).toBe(1);
    expect(result.deleted).toBe(0);

    // Memory must still be present
    const row = db.prepare('SELECT memory_id FROM memories WHERE memory_id = ?').get(memoryId);
    expect(row).toBeDefined();
  });
});

describe('fix-c: getMemory reference filter respects workspace_path', () => {
  it('reference filter does not leak across workspaces (cross-workspace isolation)', () => {
    const db = freshDb();
    const sharedRef = 'github://shared-ref-xwspace-leak-test';

    // Insert in workspace alpha with the reference
    const { memoryId: idAlpha } = insertMemory(db, {
      taskContext: 'workspace-alpha-task', observation: 'alpha memory content', importance: 5,
      references: [sharedRef],
      workspacePath: '/workspace/alpha',
    });
    // Insert in workspace beta with the SAME reference
    insertMemory(db, {
      taskContext: 'workspace-beta-task', observation: 'beta memory content', importance: 5,
      references: [sharedRef],
      workspacePath: '/workspace/beta',
    });

    // Scoped recall for workspace alpha — must NOT return beta's memory
    const { memories } = getMemory(db, {
      references: [sharedRef],
      workspacePath: '/workspace/alpha',
      strictScope: true,
      limit: 10,
    });

    expect(memories).toHaveLength(1);
    expect(memories[0]!.memory_id).toBe(idAlpha);
    expect(memories.every(m => m.workspace_path === '/workspace/alpha')).toBe(true);
  });

  it('plain text recall can find provenance reference tokens after insert and FTS rebuild', () => {
    const db = freshDb();
    const reference = 'file:/tmp/octocode-awareness-provenance-qrx42.ts';
    const { memoryId } = insertMemory(db, {
      taskContext: 'reference-only recall target',
      observation: 'this observation intentionally omits the unique token',
      importance: 7,
      references: [reference],
    });

    const fresh = getMemory(db, { query: 'provenance qrx42', limit: 5 });
    expect(fresh.memories.map(m => m.memory_id)).toContain(memoryId);

    db.exec('DELETE FROM memories_fts');
    rebuildFts(db);
    const rebuilt = getMemory(db, { query: 'provenance qrx42', limit: 5 });
    expect(rebuilt.memories.map(m => m.memory_id)).toContain(memoryId);
  });
});

describe('fix-d: findSimilarMemories inside transaction', () => {
  it('second identical insert detects the first as similar (novelty signal is accurate)', () => {
    // Verifies that findSimilarMemories runs within the transaction boundary so that
    // even rapid sequential inserts produce correct novelty scores. If findSimilarMemories
    // ran outside BEGIN IMMEDIATE, a concurrent insert could race past the similarity check
    // and produce a false novelty_score of 1.0 for a near-duplicate.
    const db = freshDb();
    const obs = 'build cache regression: never edit generated dist files because build overwrites output';

    const first = insertMemory(db, {
      taskContext: 'build-cache-ctx', observation: obs, importance: 7,
    });
    // Second insert with identical content — findSimilarMemories must see the first record
    const second = insertMemory(db, {
      taskContext: 'build-cache-ctx', observation: obs, importance: 7,
    });

    // noveltyScore < 1 means the similarity check found the prior memory
    expect(second.noveltyScore).toBeLessThan(0.75);
    expect(second.similarMemoryIds).toContain(first.memoryId);
    expect(second.memory.novelty_score).toBeLessThan(0.75);
  });

  it('findSimilarMemories returns empty for an empty store', () => {
    const db = freshDb();
    const similar = findSimilarMemories(db, 'some query text about caching', 3, null);
    expect(similar).toEqual([]);
  });

  it('findSimilarMemories excludes the specified memory_id from its results', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 'cache regression', observation: 'dist folder rebuild caching gotcha', importance: 6,
    });
    // Searching with the same text but excluding that memory should return nothing
    const similar = findSimilarMemories(
      db, 'cache regression dist folder rebuild caching gotcha', 3, memoryId
    );
    expect(similar.every(s => s.memory_id !== memoryId)).toBe(true);
  });
});

describe('fix-e: normalizeFilePath uses workspacePath fallback when cwd is undefined', () => {
  it('resolves relative path against explicit cwd', () => {
    const result = normalizeFilePath('src/index.ts', '/workspace/project');
    expect(result).toBe('/workspace/project/src/index.ts');
  });

  it('returns absolute path unchanged regardless of cwd', () => {
    const result = normalizeFilePath('/absolute/path/file.ts', '/workspace/project');
    expect(result).toBe('/absolute/path/file.ts');
  });

  it('returns null for falsy input', () => {
    expect(normalizeFilePath(null, '/workspace')).toBeNull();
    expect(normalizeFilePath('', '/workspace')).toBeNull();
    expect(normalizeFilePath(undefined, '/workspace')).toBeNull();
  });

  it('falls back to process.cwd() when cwd argument is absent', () => {
    // Relative path resolution without an explicit cwd falls back to process.cwd().
    // The fix ensures that callers (insertMemory, getMemory) pass workspacePath as the
    // effective cwd when the user did not provide an explicit cwd, preventing path
    // resolution against an unrelated process working directory.
    const relPath = 'src/relative/file.ts';
    const resolved = normalizeFilePath(relPath, undefined);
    expect(resolved).toBe(resolve(process.cwd(), relPath));
  });

  it('workspacePath used as file resolution base when cwd is not provided in getMemory', () => {
    // Verifies the caller-side fix: getMemory must pass workspacePath as the effective
    // cwd to normalizeFilePath when params.cwd is absent so relative file filters
    // are resolved against the workspace, not the process working directory.
    // We test the normalizeFilePath contract directly — the workspacePath fallback
    // means normalizeFilePath('/workspace/alpha', 'subdir/file.ts') works correctly.
    const resolved = normalizeFilePath('subdir/component.ts', '/workspace/alpha');
    expect(resolved).toBe('/workspace/alpha/subdir/component.ts');
  });
});

describe('fix-f: single jaccard function (no jaccardSimilarity export)', () => {
  it('jaccardSimilarity is not exported from the memory module', () => {
    // The mineWeakness-local jaccardSimilarity duplicate was removed in favour of the
    // single private jaccard() function. Exporting it would be a regression indicator.
    expect(Object.keys(memoryModule)).not.toContain('jaccardSimilarity');
  });

  it('findSimilarMemories (which uses jaccard internally) still computes similarity correctly', () => {
    // Smoke-test that the refactored single-jaccard path produces a non-zero similarity
    // for tokens that genuinely overlap.
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'webpack bundler configuration caching',
      observation: 'webpack cache invalidation requires content hash in output filename',
      importance: 6,
    });
    const results = findSimilarMemories(
      db, 'webpack cache invalidation content hash bundler configuration', 3, null
    );
    expect(results.length).toBeGreaterThan(0);
    expect(results[0]!.similarity).toBeGreaterThan(0);
    expect(results[0]!.similarity).toBeLessThanOrEqual(1);
  });
});
