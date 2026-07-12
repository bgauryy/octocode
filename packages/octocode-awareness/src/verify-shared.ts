/**
 * verify.ts — Verify-gate operations for the awareness Stop hook.
 *
 * auditUnverified: returns runs with status='PENDING' (edited but not verified)
 *                  for an agent/workspace. The Stop hook (stop-verify.sh) blocks
 *                  conclude when count > 0.
 *
 * markVerified:    transitions a run PENDING → SUCCESS | FAILED so the gate
 *                  clears after the agent verifies its edits. Restricted to PENDING
 *                  transitions to prevent orphaning ACTIVE locks as SUCCESS.
 *                  A linked plan task moves VERIFY → DONE | FAILED with it.
 */
import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import type { RunStatus } from './types.js';

// ─── Public shapes ────────────────────────────────────────────────────────────

export interface UnverifiedIntent {
  run_id: string;
  agent_id: string;
  status: string;
  test_plan: string;
  context_ref: string | null;
  rationale: string;
  target_files: string[];
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
}

/**
 * VER-2: An ACTIVE run whose declared file presence has expired.
 * These are orphaned work units the old audit silently missed.
 */
export interface StaleActiveIntent {
  run_id: string;
  agent_id: string;
  status: 'ACTIVE';
  rationale: string;
  context_ref: string | null;
  target_files: string[];
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
  age_hours: number; // how long stuck ACTIVE with no live file presence
}

export interface AuditUnverifiedResult {
  ok: true;
  unverified: UnverifiedIntent[];    // status=PENDING: released, awaiting verify
  stale_active: StaleActiveIntent[]; // VER-2: ACTIVE with no live file presence
  count: number;                     // total = unverified.length + stale_active.length
}

export interface AuditUnverifiedParams {
  agentId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  olderThanDays?: number | null;
  origins?: Array<'TASK' | 'WORK' | 'HOOK'>;
  before?: string | null;
}

export type VerifyStatus = 'SUCCESS' | 'FAILED';

export interface MarkVerifiedParams {
  runId?: string;            // verify one run; required unless allPending=true
  agentId?: string;
  allPending?: boolean;       // verify ALL pending runs for this agent/workspace
  workspacePath?: string | null;
  artifact?: string | null;
  message?: string;           // what was verified
  status?: VerifyStatus;
}

export interface MarkVerifiedOk {
  ok: true;
  // VER-1: null when allPending=true (no single task applies in batch mode).
  // Callers must guard for null when using allPending.
  run_id: string | null;
  run_ids?: string[];   // set when allPending=true
  count?: number;        // set when allPending=true
  status: RunStatus;
  updated_at: string;
}

export interface MarkVerifiedErr {
  ok: false;
  error: string;
  run_id: string | null;
}

export type MarkVerifiedResult = MarkVerifiedOk | MarkVerifiedErr;

// ─── Internal ─────────────────────────────────────────────────────────────────

export const VALID_VERIFY_STATUSES = new Set<string>(['SUCCESS', 'FAILED']);

export interface IntentDbRow {
  run_id: string;
  agent_id: string;
  status: string;
  test_plan: string;
  context_ref: string | null;
  rationale: string;
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
}

export interface AgentStatusRow {
  agent_id: string;
  status: string;
}

/**
 * Batched target-file lookup — chunked IN queries instead of one SELECT per
 * run. Chunking keeps huge audits under SQLITE_MAX_VARIABLE_NUMBER.
 */
export function targetFilesForRuns(db: DatabaseSync, runIds: string[]): Map<string, string[]> {
  const byRun = new Map<string, string[]>(runIds.map((id) => [id, []]));
  for (let offset = 0; offset < runIds.length; offset += 500) {
    const chunk = runIds.slice(offset, offset + 500);
    const rows = db.prepare(
      `SELECT run_id, file_path FROM run_files
       WHERE run_id IN (${chunk.map(() => '?').join(',')})
       ORDER BY file_path`,
    ).all(...chunk) as unknown as Array<{ run_id: string; file_path: string }>;
    for (const row of rows) byRun.get(row.run_id)?.push(row.file_path);
  }
  return byRun;
}

export function closeRunFiles(db: DatabaseSync, runId: string, now: string): void {
  db.prepare('DELETE FROM locks WHERE run_id = ?').run(runId);
  db.prepare(`UPDATE run_files SET heartbeat_at = ?, expires_at = ?, ended_at = ?
    WHERE run_id = ? AND ended_at IS NULL`).run(now, now, now, runId);
}

export function finishLinkedTask(
  db: DatabaseSync,
  runId: string,
  status: VerifyStatus,
  agentId: string,
  now: string,
  message?: string,
): void {
  const linked = db.prepare('SELECT task_id FROM task_runs WHERE run_id = ?')
    .get(runId) as { task_id: string | null } | undefined;
  if (!linked?.task_id) return;
  const taskStatus = status === 'SUCCESS' ? 'DONE' : 'FAILED';
  const updated = db.prepare(`UPDATE tasks SET status = ?, updated_at = ?, completed_at = ?
    WHERE task_id = ? AND status = 'VERIFY'`)
    .run(taskStatus, now, now, linked.task_id);
  if (updated.changes === 0) return;
  db.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)`)
    .run(`tevt_${randomUUID().replace(/-/g, '')}`, linked.task_id, runId, agentId,
      status === 'SUCCESS' ? 'VERIFIED' : 'VERIFICATION_FAILED', message ?? taskStatus, now);
}

export function failStaleLinkedTask(
  db: DatabaseSync,
  runId: string,
  agentId: string,
  now: string,
  message: string,
): void {
  const linked = db.prepare('SELECT task_id FROM task_runs WHERE run_id = ?')
    .get(runId) as { task_id: string | null } | undefined;
  if (!linked?.task_id) return;
  const updated = db.prepare(`UPDATE tasks SET status = 'FAILED', updated_at = ?, completed_at = ?
    WHERE task_id = ? AND status IN ('IN_PROGRESS', 'VERIFY')`)
    .run(now, now, linked.task_id);
  if (updated.changes === 0) return;
  db.prepare('DELETE FROM task_claims WHERE task_id = ?').run(linked.task_id);
  db.prepare(`INSERT INTO task_events(event_id, task_id, run_id, agent_id, event_type, message, created_at)
    VALUES (?, ?, ?, ?, 'VERIFICATION_FAILED', ?, ?)`)
    .run(`tevt_${randomUUID().replace(/-/g, '')}`, linked.task_id, runId, agentId, message, now);
}
