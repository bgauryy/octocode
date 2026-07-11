import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { insertMemory, archiveMemories, restoreMemories } from '../src/memory.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}

describe('memory archive lifecycle', () => {
  it('archives active rows, restores only archived rows, and supports dry-run', () => {
    const db = freshDb();
    const { memoryId } = insertMemory(db, {
      taskContext: 'archive unit', observation: 'restore the archived row', importance: 6,
    });

    expect(archiveMemories(db, { memoryIds: [memoryId], dryRun: true }))
      .toEqual({ archived: 0, dry_run: true, would_archive: 1, memory_ids: [memoryId] });
    expect(archiveMemories(db, { memoryIds: [memoryId] }))
      .toEqual({ archived: 1, memory_ids: [memoryId] });
    expect(db.prepare('SELECT state, expired_at FROM memories WHERE memory_id = ?').get(memoryId))
      .toMatchObject({ state: 'SUPERSEDED', expired_at: expect.any(String) });

    expect(restoreMemories(db, { memoryIds: [memoryId], dryRun: true }))
      .toEqual({ restored: 0, dry_run: true, would_restore: 1, memory_ids: [memoryId] });
    expect(restoreMemories(db, { memoryIds: [memoryId] }))
      .toEqual({ restored: 1, memory_ids: [memoryId] });
    expect(db.prepare('SELECT state, expired_at FROM memories WHERE memory_id = ?').get(memoryId))
      .toEqual({ state: 'ACTIVE', expired_at: null });
  });
});

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

  it('validates and canonicalizes temporal bounds', () => {
    const db = freshDb();
    expect(() => insertMemory(db, {
      taskContext: 't', observation: 'o', importance: 5, validFrom: 'not-a-date',
    })).toThrow(/valid_from.*ISO/i);
    expect(() => insertMemory(db, {
      taskContext: 't', observation: 'o', importance: 5, validFrom: 'January 1, 2026',
    })).toThrow(/valid_from.*ISO/i);
    expect(() => insertMemory(db, {
      taskContext: 't', observation: 'o', importance: 5,
      validFrom: '2026-01-02T00:00:00Z', validTo: '2026-01-01T00:00:00Z',
    })).toThrow(/valid_to.*after valid_from/i);

    const { memoryId } = insertMemory(db, {
      taskContext: 't', observation: 'o', importance: 5,
      validFrom: '2026-01-01T02:00:00+02:00', validTo: '2026-01-02T02:00:00+02:00',
    });
    expect(db.prepare('SELECT valid_from, valid_to FROM memories WHERE memory_id = ?').get(memoryId))
      .toEqual({ valid_from: '2026-01-01T00:00:00Z', valid_to: '2026-01-02T00:00:00Z' });
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

  it('hard-errors unknown labels by default', () => {
    const db = freshDb();
    expect(() => insertMemory(db, {
      taskContext: 't', observation: 'o', importance: 3, label: 'BOGUS',
    })).toThrow(/invalid label/);
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
