import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db-init.js';
import { insertMemory } from '../src/memory-write.js';
import { storeEmbedding } from '../src/memory-embeddings.js';
import { recallMemory } from '../src/memory-semantic.js';

vi.mock('../src/embed.js', async (importOriginal) => ({
  ...await importOriginal<Record<string, unknown>>(),
  resolveEmbedCommand: () => 'fixture-embedder',
  runHostEmbedder: () => ({ embedding: new Float32Array([1, 0]), model: 'fixture' }),
}));

describe('semantic recall access accounting', () => {
  let db: DatabaseSync;
  let best: string;
  let other: string;
  beforeEach(async () => {
    db = new DatabaseSync(':memory:');
    initDb(db);
    const record = async (observation: string, vector: number[]) => {
      const id = (await insertMemory(db, {
        agentId: 'reader', taskContext: 'accounting', observation, importance: 5,
      })).memoryId;
      storeEmbedding(db, id, new Float32Array(vector), 'fixture');
      return id;
    };
    best = (await record('best memory', [1, 0]));
    other = (await record('other memory', [0.5, 0.5]));
    db.exec('UPDATE awareness_memories SET last_accessed_at = NULL');
  });
  afterEach(() => { db.close(); });
  const access = (id: string) => db.prepare(
    'SELECT access_count, last_accessed_at FROM awareness_memories WHERE memory_id = ?',
  ).get(id);

  it('credits only the top-k memories returned to the caller', async () => {
    const result = (await recallMemory(db, { query: 'accounting', limit: 1 }, true));
    expect(result['mode']).toBe('semantic');
    expect(result['memories']).toMatchObject([{ memory_id: best }]);
    expect(result).toMatchObject({ partial: true, partialReasons: ['result_limit'], terminalLimit: { code: 'MEMORY_RECALL_LIMIT' } });
    expect(access(best)).toMatchObject({ access_count: 1 });
    expect(access(other)).toMatchObject({ access_count: 0, last_accessed_at: null });
  });

  it('discloses a saturated embedding pool even when top-k is satisfied', async () => {
    for (let i = 0; i < 1999; i++) {
      const id = (await insertMemory(db, { taskContext: 'pool', observation: `candidate ${i}`, importance: 5 })).memoryId;
      storeEmbedding(db, id, new Float32Array([1, 0]), 'fixture');
    }
    const result = (await recallMemory(db, { query: 'accounting', limit: 1, recordAccess: false }, true));
    expect(result['partialReasons']).toContain('candidate_limit');
    expect(result['terminalLimit']).toMatchObject({ code: 'MEMORY_RECALL_LIMIT', candidateLimit: 2000 });
  });

  it.each(['accounting', ''])('honors recordAccess:false for query %j', async (query) => {
    (await recallMemory(db, { query, limit: 1, recordAccess: false }, true));
    expect(access(best)).toMatchObject({ access_count: 0, last_accessed_at: null });
    expect(access(other)).toMatchObject({ access_count: 0, last_accessed_at: null });
  });
});
