/**
 * memory.ts — Core memory store operations.
 *
 * insertMemory: pure DB insert, returns { memoryId, memory, superseded }.
 * getMemory:    FTS5 + decay-scored recall.
 * bumpAccess:   update access count and timestamp.
 */
import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow, normalizeTags, normalizeReferences, normalizeLabel } from './helpers.js';
import { fillScope } from './git.js';
import { hasFts, ftsTermsForRow, replaceMemoryReferences } from './db.js';
import type { InsertMemoryParams, InsertMemoryResult } from './types.js';
import { canonicalMemoryInstant, findSimilarMemories, LABEL_HALF_LIFE_DAYS } from './memory-scoring.js';

// ─── bumpAccess ───────────────────────────────────────────────────────────────

export function bumpAccess(db: DatabaseSync, memoryIds: string[]): void {
  if (memoryIds.length === 0) return;
  const now = utcNow();
  const placeholders = memoryIds.map(() => '?').join(',');
  db.prepare(`
    UPDATE memories
    SET access_count = COALESCE(access_count, 0) + 1, last_accessed_at = ?
    WHERE memory_id IN (${placeholders})
  `).run(now, ...memoryIds);
}

// ─── insertMemory ─────────────────────────────────────────────────────────────

/**
 * Insert a new memory record.
 * Returns { memoryId, memory, superseded } — does NOT emit JSON.
 */
export function insertMemory(db: DatabaseSync, params: InsertMemoryParams): InsertMemoryResult {
  const {
    agentId = 'agent',
    taskContext,
    observation,
    importance,
    label,
    tags = [],
    tagsCsv = '',
    references = [],
    supersedes = [],
    failureSignature = null,
    validFrom: vf,
    validTo: vt,
    workspacePath,
    artifact,
    repo: repoArg,
    ref: refArg,
    fileTreeFingerprint = null,
    cwd,
  } = params;

  const imp = Number(importance);
  if (!Number.isInteger(imp) || imp < 1 || imp > 10) {
    throw new Error(`importance must be 1–10, got ${String(importance)}`);
  }

  const normalizedValidFrom = canonicalMemoryInstant(vf, 'valid_from');
  const normalizedValidTo = canonicalMemoryInstant(vt, 'valid_to');
  const memoryId = 'mem_' + randomUUID().replace(/-/g, '');
  const tagList = normalizeTags(tags, tagsCsv);
  const refList = normalizeReferences(references);
  const normalizedLabel = normalizeLabel(Array.isArray(label) ? label[0] : label);
  const createdAt = utcNow();
  const validFromVal = normalizedValidFrom ?? createdAt;
  if (normalizedValidTo != null && normalizedValidTo <= validFromVal) {
    throw new Error('valid_to must be after valid_from');
  }

  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd()
  );
  const supersedeIds = [...new Set(supersedes.filter(Boolean))];

  const halfLifeDefault = LABEL_HALF_LIFE_DAYS[normalizedLabel] ?? null;

  // Variables assigned inside the transaction (declared outside for return scope).
  let noveltyScore = 0;
  let similarMemoryIds: string[] = [];
  const superseded: string[] = [];

  const ownsTransaction = !db.isTransaction;
  if (ownsTransaction) db.exec('BEGIN IMMEDIATE');
  try {
    if (supersedeIds.length > 0) {
      const placeholders = supersedeIds.map(() => '?').join(',');
      const rows = db.prepare(`
        SELECT memory_id, agent_id, state, workspace_path, artifact, repo, ref
        FROM memories WHERE memory_id IN (${placeholders})
      `).all(...supersedeIds) as unknown as Array<Record<string, string | null>>;
      const byId = new Map(rows.map(row => [String(row['memory_id']), row]));
      for (const oldId of supersedeIds) {
        const row = byId.get(oldId);
        if (!row) throw new Error(`supersedes target not found: ${oldId}`);
        if (row['agent_id'] !== agentId) throw new Error(`supersedes target has a different owner: ${oldId}`);
        if (row['state'] !== 'ACTIVE') throw new Error(`supersedes target is not ACTIVE: ${oldId}`);
        for (const field of ['workspace_path', 'artifact', 'repo', 'ref'] as const) {
          if ((row[field] ?? null) !== (scope[field] ?? null)) {
            throw new Error(`supersedes target has a different scope: ${oldId}`);
          }
        }
      }
    }
    // FIX #8 (P1): findSimilarMemories moved inside the transaction for read consistency —
    // ensures the similarity check and the insert see the same set of ACTIVE memories.
    // TOOL-2: Use preComputedSimilar if provided (avoids double findSimilarMemories call
    // when the caller already ran a dedup gate check before deciding to insert).
    const similar = params.preComputedSimilar ?? findSimilarMemories(db, `${taskContext} ${observation}`, 3, null, {
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      cwd,
    });
    noveltyScore = Math.max(0, Math.min(1, 1 - (similar[0]?.similarity ?? 0)));
    similarMemoryIds = similar.map(m => m.memory_id);

    db.prepare(`
      INSERT INTO memories (
        memory_id, agent_id, task_context, observation, importance,
        label, tags_json, workspace_path, artifact, repo, ref,
        file_tree_fingerprint, novelty_score, created_at, updated_at,
        last_accessed_at, access_count, failure_signature, valid_from, valid_to, decay_half_life_days
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?)
    `).run(
      memoryId, agentId, taskContext, observation, imp,
      normalizedLabel, JSON.stringify(tagList),
      scope.workspace_path, scope.artifact, scope.repo, scope.ref,
      fileTreeFingerprint, noveltyScore, createdAt, createdAt,
      createdAt, failureSignature ?? null, validFromVal, normalizedValidTo, halfLifeDefault
    );

    // Populate structured reference index (memory_refs table)
    if (refList.length > 0) {
      try {
        replaceMemoryReferences(db, memoryId, refList);
      } catch (e) {
        if (!(e instanceof Error && e.message.includes('no such table'))) throw e;
      }
    }

    if (hasFts(db)) {
      db.prepare(
        'INSERT INTO memories_fts(memory_id, task_context, observation, tags) VALUES (?, ?, ?, ?)'
      ).run(
        memoryId, taskContext, observation,
        ftsTermsForRow({
          tags_json: JSON.stringify(tagList),
          label: normalizedLabel,
          references: refList,
        })
      );
    }
    // FIX #1 (P0): supersede UPDATE loop moved INSIDE BEGIN IMMEDIATE (before COMMIT)
    // so the supersede and insert are atomic — no window where the old memory is still
    // ACTIVE after the new one is visible to concurrent readers.
    for (const oldId of supersedeIds) {
      const r = db.prepare(`
        UPDATE memories
        SET state = 'SUPERSEDED', superseded_by = ?, updated_at = ?,
            valid_to = COALESCE(valid_to, ?), expired_at = ?
        WHERE memory_id = ? AND state = 'ACTIVE'
      `).run(memoryId, createdAt, validFromVal, createdAt, oldId) as { changes: number };
      if (r.changes !== 1) throw new Error(`supersedes target changed concurrently: ${oldId}`);
      superseded.push(oldId);
    }

    if (ownsTransaction) db.exec('COMMIT');
  } catch (e) {
    if (ownsTransaction) {
      try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
    }
    throw e;
  }

  return {
    memoryId,
    memory: {
      memory_id: memoryId,
      agent_id: agentId,
      task_context: taskContext,
      observation,
      importance: imp,
      label: normalizedLabel,
      tags: tagList,
      references: refList,
      workspace_path: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      failure_signature: failureSignature ?? null,
      novelty_score: noveltyScore,
      state: 'ACTIVE' as const,
      created_at: createdAt,
    },
    superseded,
    noveltyScore,
    similarMemoryIds,
  };
}

