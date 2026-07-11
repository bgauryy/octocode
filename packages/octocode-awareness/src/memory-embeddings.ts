import type { DatabaseSync } from 'node:sqlite';
import { utcNow, rowToMemory } from './helpers.js';
import type { MemoryRow, MemoryRecord } from './types.js';
import { attachMemoryReferences } from './memory-search.js';

// ─── Embedding storage + cosine search (ARCH-6) ─────────────────────────────

/**
 * Compute cosine similarity between two Float32 vectors.
 * Returns 0 if either vector has zero magnitude.
 */
export function cosineSimilarity(a: Float32Array, b: Float32Array): number {
  if (a.length !== b.length || a.length === 0) return 0;
  let dot = 0, normA = 0, normB = 0;
  for (let i = 0; i < a.length; i++) {
    dot  += a[i]! * b[i]!;
    normA += a[i]! * a[i]!;
    normB += b[i]! * b[i]!;
  }
  const denom = Math.sqrt(normA) * Math.sqrt(normB);
  return denom === 0 ? 0 : dot / denom;
}

/**
 * Store a dense embedding for a memory.
 * Uses the existing `embedding` BLOB + `embedding_model` TEXT columns
 * (already in the schema; previously unused).
 *
 * The embedding source (API, local model) is the caller's responsibility —
 * this function only handles persistence.
 *
 * @param embedding - Flat Float32Array from a text-embedding model
 * @param model     - Model identifier, e.g. 'text-embedding-3-small'
 */
export function storeEmbedding(
  db: DatabaseSync,
  memoryId: string,
  embedding: Float32Array,
  model: string,
): void {
  // Serialize Float32Array → raw binary buffer stored as BLOB
  const blob = Buffer.from(embedding.buffer, embedding.byteOffset, embedding.byteLength);
  db.prepare(
    `UPDATE memories SET embedding = ?, embedding_model = ?, updated_at = ?
     WHERE memory_id = ?`
  ).run(blob, model, utcNow(), memoryId);
}

/**
 * Search memories by cosine similarity against a query embedding.
 *
 * Retrieves stored embeddings (optionally filtered by model) and ranks them in
 * JS. To bound heap use it only loads the 2000 most-recently-accessed embedded
 * memories (see the `LIMIT 2000` below), so at larger scale older embedded
 * memories fall outside the cosine ranking and a proper vector index
 * (e.g. sqlite-vss) would be needed.
 *
 * @param queryEmbedding - The embedding of the text to search for
 * @param limit          - Maximum results to return (default 5)
 * @param threshold      - Minimum cosine similarity 0–1 (default 0.75)
 * @param model          - Only compare against embeddings from this model
 * @param states         - Memory states eligible for alternate ranking
 */
export function searchByEmbedding(
  db: DatabaseSync,
  queryEmbedding: Float32Array,
  limit = 5,
  threshold = 0.75,
  model?: string,
  states: string[] = ['ACTIVE'],
): Array<{ memory_id: string; similarity: number }> {
  const normalizedStates = [...new Set(states.filter(Boolean))];
  if (normalizedStates.length === 0) return [];
  const conditions = [
    `state IN (${normalizedStates.map(() => '?').join(',')})`,
    'embedding IS NOT NULL',
  ];
  const binds: string[] = [];
  binds.push(...normalizedStates);
  if (model) { conditions.push('embedding_model = ?'); binds.push(model); }

  type EmbRow = { memory_id: string; embedding: Buffer; embedding_model: string };
  // Limit to avoid loading unbounded embedding blobs into JS heap; cosine-rank within the cap.
  const rows = db.prepare(
    `SELECT memory_id, embedding, embedding_model FROM memories
     WHERE ${conditions.join(' AND ')}
     ORDER BY COALESCE(last_accessed_at, created_at) DESC
     LIMIT 2000`
  ).all(...binds) as unknown as EmbRow[];

  const results: Array<{ memory_id: string; similarity: number }> = [];
  for (const row of rows) {
    try {
      const stored = new Float32Array(
        row.embedding.buffer,
        row.embedding.byteOffset,
        row.embedding.byteLength / 4,
      );
      const sim = cosineSimilarity(queryEmbedding, stored);
      if (sim >= threshold) results.push({ memory_id: row.memory_id, similarity: sim });
    } catch { /* corrupted BLOB — skip */ }
  }

  return results
    .sort((a, b) => b.similarity - a.similarity)
    .slice(0, limit);
}

/**
 * Load ACTIVE memory rows by id, preserving the caller order when possible.
 */
export function loadMemoriesByIds(
  db: DatabaseSync,
  memoryIds: string[],
): MemoryRecord[] {
  const ids = [...new Set(memoryIds.filter(Boolean))];
  if (ids.length === 0) return [];
  const placeholders = ids.map(() => '?').join(', ');
  const rows = db.prepare(
    `SELECT * FROM memories WHERE memory_id IN (${placeholders}) AND state = 'ACTIVE'`
  ).all(...ids) as unknown as MemoryRow[];
  const byId = new Map(rows.map(row => [row.memory_id, rowToMemory(row)]));
  attachMemoryReferences(db, [...byId.values()]);
  return ids.map(id => byId.get(id)).filter((row): row is MemoryRecord => Boolean(row));
}
