/** Advisory file-presence and optional exclusive-lock operations. */

import { randomUUID } from 'node:crypto';
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { canonicalizePath, normalizeWorkspacePath } from './git.js';
import { ensureRunSession } from './sessions.js';
import type {
  EndWorkParams,
  ListWorkParams,
  ListWorkResult,
  StartWorkParams,
  StartWorkResult,
  TouchWorkParams,
  WorkConflict,
  WorkFileRecord,
  WorkMutationResult,
  WorkPeer,
  WorkPresence,
  WorkRunRecord,
} from './types.js';

const DEFAULT_PRESENCE_TTL_MS = 10 * 60_000;
const MAX_PRESENCE_TTL_MS = 60 * 60_000;
const PEER_DETAIL_LIMIT = 5;

function required(value: string | null | undefined, name: string): string {
  const normalized = value?.trim() ?? '';
  if (!normalized) throw new Error(`${name} is required`);
  return normalized;
}

function workspaceRoot(workspacePath?: string | null): string {
  const candidate = workspacePath ?? process.cwd();
  return normalizeWorkspacePath(candidate, candidate) ?? resolve(candidate);
}

export function normalizeFiles(files: string[], workspacePath?: string | null): string[] {
  if (files.length === 0) throw new Error('at least one target file is required');
  const base = canonicalizePath(workspacePath ? resolve(workspacePath) : process.cwd());
  return [...new Set(files.map((file) => {
    const value = required(file, 'target file');
    return canonicalizePath(isAbsolute(value) ? resolve(value) : resolve(base, value));
  }))];
}