export type GuardedMemoryInsertResult =
  | { skipped: true; similar: Array<{ memory_id: string; similarity: number }> }
  | { skipped: false; similar: Array<{ memory_id: string; similarity: number }>; result: InsertMemoryResult };

/**
 * Atomically run the duplicate gate and insert. Tool adapters must use this
 * instead of precomputing similarity before insertMemory's write transaction.
 */
export function insertMemoryWithSimilarityGate(
  db: DatabaseSync,
  params: InsertMemoryParams,
  allowSimilar = false,
): GuardedMemoryInsertResult {
  const importance = Number(params.importance);
  if (!Number.isInteger(importance) || importance < 1 || importance > 10) {
    throw new Error(`importance must be 1–10, got ${String(params.importance)}`);
  }
  // Validate fields that insertMemory owns before an early duplicate return;
  // invalid input must never appear successful merely because it resembles an
  // existing row.
  normalizeLabel(Array.isArray(params.label) ? params.label[0] : params.label);
  const ownsTransaction = !db.isTransaction;
  if (ownsTransaction) db.exec('BEGIN IMMEDIATE');
  try {
    const scope = fillScope(
      {
        workspace_path: params.workspacePath ?? null,
        artifact: normalizeArtifact(params.artifact),
        repo: params.repo ?? null,
        ref: params.ref ?? null,
      },
      params.cwd ?? process.cwd(),
    );
    const supersedes = params.supersedes ?? [];
    const similar = findSimilarMemories(
      db,
      `${params.taskContext} ${params.observation}`,
      5,
      null,
      {
        workspacePath: scope.workspace_path,
        artifact: scope.artifact,
        repo: scope.repo,
        ref: scope.ref,
        cwd: params.cwd,
      },
    );
    const unsupersededSimilar = similar.filter(memory => !supersedes.includes(memory.memory_id));
    if (unsupersededSimilar.length > 0 && !allowSimilar) {
      if (ownsTransaction) db.exec('COMMIT');
      return { skipped: true, similar: unsupersededSimilar };
    }

    const result = insertMemory(db, { ...params, preComputedSimilar: similar });
    if (ownsTransaction) db.exec('COMMIT');
    return { skipped: false, similar, result };
  } catch (error) {
    if (ownsTransaction) {
      try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
    }
    throw error;
  }
}
