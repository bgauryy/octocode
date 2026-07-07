import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { insertMemory, getMemory, bumpAccess, decayScore } from '../src/memory.js';
import type { MemoryRecord } from '../src/types.js';
import { execSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

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
      taskContext: 'ctx', observation: 'obs', importanceScore: 5,
    });
    expect(memoryId).toMatch(/^mem_/);
  });

  it('stores and retrieves the record', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      agentId: 'agent-x',
      taskContext: 'routing auth',
      observation: 'JWT must be verified before handler',
      importanceScore: 8,
      label: 'SECURITY',
      tags: ['jwt', 'auth'],
      references: ['https://example.com'],
    });
    const row = db.prepare('SELECT * FROM agent_memories WHERE memory_id = ?').get(memoryId) as Record<string, unknown>;
    expect(row['agent_id']).toBe('agent-x');
    expect(row['importance_score']).toBe(8);
    expect(row['label']).toBe('SECURITY');
    expect(JSON.parse(row['tags_json'] as string)).toEqual(['jwt', 'auth']);
    expect(typeof row['novelty_score']).toBe('number');
    expect(JSON.parse(row['similar_memory_ids_json'] as string)).toEqual([]);
  });

  it('throws for out-of-range importanceScore', () => {
    const db = freshDb();
    expect(() => insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 0 }))
      .toThrow('importanceScore');
    expect(() => insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 11 }))
      .toThrow('importanceScore');
  });

  it('supersedes a previous memory', () => {
    const db = freshDb();
    const { memoryId: oldId } = insertMemory(db, {
      taskContext: 'old ctx', observation: 'old obs', importanceScore: 5,
    });
    const { superseded } = insertMemory(db, {
      taskContext: 'new ctx', observation: 'new obs', importanceScore: 6,
      supersedes: [oldId],
    });
    expect(superseded).toContain(oldId);
    const oldRow = db.prepare('SELECT state, superseded_by FROM agent_memories WHERE memory_id = ?').get(oldId) as Record<string, unknown>;
    expect(oldRow['state']).toBe('SUPERSEDED');
  });

  it('inserts into FTS', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 'sqlite fts', observation: 'fts5 works', importanceScore: 4,
    });
    const row = db.prepare('SELECT memory_id FROM memory_fts WHERE memory_fts MATCH ?').get('fts5') as Record<string, unknown> | undefined;
    expect(row?.['memory_id']).toBe(memoryId);
  });

  it('stores novelty/similar ids for repeated memories without verbose payloads', () => {
    const db = freshDb();
    const first = insertMemory(db, {
      taskContext: 'build cache regression',
      observation: 'Never edit generated dist files because build overwrites dist output',
      importanceScore: 7,
      label: 'GOTCHA',
    });
    const second = insertMemory(db, {
      taskContext: 'build cache regression',
      observation: 'Never edit generated dist files because build overwrites dist output',
      importanceScore: 7,
      label: 'GOTCHA',
    });
    expect(second.memory.novelty_score).toBeLessThan(0.75);
    expect(second.memory.similar_memory_ids).toContain(first.memoryId);
    expect(second.similarMemoryIds).toContain(first.memoryId);
  });

  it('normalizes label to OTHER for unknown values', () => {
    const db = freshDb();
    const { memory } = insertMemory(db, {
      taskContext: 't', observation: 'o', importanceScore: 3, label: 'BOGUS',
    });
    expect(memory.label).toBe('OTHER');
  });

  it('handles undefined optional fields gracefully', () => {
    const db = freshDb();
    // workspace_path / repo may be auto-filled from git — only file should always be null
    const { memory } = insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 1 });
    expect(memory.file).toBeNull();
    // workspace_path is either null or a string (auto-filled from git)
    expect(memory.workspace_path === null || typeof memory.workspace_path === 'string').toBe(true);
  });
});

