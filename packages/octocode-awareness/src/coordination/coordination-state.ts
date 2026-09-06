import { auditUnverified } from '../verify-audit.js';
import { fileLock } from '../intents-lock.js';
import { activeLockRecords } from '../intents-preflight.js';
import { releaseFileLock } from '../intents-release.js';
import { startWork, listWork, endWork, getRun, normalizeFiles } from '../work.js';
import { markVerified } from '../verify-mark.js';
import type { CheckAudit,CheckStatus,HandoffNote,Lock,LockWaitResult,Task,WorkPresence } from '@octocodeai/agent-contracts/entities';
import { CoordinationPlansTasks } from './coordination-plans-tasks.js';
import { handoffFromRow,type HandoffRow,id,normalizeLeaseSeconds,now,required,sleepMs,splitFiles } from './coordination-shared.js';

export abstract class CoordinationState extends CoordinationPlansTasks {
  acquireLock(params: { filePath: string; agentId: string; runId?: string; reason?: string | null; testPlan?: string; ttlSeconds?: number }): Lock {
    const result = fileLock(this.db, { type: 'lock', targetFiles: [required(params.filePath, 'file')],
      agentId: required(params.agentId, 'agent-id'), runId: params.runId, workspacePath: this.workspace,
      reasoning: params.reason, testPlan: params.testPlan, ttlMs: normalizeLeaseSeconds(params.ttlSeconds) * 1000 });
    if (!result.ok || result.type !== 'lock') throw new Error(`lock conflict on ${params.filePath}`);
    const lock = this.listLocks().find(item => item.runId === result.run_id && item.filePath === normalizeFiles([params.filePath], this.workspace)[0]);
    if (!lock) throw new Error('lock acquisition produced no active lock');
    return lock;
  }

  listLocks(): Lock[] {
    return activeLockRecords(this.db, { workspacePath: this.workspace }).map(row => ({
      runId: row.run_id, filePath: row.file_path, agentId: row.agent_id,
      reason: row.reason, acquiredAt: row.acquired_at, expiresAt: row.expires_at,
    }));
  }

  waitForLock(params: { filePath: string; agentId?: string | null; waitMs?: number; retryIntervalMs?: number }): LockWaitResult {
    const filePath = normalizeFiles([required(params.filePath, 'file')], this.workspace)[0]!;
    if (!Number.isFinite(params.waitMs ?? 0) || (params.waitMs ?? 0) < 0) throw new Error('wait must be a finite nonnegative duration');
    if (!Number.isFinite(params.retryIntervalMs ?? 250) || (params.retryIntervalMs ?? 250) <= 0) throw new Error('retry interval must be a finite positive duration');
    const waitMs = Math.min(Math.floor(params.waitMs ?? 0), 60_000);
    const interval = Math.min(Math.max(Math.floor(params.retryIntervalMs ?? 250), 25), 5000);
    const start = performance.now();
    for (;;) {
      const conflict = this.listLocks().find(row => row.filePath === filePath && row.agentId !== params.agentId) ?? null;
      const waitedMs = Math.floor(performance.now() - start);
      if (!conflict) return { ok: true, lockFree: true, filePath, waitedMs, conflict: null };
      if (waitedMs >= waitMs) return { ok: false, lockFree: false, filePath, waitedMs, conflict };
      sleepMs(Math.min(interval, waitMs - waitedMs));
    }
  }

  pruneLocks(params: { dryRun?: boolean } = {}): { dryRun: boolean; matched: number; deleted: number } {
    const where = 'expires_at <= ? AND run_id IN (SELECT run_id FROM task_runs WHERE workspace_path = ?)';
    const stamp = now();
    const matched = (this.db.prepare(`SELECT COUNT(*) AS count FROM awareness_locks WHERE ${where}`).get(stamp, this.workspace) as { count: number }).count;
    const dryRun = params.dryRun !== false;
    const deleted = dryRun ? 0 : Number(this.db.prepare(`DELETE FROM awareness_locks WHERE ${where}`).run(stamp, this.workspace).changes);
    return { dryRun, matched, deleted };
  }

  releaseLock(params: { filePath: string; agentId: string; runId: string }): { released: boolean } {
    const run = getRun(this.db, required(params.runId, 'run-id'));
    if (run.workspace_path !== this.workspace || run.agent_id !== required(params.agentId, 'agent-id')) throw new Error('lock run ownership mismatch');
    return { released: releaseFileLock(this.db, { targetFiles: [params.filePath], runId: run.run_id,
      agentId: params.agentId, workspacePath: this.workspace, status: 'ACTIVE' }).released };
  }