function expiry(ttlMs?: number | null): string {
  const effective = Math.min(Math.max(1, ttlMs ?? DEFAULT_PRESENCE_TTL_MS), MAX_PRESENCE_TTL_MS);
  return new Date(Date.now() + effective).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

export function getRun(db: DatabaseSync, runId: string): WorkRunRecord {
  const row = db.prepare('SELECT * FROM task_runs WHERE run_id = ?').get(runId) as unknown as WorkRunRecord | undefined;
  if (!row) throw new Error(`run not found: ${runId}`);
  return row;
}

function fileRows(db: DatabaseSync, runId: string): WorkFileRecord[] {
  return db.prepare('SELECT * FROM run_files WHERE run_id = ? ORDER BY file_path')
    .all(runId) as unknown as WorkFileRecord[];
}

function activePeerRows(db: DatabaseSync, runId: string, files: string[]): WorkPeer[] {
  if (files.length === 0) return [];
  const now = utcNow();
  const rows = db.prepare(`SELECT rf.run_id, tr.task_id, tr.origin, tr.agent_id, rf.file_path,
      tr.rationale, rf.heartbeat_at, rf.expires_at,
      EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
        AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive
    FROM run_files rf
    JOIN task_runs tr ON tr.run_id = rf.run_id
    WHERE rf.run_id <> ?
      AND rf.file_path IN (${files.map(() => '?').join(',')})
      AND rf.ended_at IS NULL AND rf.expires_at > ? AND tr.status = 'ACTIVE'
    ORDER BY rf.file_path, rf.heartbeat_at DESC, rf.run_id`)
    .all(now, runId, ...files, now) as unknown as Array<WorkPeer & { exclusive: number | boolean }>;
  return rows.map((row) => ({ ...row, exclusive: Boolean(row.exclusive) }));
}

function mutationResult(db: DatabaseSync, runId: string, affectedFiles?: string[]): WorkMutationResult {
  const allFiles = fileRows(db, runId);
  const affected = affectedFiles ? new Set(affectedFiles) : null;
  const files = affected ? allFiles.filter((file) => affected.has(file.file_path)) : allFiles;
  const allPeers = activePeerRows(db, runId, files.filter((file) => file.ended_at == null).map((file) => file.file_path));
  return {
    run: getRun(db, runId),
    files,
    peers: allPeers.slice(0, PEER_DETAIL_LIMIT),
    peer_count: allPeers.length,
  };
}

function conflictRows(
  db: DatabaseSync,
  runId: string,
  files: string[],
  exclusive: boolean,
): WorkConflict[] {
  if (files.length === 0) return [];
  const now = utcNow();
  const placeholders = files.map(() => '?').join(',');
  if (exclusive) {
    return db.prepare(`SELECT rf.run_id, tr.task_id, tr.origin, tr.agent_id, rf.file_path,
        tr.rationale, rf.heartbeat_at, rf.expires_at,
        EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
          AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive,
        'ACTIVE_WORK' AS conflict_type
      FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id
      WHERE rf.file_path IN (${placeholders}) AND rf.run_id <> ?
        AND rf.ended_at IS NULL AND rf.expires_at > ? AND tr.status = 'ACTIVE'
      ORDER BY rf.file_path, rf.heartbeat_at DESC`)
      .all(now, ...files, runId, now) as unknown as WorkConflict[];
  }
  return db.prepare(`SELECT l.run_id, tr.task_id, tr.origin, tr.agent_id, l.file_path,
      tr.rationale, rf.heartbeat_at, COALESCE(l.expires_at, rf.expires_at) AS expires_at,
      1 AS exclusive, 'EXCLUSIVE_LOCK' AS conflict_type
    FROM locks l
    JOIN task_runs tr ON tr.run_id = l.run_id
    LEFT JOIN run_files rf ON rf.run_id = l.run_id AND rf.file_path = l.file_path
    WHERE l.file_path IN (${placeholders}) AND l.run_id <> ? AND tr.status = 'ACTIVE'
      AND (l.expires_at IS NULL OR l.expires_at > ?)
    ORDER BY l.file_path, l.acquired_at DESC`)
    .all(...files, runId, now) as unknown as WorkConflict[];
}

export function startWork(db: DatabaseSync, params: StartWorkParams): StartWorkResult {
  const agentId = required(params.agentId, 'agent id');
  const now = utcNow();
  const expiresAt = expiry(params.ttlMs);
  const requestedOrigin = params.origin ?? 'WORK';
  const source = params.source ?? (requestedOrigin === 'HOOK' ? 'HOOK' : 'EXPLICIT');
  let fileBasePath = params.workspacePath ?? process.cwd();
  let wsPath = workspaceRoot(params.workspacePath);
  let artifact = normalizeArtifact(params.artifact);
  let runId = params.runId ?? null;

  if (!runId) {
    required(params.rationale, 'rationale');
    required(params.testPlan, 'test plan');
  }

  db.exec('BEGIN IMMEDIATE');
  try {
    runId ??= `run_${randomUUID().replace(/-/g, '')}`;

    let run = db.prepare('SELECT * FROM task_runs WHERE run_id = ?').get(runId) as unknown as WorkRunRecord | undefined;
    if (run) {
      if (run.agent_id !== agentId) throw new Error(`run ${runId} belongs to ${run.agent_id}`);
      if (run.status !== 'ACTIVE') throw new Error(`run ${runId} is not ACTIVE`);
      const runWorkspace = workspaceRoot(run.workspace_path);
      if (params.workspacePath != null && wsPath !== runWorkspace) {
        throw new Error(`workspace ${wsPath} does not match run workspace ${runWorkspace}`);
      }
      const runArtifact = normalizeArtifact(run.artifact);
      if (params.artifact != null && artifact !== runArtifact) {
        throw new Error(`artifact ${artifact ?? '(none)'} does not match run artifact ${runArtifact ?? '(none)'}`);
      }
      wsPath = runWorkspace;
      artifact = runArtifact;
      fileBasePath = runWorkspace;
      if (params.sessionId != null) {
        if (params.sessionId !== run.session_id) {
          throw new Error(`run ${runId} belongs to session ${run.session_id ?? '(none)'}`);
        }
        ensureRunSession(db, {
          sessionId: params.sessionId,
          agentId,
          workspacePath: runWorkspace,
          artifact: runArtifact,
        });
      }
    } else {
      if (params.runId) throw new Error(`run not found: ${params.runId}`);
      if (params.sessionId) {
        ensureRunSession(db, {
          sessionId: params.sessionId,
          agentId,
          workspacePath: wsPath,
          artifact,
        });
      }
      db.prepare(`INSERT INTO task_runs
        (run_id, task_id, origin, agent_id, session_id, rationale, test_plan, context_ref,
         status, workspace_path, artifact, created_at, updated_at)
        VALUES (?, NULL, ?, ?, ?, ?, ?, ?, 'ACTIVE', ?, ?, ?, ?)`)
        .run(runId, requestedOrigin, agentId, params.sessionId ?? null,
          required(params.rationale, 'rationale'), required(params.testPlan, 'test plan'),
          params.contextRef ?? null, wsPath, artifact, now, now);
      run = getRun(db, runId);
    }

    const files = normalizeFiles(params.targetFiles, fileBasePath);

    const conflicts = conflictRows(db, runId, files, params.exclusive === true);
    if (conflicts.length > 0) {
      db.exec('ROLLBACK');
      return { ok: false, conflict: true, conflicts };
    }

    const upsert = db.prepare(`INSERT INTO run_files
      (run_id, file_path, reason_override, source, started_at, heartbeat_at, expires_at, ended_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, NULL)
      ON CONFLICT(run_id, file_path) DO UPDATE SET
        reason_override = COALESCE(excluded.reason_override, run_files.reason_override),
        source = excluded.source,
        started_at = CASE WHEN run_files.ended_at IS NULL THEN run_files.started_at ELSE excluded.started_at END,
        heartbeat_at = excluded.heartbeat_at,
        expires_at = excluded.expires_at,
        ended_at = NULL`);
    for (const file of files) {
      upsert.run(runId, file, params.reasonOverride?.trim() || null, source, now, now, expiresAt);
      if (params.exclusive) {
        db.prepare(`INSERT INTO locks(lock_id, file_path, run_id, acquired_at, expires_at)
          VALUES (?, ?, ?, ?, ?)
          ON CONFLICT(file_path, run_id) DO UPDATE SET expires_at = excluded.expires_at`)
          .run(`lock_${randomUUID().replace(/-/g, '')}`, file, runId, now, expiresAt);
      }
    }
    db.prepare('UPDATE task_runs SET updated_at = ? WHERE run_id = ?').run(now, runId);
    db.exec('COMMIT');
    return { ok: true, ...mutationResult(db, runId, files) };
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    throw error;
  }
}

export function renewWorkLease(
  db: DatabaseSync,
  params: TouchWorkParams,
  options: { exclusiveOnly?: boolean } = {},
): { result: WorkMutationResult; locksRenewed: number; expiresAt: string | null } {
  const run = getRun(db, params.runId);
  if (run.agent_id !== params.agentId) throw new Error(`run ${params.runId} belongs to ${run.agent_id}`);
  if (run.status !== 'ACTIVE') throw new Error(`run ${params.runId} is not ACTIVE`);
  const now = utcNow();
  const expiresAt = expiry(params.ttlMs);

  db.exec('BEGIN IMMEDIATE');
  try {
    const currentRun = getRun(db, params.runId);
    if (currentRun.agent_id !== params.agentId) {
      throw new Error(`run ${params.runId} belongs to ${currentRun.agent_id}`);
    }
    if (currentRun.status !== 'ACTIVE') throw new Error(`run ${params.runId} is not ACTIVE`);
    const allLockRows = db.prepare('SELECT file_path FROM locks WHERE run_id = ?')
      .all(params.runId) as unknown as Array<{ file_path: string }>;
    const lockedTargets = new Set(allLockRows.map((row) => row.file_path));
    const targets = options.exclusiveOnly
      ? [...lockedTargets]
      : params.targetFiles?.length
        ? normalizeFiles(params.targetFiles, currentRun.workspace_path)
        : fileRows(db, params.runId).filter((file) => file.ended_at == null).map((file) => file.file_path);
    if (targets.length === 0) {
      db.exec('COMMIT');
      if (options.exclusiveOnly) {
        return { result: mutationResult(db, params.runId, []), locksRenewed: 0, expiresAt: null };
      }
      throw new Error('run has no active file presence');
    }

    const present = db.prepare(`SELECT file_path FROM run_files
      WHERE run_id = ? AND ended_at IS NULL AND file_path IN (${targets.map(() => '?').join(',')})`)
      .all(params.runId, ...targets) as unknown as Array<{ file_path: string }>;
    if (present.length !== targets.length) throw new Error('one or more active file presences were not found for this run');

    db.prepare('DELETE FROM locks WHERE expires_at IS NOT NULL AND expires_at <= ?').run(now);
    for (const file of targets) {
      const conflicts = conflictRows(db, params.runId, [file], lockedTargets.has(file));
      if (conflicts.length > 0) {
        throw new Error(`work lease conflict on ${file}: held by ${conflicts.map((item) => item.agent_id).join(', ')}`);
      }
    }

    const update = db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?
      WHERE run_id = ? AND file_path = ? AND ended_at IS NULL`);
    for (const file of targets) {
      const result = update.run(now, expiresAt, params.runId, file) as { changes: number };
      if (result.changes === 0) throw new Error(`active file presence not found: ${file}`);
      if (lockedTargets.has(file)) {
        db.prepare(`INSERT INTO locks(lock_id, file_path, run_id, acquired_at, expires_at)
          VALUES (?, ?, ?, ?, ?)
          ON CONFLICT(file_path, run_id) DO UPDATE SET expires_at = excluded.expires_at`)
          .run(`lock_${randomUUID().replace(/-/g, '')}`, file, params.runId, now, expiresAt);
      }
    }
    db.prepare('UPDATE task_runs SET updated_at = ? WHERE run_id = ?').run(now, params.runId);
    db.exec('COMMIT');
    return {
      result: mutationResult(db, params.runId, targets),
      locksRenewed: targets.filter((file) => lockedTargets.has(file)).length,
      expiresAt,
    };
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    throw error;
  }
}

export function touchWork(db: DatabaseSync, params: TouchWorkParams): WorkMutationResult {
  return renewWorkLease(db, params).result;
}

export function endWork(db: DatabaseSync, params: EndWorkParams): WorkMutationResult {
  const run = getRun(db, params.runId);
  if (run.agent_id !== params.agentId) throw new Error(`run ${params.runId} belongs to ${run.agent_id}`);
  if (run.origin === 'TASK') throw new Error('TASK work must end through task submit or task release');
  const targets = params.targetFiles?.length
    ? normalizeFiles(params.targetFiles, run.workspace_path)
    : fileRows(db, params.runId).filter((file) => file.ended_at == null).map((file) => file.file_path);
  const now = utcNow();

  db.exec('BEGIN IMMEDIATE');
  try {
    if (targets.length > 0) {
      const ended = db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
        WHERE run_id = ? AND file_path IN (${targets.map(() => '?').join(',')}) AND ended_at IS NULL`)
        .run(now, now, now, params.runId, ...targets) as { changes: number };
      if (ended.changes !== targets.length) {
        throw new Error('one or more active file presences were not found for this run');
      }
      db.prepare(`DELETE FROM locks WHERE run_id = ?
        AND file_path IN (${targets.map(() => '?').join(',')})`)
        .run(params.runId, ...targets);
    }
    const active = db.prepare(`SELECT 1 FROM run_files
      WHERE run_id = ? AND ended_at IS NULL AND expires_at > ? LIMIT 1`).get(params.runId, now);
    if (!active) {
      db.prepare(`UPDATE task_runs SET status = 'PENDING', updated_at = ?
        WHERE run_id = ? AND status = 'ACTIVE' AND origin IN ('WORK','HOOK')`).run(now, params.runId);
    }
    db.exec('COMMIT');
  } catch (error) {
    try { db.exec('ROLLBACK'); } catch { /* transaction did not open */ }
    throw error;
  }
  return mutationResult(db, params.runId, targets);
}

