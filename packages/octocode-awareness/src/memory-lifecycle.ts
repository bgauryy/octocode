import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { fillScope } from './git.js';
import { hasFts } from './db.js';
import type { ForgetMemoryParams, ForgetMemoryResult, MemoryLifecycleParams, ArchiveMemoryResult, RestoreMemoryResult } from './types.js';
import { SALIENCE_FLOOR } from './memory-scoring.js';

// ─── Reversible archive lifecycle ─────────────────────────────────────────────

export function lifecycleSelection(
  db: DatabaseSync,
  params: MemoryLifecycleParams,
  statePredicate: string,
): string[] {
  const memoryIds = [...new Set(params.memoryIds.map(String).filter(Boolean))];
  if (memoryIds.length === 0) throw new Error('memory lifecycle requires at least one memoryId');
  const scope = fillScope(
    {
      workspace_path: params.workspacePath ?? null,
      artifact: normalizeArtifact(params.artifact),
      repo: params.repo ?? null,
      ref: params.ref ?? null,
    },
    params.cwd ?? params.workspacePath ?? process.cwd(),
  );
  const conditions = [
    `memory_id IN (${memoryIds.map(() => '?').join(',')})`,
    statePredicate,
  ];
  const binds: (string | number)[] = [...memoryIds];
  if (params.workspacePath && scope.workspace_path) {
    conditions.push('workspace_path = ?');
    binds.push(scope.workspace_path);
  }
  if (params.artifact && scope.artifact) {
    conditions.push('artifact = ?');
    binds.push(scope.artifact);
  }
  if (params.repo && scope.repo) {
    conditions.push('repo = ?');
    binds.push(scope.repo);
  }
  if (params.ref && scope.ref) {
    conditions.push('ref = ?');
    binds.push(scope.ref);
  }
  const rows = db.prepare(
    `SELECT memory_id FROM memories WHERE ${conditions.join(' AND ')}`
  ).all(...binds) as Array<{ memory_id: string }>;
  const selected = new Set(rows.map(row => row.memory_id));
  return memoryIds.filter(memoryId => selected.has(memoryId));
}

/**
 * Reversibly archive active rows using existing lifecycle metadata. Archived rows
 * are SUPERSEDED with expired_at set and superseded_by left null, which keeps them
 * distinguishable from replacement history without a schema migration.
 */
export function archiveMemories(db: DatabaseSync, params: MemoryLifecycleParams): ArchiveMemoryResult {
  const ids = lifecycleSelection(db, params, "state = 'ACTIVE'");
  if (params.dryRun) {
    return { archived: 0, dry_run: true, would_archive: ids.length, memory_ids: ids };
  }
  if (ids.length > 0) {
    const now = utcNow();
    db.prepare(
      `UPDATE memories SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
       WHERE memory_id IN (${ids.map(() => '?').join(',')})`
    ).run(now, now, ...ids);
  }
  return { archived: ids.length, memory_ids: ids };
}

/** Restore only explicitly archived rows; superseded replacement history is immutable. */
export function restoreMemories(db: DatabaseSync, params: MemoryLifecycleParams): RestoreMemoryResult {
  const ids = lifecycleSelection(
    db,
    params,
    "state = 'SUPERSEDED' AND superseded_by IS NULL AND expired_at IS NOT NULL",
  );
  if (params.dryRun) {
    return { restored: 0, dry_run: true, would_restore: ids.length, memory_ids: ids };
  }
  if (ids.length > 0) {
    const now = utcNow();
    db.prepare(
      `UPDATE memories SET state = 'ACTIVE', expired_at = NULL, valid_to = NULL, updated_at = ?
       WHERE memory_id IN (${ids.map(() => '?').join(',')})`
    ).run(now, ...ids);
  }
  return { restored: ids.length, memory_ids: ids };
}

// ─── forgetMemory ─────────────────────────────────────────────────────────────

/**
 * Delete memories by id, tag, age, or importance ceiling.
 * dryRun=true returns the count without deleting anything.
 */