  startWork(params: { filePath: string; agentId: string; runId?: string; reason?: string | null; testPlan?: string; ttlSeconds?: number }): WorkPresence {
    const result = startWork(this.db, { targetFiles: [required(params.filePath, 'file')], agentId: required(params.agentId, 'agent-id'),
      runId: params.runId, workspacePath: this.workspace, rationale: params.reason ?? undefined,
      testPlan: params.testPlan, reasonOverride: params.reason, ttlMs: normalizeLeaseSeconds(params.ttlSeconds) * 1000 });
    if (!result.ok) throw new Error(`work conflict on ${params.filePath}`);
    const file = result.files[0]!;
    return { runId: result.run.run_id, filePath: file.file_path, agentId: result.run.agent_id,
      reason: file.reason_override ?? result.run.rationale, startedAt: file.started_at,
      updatedAt: file.heartbeat_at, expiresAt: file.expires_at };
  }

  listWork(params: { filePath?: string | null; agentId?: string | null } = {}): WorkPresence[] {
    return listWork(this.db, { workspacePath: this.workspace, filePath: params.filePath, agentId: params.agentId }).files.map(row => ({
      runId: row.run_id, filePath: row.file_path, agentId: row.agent_id, reason: row.reason_override ?? row.rationale,
      startedAt: row.started_at, updatedAt: row.heartbeat_at, expiresAt: row.expires_at,
    }));
  }

  showWork(params: { filePath: string }): WorkPresence[] {
    return this.listWork({ filePath: required(params.filePath, 'file') });
  }

  endWork(params: { filePath: string; agentId: string; runId: string }): { ended: boolean } {
    const run = getRun(this.db, required(params.runId, 'run-id'));
    if (run.workspace_path !== this.workspace || run.agent_id !== required(params.agentId, 'agent-id')) throw new Error('work run ownership mismatch');
    const file = normalizeFiles([params.filePath], this.workspace)[0]!;
    const present = this.db.prepare('SELECT 1 FROM run_files WHERE run_id = ? AND file_path = ? AND ended_at IS NULL').get(run.run_id, file);
    if (!present) return { ended: false };
    endWork(this.db, { targetFiles: [file], agentId: params.agentId, runId: run.run_id });
    return { ended: true };
  }

  addHandoff(params: { agentId: string; summary: string; files?: string | string[] | null }): HandoffNote {
    const stamp = now();
    const handoffId = id('handoff');
    this.db.prepare('INSERT INTO handoffs(handoff_id, workspace_path, agent_id, summary, files_json, created_at, cleared_at) VALUES (?, ?, ?, ?, ?, ?, NULL)')
      .run(handoffId, this.workspace, required(params.agentId, 'agent-id'), required(params.summary, 'summary'), JSON.stringify(splitFiles(params.files)), stamp);
    return this.getHandoff(handoffId);
  }

  listHandoffs(params: { includeCleared?: boolean } = {}): HandoffNote[] {
    const rows = params.includeCleared
      ? this.db.prepare('SELECT * FROM handoffs WHERE workspace_path = ? ORDER BY created_at DESC').all(this.workspace)
      : this.db.prepare('SELECT * FROM handoffs WHERE workspace_path = ? AND cleared_at IS NULL ORDER BY created_at DESC').all(this.workspace);
    return (rows as unknown as HandoffRow[]).map(handoffFromRow);
  }

  clearHandoff(params: { handoffId: string }): { cleared: boolean } {
    const result = this.db.prepare('UPDATE handoffs SET cleared_at = ? WHERE workspace_path = ? AND handoff_id = ? AND cleared_at IS NULL')
      .run(now(), this.workspace, required(params.handoffId, 'handoff-id'));
    return { cleared: result.changes > 0 };
  }

  auditChecks(params: { agentId?: string | null; planId?: string | null; minAgeMs?: number | null } = {}): CheckAudit {
    const audit = auditUnverified(this.db, { workspacePath: this.workspace, agentId: params.agentId, minAgeMs: params.minAgeMs });
    const pending = audit.unverified.map(row => {
      const run = getRun(this.db, row.run_id);
      return { runId: row.run_id, taskId: run.task_id, agentId: row.agent_id,
        testPlan: row.test_plan, rationale: row.rationale, createdAt: row.created_at };
    }).filter(row => !params.planId || (row.taskId && this.getTask(row.taskId).planId === params.planId));
    return {
      ok: pending.length === 0,
      pending,
      pendingCount: pending.length,
      filters: {
        agentId: params.agentId?.trim() || null,
        planId: params.planId?.trim() || null,
        minAgeMs: params.minAgeMs ?? null,
      },
    };
  }

  markCheck(params: { taskId: string; runId: string; doneAt: string; agentId: string; message: string; status?: CheckStatus }): Task {
    return this.writeTransaction(() => {
      const task = this.getTask(params.taskId);
      const run = getRun(this.db, required(params.runId, 'run-id'));
      if (run.task_id !== task.taskId || run.workspace_path !== this.workspace) throw new Error('verification task/run scope mismatch');
      if (run.status !== 'PENDING' || run.updated_at !== required(params.doneAt, 'done-at')) throw new Error(`stale task completion: ${params.taskId}`);
      const result = markVerified(this.db, { runId: run.run_id, agentId: required(params.agentId, 'agent-id'),
        workspacePath: this.workspace, message: required(params.message, 'message'), status: params.status });
      if (!result.ok) throw new Error(result.error);
      return this.getTask(task.taskId);
    });
  }
}
