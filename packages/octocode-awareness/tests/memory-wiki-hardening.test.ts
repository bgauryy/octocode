import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db-init.js';
import { projectMemoryLean } from '../src/helpers.js';
import { getMemory } from '../src/memory-recall.js';
import { insertMemory } from '../src/memory-write.js';
import { decayComponents } from '../src/memory-scoring.js';
import type { MemoryRecord } from '../src/types/identity-memory.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('memory trust with lean retrieval', () => {
  it('keeps explicit smart filters until the requested result set under-fills', () => {
    const db = freshDb();
    insertMemory(db, {
      taskContext: 'cache invalidation rule', observation: 'high confidence rule', importance: 5,
    });
    insertMemory(db, {
      taskContext: 'cache invalidation rule', observation: 'lower confidence rule', importance: 4,
    });

    const result = getMemory(db, {
      query: 'cache invalidation rule', minImportance: 5, limit: 1, smart: true, explain: true,
      recordAccess: false,
    });

    expect(result.count).toBe(1);
    expect(result.smart_expanded).toBeUndefined();
    expect(result.applied_filters?.min_importance).toBe(5);
  });

  it('does not let recall activity refresh evidence age', () => {
    const now = new Date().toISOString();
    const base = {
      memory_id: 'm', agent_id: 'a', task_context: 't', observation: 'o', importance: 5,
      state: 'ACTIVE' as const, label: 'OTHER', superseded_by: null, tags: [], references: [],
      workspace_path: null, artifact: null, repo: null, ref: null, novelty_score: null,
      failure_signature: null, access_count: 0, decay_half_life_days: null,
      valid_from: null, valid_to: null, expired_at: null, file_tree_fingerprint: null,
      updated_at: null,
    };
    const staleButRead = { ...base, created_at: '2020-01-01T00:00:00Z', last_accessed_at: now } as MemoryRecord;
    const newerEvidence = { ...base, memory_id: 'new', created_at: '2026-01-01T00:00:00Z', last_accessed_at: null } as MemoryRecord;

    expect(decayComponents(newerEvidence, 1).recency)
      .toBeGreaterThan(decayComponents(staleButRead, 1).recency);
  });

  it('rejects missing, foreign-owner, cross-scope, and inactive supersession targets atomically', () => {
    const db = freshDb();
    const old = insertMemory(db, {
      agentId: 'owner', taskContext: 'old rule', observation: 'old evidence', importance: 5,
      workspacePath: '/workspace/a',
    });

    expect(() => insertMemory(db, {
      agentId: 'intruder', taskContext: 'replacement', observation: 'foreign owner', importance: 5,
      workspacePath: '/workspace/a', supersedes: [old.memoryId],
    })).toThrow(/owner/i);
    expect(() => insertMemory(db, {
      agentId: 'owner', taskContext: 'replacement', observation: 'cross scope', importance: 5,
      workspacePath: '/workspace/b', supersedes: [old.memoryId],
    })).toThrow(/scope/i);
    expect(() => insertMemory(db, {
      agentId: 'owner', taskContext: 'replacement', observation: 'missing target', importance: 5,
      workspacePath: '/workspace/a', supersedes: ['mem_missing'],
    })).toThrow(/not found/i);

    const row = db.prepare('SELECT state, superseded_by FROM awareness_memories WHERE memory_id = ?').get(old.memoryId) as Record<string, unknown>;
    expect(row).toEqual({ state: 'ACTIVE', superseded_by: null });
    expect((db.prepare('SELECT COUNT(*) AS count FROM awareness_memories').get() as { count: number }).count).toBe(1);
  });

  it('caps list fields and omits absent optional fields from lean memory rows', () => {
    const memory = {
      memory_id: 'mem_1', label: 'GOTCHA', importance: 8,
      task_context: 'context', observation: 'observation',
      tags: ['a', 'b', 'c', 'd'], references: ['r1', 'r2', 'r3', 'r4'],
      score: undefined, failure_signature: null,
    } as unknown as MemoryRecord;
    const lean = projectMemoryLean(memory);
    expect(lean).toMatchObject({
      tags: ['a', 'b', 'c'], tag_count: 4, tag_omitted_count: 1,
      references: ['r1', 'r2', 'r3'], reference_count: 4, reference_omitted_count: 1,
    });
    expect(lean).not.toHaveProperty('score');
    expect(lean).not.toHaveProperty('failure_signature');
    expect(lean).not.toHaveProperty('created_at');
  });
});
