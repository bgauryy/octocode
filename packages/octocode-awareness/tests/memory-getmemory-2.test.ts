import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb, rebuildFts, replaceMemoryReferences } from '../src/db.js';
import { insertMemory, getMemory, bumpAccess, decayScore } from '../src/memory.js';
import type { MemoryRecord } from '../src/types.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}

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

  it('uses a deterministic memory-id tie break for equal sort values', () => {
    const db = freshDb();
    const first = insertMemory(db, { taskContext: 'stable order', observation: 'same', importance: 5 });
    const second = insertMemory(db, { taskContext: 'stable order', observation: 'same', importance: 5 });
    db.prepare("UPDATE memories SET created_at = '2026-01-01T00:00:00Z', last_accessed_at = '2026-01-01T00:00:00Z'").run();

    const ids = getMemory(db, { query: 'stable order', sort: 'recent', limit: 10, recordAccess: false })
      .memories.map(memory => memory.memory_id);
    expect(ids).toEqual([first.memoryId, second.memoryId].sort());
  });

  it('explains the exact search, scope, filter, and ranking contract on request', () => {
    const db = freshDb();
    const result = getMemory(db, {
      query: 'parser failure', workspacePath: '/tmp/project', label: ['GOTCHA'],
      tags: ['parser'], files: ['src/parser.ts'], sort: 'importance', smart: true,
      strictScope: true, minImportance: 6, limit: 4, explain: true, recordAccess: false,
    });

    expect(result.applied_filters).toMatchObject({
      query: 'parser failure', limit: 4, min_importance: 1, labels: [],
      tags: [], files: ['src/parser.ts'], workspace_path: '/tmp/project',
      strict_scope: true, global_only: false, states: ['ACTIVE'], sort: 'importance',
      smart: true,
    });
  });

  // 30s timeout: 10 inserts each run similar-memory Jaccard scans; under the
  // full parallel coverage suite this occasionally exceeds the default 5s on CPU contention.
  it('respects limit', { timeout: 30_000 }, () => {
    const db = freshDb();
    for (let i = 0; i < 10; i++) {
      insertMemory(db, { taskContext: 'ctx', observation: `obs ${i}`, importance: 5 });
    }
    const { memories, count } = getMemory(db, { limit: 3 });
    expect(memories).toHaveLength(3);
    expect(count).toBe(3);
  });

  it('can defer access tracking until an alternate ranker chooses final results', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 'alternate ranker', observation: 'candidate result', importance: 5,
    });
    const before = db.prepare('SELECT access_count FROM memories WHERE memory_id = ?')
      .get(memoryId) as { access_count: number };

    const result = getMemory(db, {
      query: '', candidateMemoryIds: [memoryId], recordAccess: false,
    });

    expect(result.memories.map(memory => memory.memory_id)).toEqual([memoryId]);
    expect(db.prepare('SELECT access_count FROM memories WHERE memory_id = ?').get(memoryId))
      .toEqual({ access_count: before.access_count });
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

  it('does not turn a stopword-only query into unrelated browsing', () => {
    const db = freshDb();
    insertMemory(db, { taskContext: 'unrelated', observation: 'durable lesson', importance: 9 });
    const result = getMemory(db, { query: 'the and with', limit: 5 });
    expect(result.memories).toEqual([]);
    expect(result.judgment_required).toBe(true);
  });

  it('excludes currently expired ACTIVE memories from normal recall', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'expiredcurrent temporal', observation: 'must not be recalled now', importance: 9,
      validFrom: '2019-01-01T00:00:00Z', validTo: '2020-01-01T00:00:00Z',
    });
    expect(getMemory(db, { query: 'expiredcurrent temporal', limit: 5 }).memories).toEqual([]);
  });

  it('historical recall includes memories valid then even if later superseded', () => {
    const db = freshDb();
    const old = insertMemory(db, {
      taskContext: 'historicalstate temporal', observation: 'old truth', importance: 7,
      validFrom: '2019-01-01T00:00:00Z',
    });
    insertMemory(db, {
      taskContext: 'historicalstate temporal', observation: 'new truth', importance: 8,
      validFrom: '2021-01-01T00:00:00Z', supersedes: [old.memoryId],
    });

    const historical = getMemory(db, {
      query: 'historicalstate temporal', asOf: '2020-06-01T00:00:00Z', limit: 5,
    });
    expect(historical.memories.map(memory => memory.memory_id)).toContain(old.memoryId);

    const explicitlyActive = getMemory(db, {
      query: 'historicalstate temporal', asOf: '2020-06-01T00:00:00Z', states: ['ACTIVE'], limit: 5,
    });
    expect(explicitlyActive.memories.map(memory => memory.memory_id)).not.toContain(old.memoryId);
  });

  it('rejects non-ISO historical timestamps instead of using implementation-defined parsing', () => {
    const db = freshDb();
    expect(() => getMemory(db, { asOf: 'June 1, 2020' })).toThrow(/as_of.*ISO/i);
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
