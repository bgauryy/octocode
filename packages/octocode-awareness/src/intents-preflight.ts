import type { FileLock } from './types/identity-memory.js';
/**
 * intents.ts — execution-run and file-lock operations.
 */
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { canonicalizePath, normalizeWorkspacePath } from './git.js';
import { startWork } from './work.js';
import type { FileLockReleaseStatus, PreFlightRunParams, PreFlightRunResult } from './types/locks-reflection.js';

export const MAX_LOCK_TTL_MS = 10 * 60_000;
export const VALID_RELEASE_STATUSES = new Set<FileLockReleaseStatus>(['PENDING', 'ACTIVE', 'FAILED']);

export function effectiveTtlMs(ttlMs: number | null | undefined): number {
  return Math.min(Math.max(1, ttlMs ?? MAX_LOCK_TTL_MS), MAX_LOCK_TTL_MS);
}

export function workspaceScopeRoot(workspacePath?: string | null): string {
  const candidate = workspacePath ?? process.cwd();
  return normalizeWorkspacePath(candidate, candidate) ?? resolve(candidate);
}

export function workspaceFileBase(workspacePath?: string | null): string {
  return workspacePath ? resolve(workspacePath) : process.cwd();
}

export function resolveTargetFiles(targetFiles: string[] = [], workspacePath?: string | null): string[] {
  const root = workspaceFileBase(workspacePath);
  return targetFiles.map((file) => canonicalizePath(isAbsolute(file) ? resolve(file) : resolve(root, file)));
}

export function toSimpleLock(params: {
  filePath: string;
  agentId: string;
  runId: string;
  reason: string;
  expiresAt: string | null;
  state?: FileLock['state'];
}): FileLock {
  return {
    path: params.filePath,
    agent: params.agentId,
    state: params.state ?? 'locked',
    reason: params.reason,
    run_id: params.runId,
    expires_at: params.expiresAt,
  };
}

export function activeLockRows(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null; agentId?: string | null; sessionId?: string | null; runId?: string | null } = {},
): FileLock[] {
  return activeLockRecords(db, params).map((row) => toSimpleLock({
    filePath: row.file_path,
    agentId: row.agent_id,
    runId: row.run_id,
    reason: row.reason,
    expiresAt: row.expires_at,
  }));
}

export interface ActiveLockRecord {
  run_id: string;
  file_path: string;
  agent_id: string;
  reason: string;
  acquired_at: string;
  expires_at: string | null;
}

export function activeLockRecords(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null; agentId?: string | null; sessionId?: string | null; runId?: string | null } = {},
): ActiveLockRecord[] {
  const now = utcNow();

  const clauses = ["ai.status = 'ACTIVE'", "(fl.expires_at IS NULL OR fl.expires_at > ?)"];
  const binds: (string | number)[] = [now];
  if (params.workspacePath) {
    clauses.push('ai.workspace_path = ?');
    binds.push(workspaceScopeRoot(params.workspacePath));
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) {
    clauses.push('(ai.artifact = ? OR ai.artifact IS NULL)');
    binds.push(artifact);
  }
  if (params.agentId) {
    clauses.push('ai.agent_id = ?');
    binds.push(params.agentId);
  }
  if (params.sessionId) {
    clauses.push('ai.session_id = ?');
    binds.push(params.sessionId);
  }
  if (params.runId) {
    clauses.push('fl.run_id = ?');
    binds.push(params.runId);
  }

  const rows = db.prepare(
    `SELECT fl.run_id, fl.file_path, ai.agent_id, ai.rationale AS reason, fl.acquired_at, fl.expires_at
       FROM awareness_locks fl
       JOIN task_runs ai ON ai.run_id = fl.run_id
      WHERE ${clauses.join(' AND ')}
      ORDER BY fl.acquired_at DESC`
  ).all(...binds) as unknown as ActiveLockRecord[];
  return rows;
}

/**
 * Claim file locks for an agent write operation.
 * Returns { ok: true, run } on success or { ok: false, conflict, conflicts } on conflict.
 */
export function preFlightIntent(
  db: DatabaseSync,
  params: PreFlightRunParams,
): PreFlightRunResult {
  const agentId = params.agentId ?? 'agent';
  const requireRunContract = params.requireRunContract === true && !params.runId;
  const result = startWork(db, {
    agentId,
    sessionId: params.sessionId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    runId: params.runId,
    rationale: requireRunContract ? params.rationale : params.rationale ?? 'agent write operation',
    testPlan: requireRunContract ? params.testPlan : params.testPlan ?? 'post-edit verification',
    contextRef: params.contextRef,
    targetFiles: params.targetFiles ?? [],
    origin: 'WORK',
    source: 'EXPLICIT',
    ttlMs: effectiveTtlMs(params.ttlMs),
    exclusive: true,
  });
  if (!result.ok) {
    return {
      ok: false,
      conflict: true,
      conflicts: result.conflicts.map((conflict) => toSimpleLock({
        filePath: conflict.file_path,
        agentId: conflict.agent_id,
        runId: conflict.run_id,
        reason: conflict.rationale,
        expiresAt: conflict.expires_at,
        state: 'conflict',
      })),
    };
  }
  const locks = activeLockRows(db, { runId: result.run.run_id });
  return {
    ok: true,
    run: {
      run_id: result.run.run_id,
      task_id: result.run.task_id,
      origin: result.run.origin,
      agent_id: result.run.agent_id,
      session_id: result.run.session_id,
      workspace_path: result.run.workspace_path ?? workspaceScopeRoot(params.workspacePath),
      artifact: result.run.artifact,
      context_ref: result.run.context_ref,
      target_files: result.files.filter((file) => file.ended_at == null).map((file) => file.file_path),
      locks,
      status: result.run.status,
      created_at: result.run.created_at,
    },
  };
}