export function forgetMemory(db: DatabaseSync, params: ForgetMemoryParams): ForgetMemoryResult {
  const { memoryIds = [], tags = [], before, dryRun = false } = params;
  let { maxImportance } = params;
  const scope = fillScope(
    {
      workspace_path: params.workspacePath ?? null,
      artifact: normalizeArtifact(params.artifact),
      repo: params.repo ?? null,
      ref: params.ref ?? null,
    },
    params.cwd ?? params.workspacePath ?? process.cwd(),
  );

  // Two independent OR-combined selector groups so filters don't cross-contaminate:
  //   Group 1 — explicit IDs: selected directly, no importance or tag filter applied.
  //             (combining id + maxImportance as AND silently deleted nothing when the
  //             target memory had higher importance than the ceiling — docstring says OR.)
  //   Group 2 — attribute-based: tags/age/importance with salience-floor guard.
  // Optional scope flags are AND-combined with either selector group.
  const selectorGroups: string[] = [];
  const bindParams: (string | number)[] = [];
  let salienceFloorApplied = false;

  // Group 1: direct by id (unconditional)
  if (memoryIds.length > 0) {
    selectorGroups.push(`memory_id IN (${memoryIds.map(() => '?').join(',')})`);
    bindParams.push(...memoryIds);
  }

  // Group 2: attribute-based (tags + age + importance ceiling)
  const attrConds: string[] = [];
  const attrBinds: (string | number)[] = [];
  if (tags.length > 0) {
    // Use json_each subquery for tag filtering.
    attrConds.push(
      `(${tags.map(() => 'EXISTS (SELECT 1 FROM json_each(tags_json) WHERE value = ?)').join(' OR ')})`
    );
    attrBinds.push(...tags);
  }
  if (before) {
    attrConds.push('created_at < ?');
    attrBinds.push(before);
  }
  if (attrConds.length > 0 || maxImportance != null) {
    // Salience floor: broad attribute selectors never sweep high-importance memories
    // unless --max-importance explicitly raises the ceiling.
    if (maxImportance == null) {
      maxImportance = SALIENCE_FLOOR - 1;
      salienceFloorApplied = true;
    }
    attrConds.push('importance <= ?');
    attrBinds.push(maxImportance);
    selectorGroups.push(`(${attrConds.join(' AND ')})`);
    bindParams.push(...attrBinds);
  }

  if (selectorGroups.length === 0) {
    throw new Error('forgetMemory requires at least one filter: memoryIds, tags, before, or maxImportance');
  }

  const scopeConds: string[] = [];
  const scopeBinds: (string | number)[] = [];
  if (params.workspacePath && scope.workspace_path) {
    scopeConds.push('workspace_path = ?');
    scopeBinds.push(scope.workspace_path);
  }
  if (params.artifact && scope.artifact) {
    scopeConds.push('artifact = ?');
    scopeBinds.push(scope.artifact);
  }
  if (params.repo && scope.repo) {
    scopeConds.push('repo = ?');
    scopeBinds.push(scope.repo);
  }
  if (params.ref && scope.ref) {
    scopeConds.push('ref = ?');
    scopeBinds.push(scope.ref);
  }

  const selectorWhere = selectorGroups.join(' OR ');
  const where = scopeConds.length > 0
    ? `(${selectorWhere}) AND ${scopeConds.join(' AND ')}`
    : selectorWhere;
  const rows = db.prepare(
    `SELECT memory_id FROM memories WHERE ${where}`
  ).all(...bindParams, ...scopeBinds) as unknown as Array<{ memory_id: string }>;
  const ids = rows.map(r => r.memory_id);

  if (dryRun) {
    return {
      deleted: 0, dry_run: true, would_delete: ids.length, memory_ids: ids,
      ...(salienceFloorApplied ? { salience_floor: SALIENCE_FLOOR } : {}),
    };
  }

  // FIX #5 (P0): wrap all three DELETEs in a transaction so FTS and refs rows are
  // never left orphaned if one of the deletes fails mid-way.
  if (ids.length > 0) {
    const ph = ids.map(() => '?').join(',');
    db.exec('BEGIN IMMEDIATE');
    try {
      db.prepare(`DELETE FROM memories WHERE memory_id IN (${ph})`).run(...ids);
      if (hasFts(db)) {
        db.prepare(`DELETE FROM memories_fts WHERE memory_id IN (${ph})`).run(...ids);
      }
      try {
        db.prepare(`DELETE FROM memory_refs WHERE memory_id IN (${ph})`).run(...ids);
      } catch { /* ignore if table missing */ }
      db.exec('COMMIT');
    } catch (e) {
      try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
      throw e;
    }
  }

  return {
    deleted: ids.length, memory_ids: ids,
    ...(salienceFloorApplied ? { salience_floor: SALIENCE_FLOOR } : {}),
  };
}
