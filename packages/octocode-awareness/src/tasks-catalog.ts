/** Durable plan tasks, dependencies, leases, and execution runs. */
import { randomUUID } from 'node:crypto';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { utcNow } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';

export type PlanTaskStatus = 'OPEN' | 'IN_PROGRESS' | 'BLOCKED' | 'VERIFY' | 'DONE' | 'FAILED' | 'CANCELLED';

export interface PlanTaskRecord {
  task_id: string;
  plan_id: string;
  title: string;
  reasoning: string;
  acceptance_criteria: string;
  status: PlanTaskStatus;
  priority: number;
  created_by: string;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
  paths: string[];
  dependencies: string[];
  claim: TaskClaimRecord | null;
}

export interface TaskClaimRecord {
  task_id: string;
  run_id: string;
  agent_id: string;
  claimed_at: string;
  heartbeat_at: string;
  expires_at: string;
}

export interface TaskRunRecord {
  run_id: string;
  task_id: string | null;
  origin: 'TASK' | 'WORK' | 'HOOK';
  agent_id: string;
  session_id: string | null;
  rationale: string;
  test_plan: string;
  context_ref: string | null;
  status: 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED';
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateTaskParams {
  planId: string;
  title: string;
  reasoning: string;
  acceptanceCriteria: string;
  paths: string[];
  createdBy: string;
  priority?: number;
  dependsOn?: string[];
}

export const DEFAULT_CLAIM_LEASE_MS = 30 * 60_000;
export const MAX_CLAIM_LEASE_MS = 60 * 60_000;

export function required(value: string, field: string): string {
  const normalized = value.trim();
  if (!normalized) throw new Error(`${field} is required`);
  return normalized;
}

export function normalizeTaskPaths(workspacePath: string, paths: string[]): string[] {
  if (paths.length === 0) throw new Error('at least one task path is required');
  const root = resolve(workspacePath);
  const normalized = paths.map((input) => {
    const value = required(input, 'task path');
    const absolute = isAbsolute(value) ? resolve(value) : resolve(root, value);
    const rel = relative(root, absolute);
    if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) {
      throw new Error(`task path must be workspace-relative and below the workspace: ${input}`);
    }
    return rel.split(sep).join('/');
  });
  return [...new Set(normalized)];
}

export function event(
  db: DatabaseSync,
  taskId: string,
  runId: string | null,
  agentId: string,
  eventType: string,
  message: string,
  now = utcNow(),
): void {
  db.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)`)
    .run(`tevt_${randomUUID().replace(/-/g, '')}`, taskId, runId, agentId, eventType, message, now);
}

export function evictExpiredTaskClaims(db: DatabaseSync, now = utcNow()): void {
  const expired = db.prepare(
    'SELECT task_id, run_id, agent_id FROM task_claims WHERE expires_at <= ?',
  ).all(now) as unknown as Array<{ task_id: string; run_id: string; agent_id: string }>;
  if (expired.length === 0) return;

  // SAVEPOINT (not BEGIN): claimTask already holds BEGIN IMMEDIATE when it calls us.
  db.exec('SAVEPOINT evict_expired_task_claims');
  try {
    for (const claim of expired) {
      db.prepare('DELETE FROM locks WHERE run_id = ?').run(claim.run_id);
      db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
        WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, claim.run_id);
      db.prepare("UPDATE task_runs SET status = 'FAILED', updated_at = ? WHERE run_id = ? AND status = 'ACTIVE'")
        .run(now, claim.run_id);
      db.prepare("UPDATE tasks SET status = 'OPEN', updated_at = ? WHERE task_id = ? AND status = 'IN_PROGRESS'")
        .run(now, claim.task_id);
      db.prepare('DELETE FROM task_claims WHERE task_id = ?').run(claim.task_id);
      event(db, claim.task_id, claim.run_id, claim.agent_id, 'CLAIM_EXPIRED', 'claim lease expired', now);
    }
    db.exec('RELEASE SAVEPOINT evict_expired_task_claims');
  } catch (e) {
    try { db.exec('ROLLBACK TO SAVEPOINT evict_expired_task_claims'); } catch { /* already rolled back */ }
    try { db.exec('RELEASE SAVEPOINT evict_expired_task_claims'); } catch { /* already released */ }
    throw e;
  }
}

export function hydrateTask(db: DatabaseSync, row: Record<string, unknown>): PlanTaskRecord {
  const taskId = String(row['task_id']);
  const paths = db.prepare('SELECT path FROM task_paths WHERE task_id = ? ORDER BY ordinal, path')
    .all(taskId).map((item) => String((item as Record<string, unknown>)['path']));
  const dependencies = db.prepare(
    'SELECT depends_on_task_id FROM task_dependencies WHERE task_id = ? ORDER BY created_at, depends_on_task_id',
  ).all(taskId).map((item) => String((item as Record<string, unknown>)['depends_on_task_id']));
  const claim = db.prepare('SELECT * FROM task_claims WHERE task_id = ?').get(taskId) as unknown as TaskClaimRecord | undefined;
  return { ...(row as unknown as Omit<PlanTaskRecord, 'paths' | 'dependencies' | 'claim'>), paths, dependencies, claim: claim ?? null };
}

export function getTask(db: DatabaseSync, taskId: string): PlanTaskRecord | null {
  evictExpiredTaskClaims(db);
  const row = db.prepare('SELECT * FROM tasks WHERE task_id = ?').get(taskId) as Record<string, unknown> | undefined;
  return row ? hydrateTask(db, row) : null;
}

/** Return one unambiguous live plan-task claim for hook attachment. */
export function activeTaskClaimForAgent(
  db: DatabaseSync,
  params: { agentId: string; workspacePath: string; artifact?: string | null },
): TaskClaimRecord | null {
  evictExpiredTaskClaims(db);
  const workspacePath = normalizeWorkspacePath(params.workspacePath, params.workspacePath)
    ?? resolve(params.workspacePath);
  const where = ['c.agent_id = ?', 'p.workspace_path = ?', 'c.expires_at > ?'];
  const binds: Array<string | null> = [params.agentId, workspacePath, utcNow()];
  if (params.artifact) {
    where.push('(p.artifact = ? OR p.artifact IS NULL)');
    binds.push(params.artifact);
  }
  const claims = db.prepare(`SELECT c.* FROM task_claims c
    JOIN tasks t ON t.task_id = c.task_id
    JOIN plans p ON p.plan_id = t.plan_id
    WHERE ${where.join(' AND ')} ORDER BY c.claimed_at DESC LIMIT 2`)
    .all(...binds) as unknown as TaskClaimRecord[];
  return claims.length === 1 ? claims[0]! : null;
}
