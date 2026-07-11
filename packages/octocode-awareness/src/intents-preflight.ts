/**
 * intents.ts — execution-run and file-lock operations.
 */
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { evictExpiredLocks } from './db.js';
import { canonicalizePath, normalizeWorkspacePath } from './git.js';
import { startWork } from './work.js';
import type { PreFlightRunParams, PreFlightRunResult, FileLockStatusEntry } from './types.js';

export const MAX_LOCK_TTL_MS = 10 * 60_000;
export const VALID_RELEASE_STATUSES = new Set(['PENDING', 'ACTIVE', 'SUCCESS', 'FAILED']);
export type ReleaseStatus = 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED';

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

export function activeLockRows(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null; agentId?: string | null; sessionId?: string | null; runId?: string | null } = {},
): FileLockStatusEntry[] {
  // ARCH-3: Delegate eviction to the shared evictExpiredLocks instead of
  // duplicating the DELETE. Note: eviction here is intentional — stale locks
  // must be cleared before the caller decides whether a file is locked.
  evictExpiredLocks(db);
  const now = utcNow(); // re-read after eviction so the SELECT filter is consistent

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

  return db.prepare(
    `SELECT fl.lock_id, fl.run_id, fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path, ai.artifact,
            ai.rationale AS reasoning, ai.test_plan AS test_plan, 'EXCLUSIVE' AS lock_type,
            fl.acquired_at, fl.expires_at
       FROM locks fl
       JOIN task_runs ai ON ai.run_id = fl.run_id
      WHERE ${clauses.join(' AND ')}
      ORDER BY fl.acquired_at DESC`
  ).all(...binds) as unknown as FileLockStatusEntry[];
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
  const result = startWork(db, {
    agentId,
    sessionId: params.sessionId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    runId: params.runId,
    rationale: params.rationale ?? 'agent write operation',
    testPlan: params.testPlan ?? 'post-edit verification',
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
      conflicts: result.conflicts.map((conflict) => {
        const holder = db.prepare(`SELECT tr.session_id, s.ended_at, l.acquired_at
          FROM task_runs tr
          LEFT JOIN sessions s ON s.session_id = tr.session_id
          LEFT JOIN locks l ON l.run_id = tr.run_id AND l.file_path = ?
          WHERE tr.run_id = ?`).get(conflict.file_path, conflict.run_id) as {
            session_id: string | null;
            ended_at: string | null;
            acquired_at: string | null;
          } | undefined;
        return {
          file_path: conflict.file_path,
          lock_type: 'EXCLUSIVE' as const,
          agent_id: conflict.agent_id,
          acquired_at: holder?.acquired_at ?? conflict.heartbeat_at,
          expires_at: conflict.expires_at,
          run_id: conflict.run_id,
          reasoning: conflict.rationale,
          test_plan: db.prepare('SELECT test_plan FROM task_runs WHERE run_id = ?')
            .get(conflict.run_id)?.['test_plan'] as string ?? 'post-edit verification',
          session_id: holder?.session_id ?? null,
          holder_session_active: !holder?.ended_at,
        };
      }),
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
      locks: locks.map((lock) => ({
        lock_id: lock.lock_id,
        file_path: lock.file_path,
        lock_type: 'EXCLUSIVE',
        agent_id: lock.agent_id,
        session_id: lock.session_id,
        acquired_at: lock.acquired_at,
        expires_at: lock.expires_at,
      })),
      status: result.run.status,
      created_at: result.run.created_at,
    },
  };
}