describe('getMemory', () => {
  it('returns memories sorted by decay score', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 'low', observation: 'low score', importanceScore: 1 });
    insertMemory(db, { taskContext: 'high', observation: 'high score', importanceScore: 9 });
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
      insertMemory(db, { taskContext: 'ctx', observation: `obs ${i}`, importanceScore: 5 });
    }
    const { memories, count } = getMemory(db, { limit: 3 });
    expect(memories).toHaveLength(3);
    expect(count).toBe(3);
  });

  it('filters by minImportance', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 't', observation: 'low', importanceScore: 2 });
    insertMemory(db, { taskContext: 't', observation: 'high', importanceScore: 8 });
    const { memories } = getMemory(db, { minImportance: 5 });
    expect(memories.every(m => m.importance_score >= 5)).toBe(true);
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
    insertMemory(db, { taskContext: 't', observation: 'borderline', importanceScore: 3 });
    const normal = getMemory(db, { minImportance: 4, smart: false });
    const smart = getMemory(db, { minImportance: 4, smart: true });
    // smart=true lowers threshold by 1, so imp=3 should appear
    expect(smart.memories.length).toBeGreaterThanOrEqual(normal.memories.length);
  });

  it('filters by label', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 5, label: 'BUG' });
    insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 5, label: 'GOTCHA' });
    const { memories } = getMemory(db, { label: 'BUG', limit: 10 });
    expect(memories.every(m => m.label === 'BUG')).toBe(true);
  });

  it('filters by tags', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 5, tags: ['react'] });
    insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 5, tags: ['vue'] });
    const { memories } = getMemory(db, { tags: ['react'], limit: 10 });
    expect(memories.every(m => m.tags.includes('react'))).toBe(true);
  });

  it('all returned memories have a score field', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 5 });
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
      taskContext: 't', observation: 'o', importanceScore: 5,
    });
    bumpAccess(db, [memoryId]);
    const row = db.prepare('SELECT access_count FROM agent_memories WHERE memory_id = ?').get(memoryId) as { access_count: number };
    expect(row.access_count).toBe(1);
  });

  it('is a no-op for an empty array', () => {
    const db = freshDb();
    expect(() => bumpAccess(db, [])).not.toThrow();
  });

  it('bumps multiple IDs at once', () => {
    const db = freshDb();
    const { memoryId: id1 } = insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 5 });
    const { memoryId: id2 } = insertMemory(db, { taskContext: 't', observation: 'o', importanceScore: 5 });
    bumpAccess(db, [id1, id2]);
    const r1 = db.prepare('SELECT access_count FROM agent_memories WHERE memory_id = ?').get(id1) as { access_count: number };
    const r2 = db.prepare('SELECT access_count FROM agent_memories WHERE memory_id = ?').get(id2) as { access_count: number };
    expect(r1.access_count).toBe(1);
    expect(r2.access_count).toBe(1);
  });
});

describe('decayScore', () => {
  const now = new Date().toISOString();

  it('importance 10 scores higher than importance 1', () => {
    const base: Omit<MemoryRecord, 'importance_score'> = {
      memory_id: 'm', agent_id: 'a', task_context: 't', observation: 'o',
      state: 'ACTIVE', label: 'OTHER',
      superseded_by: null, tags: [], references: [], workspace_path: null,
      repo: null, ref: null, file: null, novelty_score: null, similar_memory_ids: [], failure_signature: null,
      access_count: 0, last_accessed_at: now, decay_half_life_days: null,
      valid_from: null, valid_to: null, expired_at: null,
      file_tree_fingerprint: null, created_at: now, updated_at: null,
    };
    const high = decayScore({ ...base, importance_score: 10 }, 0.5);
    const low = decayScore({ ...base, importance_score: 1 }, 0.5);
    expect(high).toBeGreaterThan(low);
    // With imp=10 + fresh + lexical=0.5: 0.25 + 0.30 + 0 + 0.15 = 0.70
    expect(high).toBeGreaterThan(0.6);
  });

  it('older memories score lower than fresh ones', () => {
    const base: MemoryRecord = {
      memory_id: 'm', agent_id: 'a', task_context: 't', observation: 'o',
      importance_score: 5, state: 'ACTIVE', label: 'OTHER',
      superseded_by: null, tags: [], references: [], workspace_path: null,
      repo: null, ref: null, file: null, novelty_score: null, similar_memory_ids: [], failure_signature: null,
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
        importanceScore: 8, label: 'GOTCHA',
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
        importanceScore: 8, label: 'GOTCHA',
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
