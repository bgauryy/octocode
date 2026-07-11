import { parseJsonList, utcNow } from './helpers.js';
import type { MemoryRow } from './types.js';
import { DatabaseSync } from './db-runtime.js';

// ─── FTS helpers ──────────────────────────────────────────────────────────────

export function hasFts(db: DatabaseSync): boolean {
  const row = db.prepare(
    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='memories_fts'"
  ).get() as Record<string, number> | undefined;
  return Boolean(row);
}

export type FtsTermRow = Partial<MemoryRow> & { references?: string[] };

/**
 * Build the FTS5 `tags` column value for a memory row.
 *
 * Index semantic classifiers plus explicit provenance references. Workspace,
 * repo, git ref, and failure_signature remain structural filters so broad path
 * or repo names do not dominate natural-language ranking.
 */
export function ftsTermsForRow(row: FtsTermRow): string {
  const tags = parseJsonList(row.tags_json);
  const label = (row.label ?? 'OTHER').toLowerCase();
  return [...tags, label, ...(row.references ?? [])].filter(Boolean).join(' ');
}

export function rebuildFts(db: DatabaseSync): void {
  // DB-4 reverted: 'delete-all' FTS5 command only works on content= (contentless)
  // tables, not regular FTS5 tables. DELETE FROM is the correct approach for
  // a standard fts5 table (it goes through the shadow tables properly).
  db.exec('SAVEPOINT rebuild_fts');
  try {
    db.exec('DELETE FROM memories_fts');
    // Select only the columns needed for FTS indexing — avoids loading the
    // embedding BLOB (can be 1536 floats = 6KB per row) for all rows.
    const rows = db.prepare(
      'SELECT memory_id, task_context, observation, tags_json, label FROM memories'
    ).all() as unknown as Array<Pick<MemoryRow, 'memory_id' | 'task_context' | 'observation' | 'tags_json' | 'label'> & { references?: string[] }>;
    if (rows.length > 0) {
      const refs = db.prepare(
        `SELECT r.memory_id, r.reference
         FROM memory_refs r
         JOIN memories m ON m.memory_id = r.memory_id
         ORDER BY r.memory_id, r.ordinal`
      ).all() as unknown as Array<{ memory_id: string; reference: string }>;
      const refsByMemory = new Map<string, string[]>();
      for (const ref of refs) {
        const list = refsByMemory.get(ref.memory_id) ?? [];
        list.push(ref.reference);
        refsByMemory.set(ref.memory_id, list);
      }
      for (const row of rows) row.references = refsByMemory.get(row.memory_id) ?? [];
    }
    const insert = db.prepare(
      'INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
    );
    for (const row of rows) {
      insert.run(row.memory_id, row.task_context, row.observation, ftsTermsForRow(row));
    }
    db.exec('RELEASE SAVEPOINT rebuild_fts');
  } catch (e) {
    try { db.exec('ROLLBACK TO SAVEPOINT rebuild_fts'); } catch { /* already rolled back */ }
    try { db.exec('RELEASE SAVEPOINT rebuild_fts'); } catch { /* already released */ }
    throw e;
  }
}

// ─── Memory references ────────────────────────────────────────────────────────

export function referenceKind(reference: string): string {
  if (/^https?:\/\//.test(reference)) return 'url';
  const m = reference.match(/^([a-zA-Z][a-zA-Z0-9_.\-]*):/);
  return m ? m[1]!.toLowerCase() : 'other';
}

export function replaceMemoryReferences(db: DatabaseSync, memoryId: string, references: string[]): void {
  db.prepare('DELETE FROM memory_refs WHERE memory_id = ?').run(memoryId);
  const insert = db.prepare(
    'INSERT OR REPLACE INTO memory_refs(memory_id, reference, kind, ordinal) VALUES (?, ?, ?, ?)'
  );
  references.forEach((ref, i) => insert.run(memoryId, ref, referenceKind(ref), i));
}

// ─── Lock maintenance ─────────────────────────────────────────────────────────

/**
 * Evict expired exclusive locks without changing run lifecycle. Advisory
 * presence is independent: WORK/HOOK ends explicitly and TASK ends through
 * task submit/release.
 */
export interface EvictExpiredLocksResult {
  pruned_locks: number;
}

export function evictExpiredLocks(db: DatabaseSync): EvictExpiredLocksResult {
  const now = utcNow();
  const stale = db.prepare(
    'SELECT COUNT(*) AS c FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?'
  ).get(now) as { c: number };
  if (stale.c === 0) return { pruned_locks: 0 };

  db.exec('SAVEPOINT evict_expired_locks');
  try {
    const deleteRes = db.prepare(
      'DELETE FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?'
    ).run(now) as { changes: number };
    db.exec('RELEASE SAVEPOINT evict_expired_locks');
    return { pruned_locks: deleteRes.changes };
  } catch (e) {
    try { db.exec('ROLLBACK TO SAVEPOINT evict_expired_locks'); } catch { /* already rolled back */ }
    try { db.exec('RELEASE SAVEPOINT evict_expired_locks'); } catch { /* already released */ }
    throw e;
  }
}