export function listWork(db: DatabaseSync, params: ListWorkParams = {}): ListWorkResult {
  const now = utcNow();
  const where = ['1 = 1'];
  const binds: Array<string | number> = [now];
  if (params.activeOnly !== false) {
    where.push("rf.ended_at IS NULL", 'rf.expires_at > ?', "tr.status = 'ACTIVE'");
    binds.push(now);
  }
  if (params.workspacePath) { where.push('tr.workspace_path = ?'); binds.push(workspaceRoot(params.workspacePath)); }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) { where.push('(tr.artifact = ? OR tr.artifact IS NULL)'); binds.push(artifact); }
  if (params.agentId) { where.push('tr.agent_id = ?'); binds.push(params.agentId); }
  if (params.runId) { where.push('tr.run_id = ?'); binds.push(params.runId); }
  if (params.filePath) {
    where.push('rf.file_path = ?');
    binds.push(normalizeFiles([params.filePath], params.workspacePath)[0]!);
  }
  const limit = params.limit == null ? null : Math.max(1, Math.floor(params.limit));
  const limitSql = limit == null ? '' : 'LIMIT ?';
  if (limit != null) binds.push(limit);
  const rows = db.prepare(`SELECT rf.*, tr.task_id, tr.origin, tr.agent_id, tr.session_id,
      tr.rationale, tr.test_plan, tr.status, tr.workspace_path, tr.artifact,
      EXISTS(SELECT 1 FROM locks l WHERE l.run_id = rf.run_id AND l.file_path = rf.file_path
        AND (l.expires_at IS NULL OR l.expires_at > ?)) AS exclusive,
      COUNT(*) OVER() AS result_total
    FROM run_files rf JOIN task_runs tr ON tr.run_id = rf.run_id
    WHERE ${where.join(' AND ')}
    ORDER BY rf.file_path, rf.heartbeat_at DESC, rf.run_id ${limitSql}`)
    .all(...binds) as unknown as Array<WorkPresence & { exclusive: number | boolean; result_total: number }>;
  const totalCount = rows[0]?.result_total ?? 0;
  const files = rows.map(({ result_total: _total, ...row }) => ({ ...row, exclusive: Boolean(row.exclusive) }));
  return {
    count: files.length,
    total_count: totalCount,
    omitted_count: Math.max(0, totalCount - files.length),
    files,
  };
}

export function showWork(
  db: DatabaseSync,
  params: Omit<ListWorkParams, 'filePath'> & { filePath: string },
): ListWorkResult {
  return listWork(db, { ...params, filePath: params.filePath });
}
