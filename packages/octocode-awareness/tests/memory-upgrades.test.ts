import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { insertMemory, getMemory, forgetMemory } from '../src/memory.js';

/**
 * 2026-07-07 upgrade set from the agentic-memory research pass:
 * weak-pool relevance squash, judgment_required flag, per-label decay
 * defaults, and the salience floor on broad forget selectors.
 */

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function seedCorpus(db: DatabaseSync): void {
  insertMemory(db, { taskContext: 'auth router refactor', observation: 'tenant id normalization order matters for policy lookup', importanceScore: 7, label: 'GOTCHA', tags: ['auth'] });
  insertMemory(db, { taskContext: 'build pipeline', observation: 'esbuild banner suppresses the sqlite warning noise', importanceScore: 5, label: 'BUILD' });
  insertMemory(db, { taskContext: 'release process', observation: 'prepack guard rejects workspace protocol leaks', importanceScore: 6, label: 'RELEASE' });
  insertMemory(db, { taskContext: 'unrelated note', observation: 'completely different topic entirely', importanceScore: 3, label: 'OTHER' });
}

describe('weak-pool relevance squash', () => {
  // 20s: first test in the file pays the cold initDb + 4-insert Jaccard cost,
  // which can exceed the 5s default under full-suite CPU contention.
  it('a strong match scores high but never a perfect 1.0', { timeout: 20_000 }, () => {
    const db = freshDb();
    seedCorpus(db);
    const { memories } = getMemory(db, { query: 'tenant normalization order', limit: 3, explain: true });
    const top = memories[0]!;
    expect(top.observation).toContain('tenant id normalization');
    expect(top.lexical!).toBeGreaterThan(0.5);
    expect(top.lexical!).toBeLessThan(1.0);
  });

  it('degenerate bm25 pools (near-empty store) get neutral 0.5, not inflated 1.0', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 'auth router', observation: 'tenant order matters', importanceScore: 5 });
    const { memories } = getMemory(db, { query: 'auth router', limit: 1 });
    // Single-row corpus: IDF collapses, bm25 ≈ 0 — neutral relevance instead of a false 1.0 or 0.0.
    expect(memories[0]!.lexical).toBeCloseTo(0.5, 5);
  });
});

describe('judgment_required — low-confidence recall flag', () => {
  it('is absent on a confident match', () => {
    const db = freshDb();
    seedCorpus(db);
    const res = getMemory(db, { query: 'tenant normalization order', limit: 3 });
    expect(res.judgment_required).toBeUndefined();
  });

  it('fires with a reason when the query matches nothing', () => {
    const db = freshDb();
    seedCorpus(db);
    const res = getMemory(db, { query: 'kubernetes ingress annotation', limit: 3 });
    expect(res.count).toBe(0);
    expect(res.judgment_required).toBe(true);
    expect(res.judgment_reason).toContain('absence');
  });

  it('does not fire on empty-query browsing', () => {
    const db = freshDb();
    seedCorpus(db);
    const res = getMemory(db, { query: '', limit: 3 });
    expect(res.judgment_required).toBeUndefined();
  });
});

describe('per-label decay half-life defaults', () => {
  function halfLife(db: DatabaseSync, id: string): number | null {
    return (db.prepare('SELECT decay_half_life_days h FROM agent_memories WHERE memory_id = ?')
      .get(id) as { h: number | null }).h;
  }

  it('durable labels get 90d, EXPERIENCE 14d, others store default (NULL → 30d at read)', () => {
    const db = freshDb();
    const decision = insertMemory(db, { taskContext: 't', observation: 'a', importanceScore: 8, label: 'DECISION' }).memoryId;
    const gotcha = insertMemory(db, { taskContext: 't', observation: 'b', importanceScore: 8, label: 'GOTCHA' }).memoryId;
    const exp = insertMemory(db, { taskContext: 't', observation: 'c', importanceScore: 5, label: 'EXPERIENCE' }).memoryId;
    const bug = insertMemory(db, { taskContext: 't', observation: 'd', importanceScore: 5, label: 'BUG' }).memoryId;
    expect(halfLife(db, decision)).toBe(90);
    expect(halfLife(db, gotcha)).toBe(90);
    expect(halfLife(db, exp)).toBe(14);
    expect(halfLife(db, bug)).toBeNull();
  });
});

describe('salience floor on broad forget selectors', () => {
  it('a broad --before sweep skips high-importance memories and reports the floor', () => {
    const db = freshDb();
    const critical = insertMemory(db, { taskContext: 'critical', observation: 'data-loss rule', importanceScore: 9 }).memoryId;
    const minor = insertMemory(db, { taskContext: 'minor', observation: 'small note', importanceScore: 3 }).memoryId;
    const res = forgetMemory(db, { before: '2100-01-01T00:00:00Z' });
    expect(res.salience_floor).toBe(8);
    expect(res.memory_ids).toContain(minor);
    expect(res.memory_ids).not.toContain(critical);
    expect(db.prepare('SELECT 1 FROM agent_memories WHERE memory_id = ?').get(critical)).toBeTruthy();
  });

  it('explicit --max-importance overrides the floor; explicit ids bypass it', () => {
    const db = freshDb();
    const critical = insertMemory(db, { taskContext: 'critical', observation: 'rule', importanceScore: 9 }).memoryId;
    const swept = forgetMemory(db, { before: '2100-01-01T00:00:00Z', maxImportance: 10, dryRun: true });
    expect(swept.salience_floor).toBeUndefined();
    expect(swept.memory_ids).toContain(critical);
    const byId = forgetMemory(db, { memoryIds: [critical] });
    expect(byId.deleted).toBe(1);
    expect(byId.salience_floor).toBeUndefined();
  });
});
