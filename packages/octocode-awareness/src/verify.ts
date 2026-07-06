/**
 * verify.ts — Verify-gate operations for the awareness Stop hook.
 *
 * auditUnverified: returns intents with status='PENDING' (edited but not verified)
 *                  for an agent/workspace. The Stop hook (stop-verify.sh) blocks
 *                  conclude when count > 0.
 *
 * markVerified:    transitions an intent PENDING → SUCCESS | FAILED so the gate
 *                  clears after the agent verifies its edits. Restricted to PENDING
 *                  transitions to prevent orphaning ACTIVE locks as SUCCESS.
 */

import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { utcNow, parseJsonList } from './helpers.js';
import type { IntentStatus } from './types.js';

// ─── Public shapes ────────────────────────────────────────────────────────────

export interface UnverifiedIntent {
  intent_id: string;
  agent_id: string;
  status: string;
  test_plan: string;
  rationale: string;
  target_files: string[];
  workspace_path: string | null;
  created_at: string;
}

/**
 * VER-2: An ACTIVE intent whose file_locks have all been evicted (expired).
 * These are orphaned sessions the old audit silently missed.
 */
export interface StaleActiveIntent {
  intent_id: string;
  agent_id: string;
  status: 'ACTIVE';
  rationale: string;
  target_files: string[];
  workspace_path: string | null;
  created_at: string;
  age_hours: number; // how long stuck ACTIVE with no live locks
}

export interface AuditUnverifiedResult {
  ok: true;
  unverified: UnverifiedIntent[];    // status=PENDING: released, awaiting verify
  stale_active: StaleActiveIntent[]; // VER-2: ACTIVE with no live file_locks
  count: number;                     // total = unverified.length + stale_active.length
}

export interface AuditUnverifiedParams {
  agentId?: string | null;
  workspacePath?: string | null;
  abandon?: boolean;         // dismiss all PENDING intents as FAILED (clear orphaned)
}

export type VerifyStatus = 'SUCCESS' | 'FAILED';

export interface MarkVerifiedParams {
  intentId?: string;          // verify one intent; required unless allPending=true
  agentId?: string;
  allPending?: boolean;       // verify ALL pending intents for this agent/workspace
  workspacePath?: string | null;
  message?: string;           // what was verified
  status?: VerifyStatus;
}

export interface MarkVerifiedOk {
  ok: true;
  // VER-1: null when allPending=true (no single intent applies in batch mode).
  // Callers must guard for null when using allPending.
  intent_id: string | null;
  intent_ids?: string[];   // set when allPending=true
  count?: number;          // set when allPending=true
  status: IntentStatus;
  updated_at: string;
}

export interface MarkVerifiedErr {
  ok: false;
  error: string;
  intent_id: string | null;
}

export type MarkVerifiedResult = MarkVerifiedOk | MarkVerifiedErr;

// ─── Internal ─────────────────────────────────────────────────────────────────

const VALID_VERIFY_STATUSES = new Set<string>(['SUCCESS', 'FAILED']);

interface IntentDbRow {
  intent_id: string;
  agent_id: string;
  status: string;
  test_plan: string;
  rationale: string;
  workspace_path: string | null;
  files_json: string;
  created_at: string;
}

interface AgentStatusRow {
  agent_id: string;
  status: string;
}

// ─── Exports ──────────────────────────────────────────────────────────────────

/**
 * Return all agent_intents rows with status='PENDING', optionally scoped to an
 * agent and/or workspace. A non-zero count means the Stop hook should block
 * conclude.
 */
export function auditUnverified(
  db: DatabaseSync,
  params: AuditUnverifiedParams = {},
): AuditUnverifiedResult {
  const where: string[] = ["status = 'PENDING'"];
  const binds: (string | number)[] = [];

  if (params.agentId) {
    where.push('agent_id = ?');
    binds.push(params.agentId);
  }
  if (params.workspacePath) {
    where.push('workspace_path = ?');
    binds.push(params.workspacePath);
  }

  const rows = db.prepare(
    `SELECT intent_id, agent_id, status, test_plan, rationale, workspace_path, files_json, created_at
     FROM agent_intents
     WHERE ${where.join(' AND ')}
     ORDER BY created_at ASC`,
  ).all(...binds) as unknown as IntentDbRow[];

  const unverified: UnverifiedIntent[] = rows.map(r => ({
    intent_id: r.intent_id,
    agent_id: r.agent_id,
    status: r.status,
    test_plan: r.test_plan,
    rationale: r.rationale,
    target_files: parseJsonList(r.files_json),
    workspace_path: r.workspace_path,
    created_at: r.created_at,
  }));

  if (params.abandon && unverified.length > 0) {
    const now = utcNow();
    for (const intent of unverified) {
      db.prepare(
        "UPDATE agent_intents SET status = 'FAILED', updated_at = ? WHERE intent_id = ? AND status = 'PENDING'"
      ).run(now, intent.intent_id);
      try {
        db.prepare(
          `INSERT INTO intent_events(event_id, intent_id, agent_id, event_type, message, created_at)
           VALUES (?, ?, ?, 'ABANDONED', 'orphaned by audit-unverified --abandon', ?)`
        ).run('evt_' + randomUUID().replace(/-/g, ''), intent.intent_id, intent.agent_id, now);
      } catch { /* intent_events may not exist */ }
    }
  }

  // VER-2: Detect ACTIVE intents whose file_locks have all expired/been evicted.
  // These are orphaned sessions — the agent crashed or exited without releasing locks.
  // We query for ACTIVE intents that have no remaining live file_locks.
  const staleActive: StaleActiveIntent[] = [];
  try {
    const nowIso = utcNow();
    const staleWhere: string[] = [
      "ai.status = 'ACTIVE'",
      // No live lock remains: all locks either deleted or expired
      `NOT EXISTS (
        SELECT 1 FROM file_locks fl
        WHERE fl.intent_id = ai.intent_id
          AND (fl.expires_at IS NULL OR fl.expires_at > ?)
      )`,
    ];
    const staleBinds: (string | number)[] = [nowIso];
    if (params.agentId) { staleWhere.push('ai.agent_id = ?'); staleBinds.push(params.agentId); }
    if (params.workspacePath) { staleWhere.push('ai.workspace_path = ?'); staleBinds.push(params.workspacePath); }

    const staleRows = db.prepare(
      `SELECT ai.intent_id, ai.agent_id, ai.rationale, ai.workspace_path, ai.files_json, ai.created_at
       FROM agent_intents ai
       WHERE ${staleWhere.join(' AND ')}
       ORDER BY ai.created_at ASC`
    ).all(...staleBinds) as unknown as IntentDbRow[];

    for (const r of staleRows) {
      const ageMs = Date.now() - new Date(r.created_at).getTime();
      staleActive.push({
        intent_id: r.intent_id,
        agent_id: r.agent_id,
        status: 'ACTIVE',
        rationale: r.rationale,
        target_files: parseJsonList(r.files_json),
        workspace_path: r.workspace_path,
        created_at: r.created_at,
        age_hours: Math.round(ageMs / 3600000 * 10) / 10,
      });
    }
  } catch { /* file_locks table may not exist on very old schemas */ }

  const total = unverified.length + staleActive.length;
  return { ok: true, unverified, stale_active: staleActive, count: total };
}

/**
 * Transition a PENDING intent to SUCCESS or FAILED.
 *
 * Only operates on PENDING intents — attempting to verify an ACTIVE, SUCCESS,
 * or FAILED intent returns ok=false with a descriptive error so the agent knows
 * exactly what went wrong.
 */
export function markVerified(
  db: DatabaseSync,
  params: MarkVerifiedParams,
): MarkVerifiedResult {
  const { agentId = 'agent', allPending = false, workspacePath, message } = params;
  const intentId = params.intentId ?? '';
  const status = params.status ?? 'SUCCESS';

  // --all-pending: verify every PENDING intent for this agent/workspace at once
  if (allPending) {
    const where: string[] = ["status = 'PENDING'", 'agent_id = ?'];
    const binds: (string | number)[] = [agentId];
    if (workspacePath) { where.push('workspace_path = ?'); binds.push(workspacePath); }
    const rows = db.prepare(
      `SELECT intent_id FROM agent_intents WHERE ${where.join(' AND ')}`
    ).all(...binds) as unknown as Array<{ intent_id: string }>;
    const now = utcNow();
    const ids: string[] = [];
    for (const row of rows) {
      db.prepare(
        "UPDATE agent_intents SET status = ?, updated_at = ? WHERE intent_id = ? AND status = 'PENDING'"
      ).run(status, now, row.intent_id);
      ids.push(row.intent_id);
      if (message) {
        try {
          db.prepare(
            `INSERT INTO intent_events(event_id, intent_id, agent_id, event_type, message, created_at)
             VALUES (?, ?, ?, 'VERIFIED', ?, ?)`
          ).run('evt_' + randomUUID().replace(/-/g, ''), row.intent_id, agentId, message, now);
        } catch { /* intent_events may not exist */ }
      }
    }
    // VER-1: Return null for intent_id — no single intent applies in allPending batch mode.
    return { ok: true, intent_id: null, intent_ids: ids, count: ids.length, status: status as IntentStatus, updated_at: now };
  }

  if (!intentId) {
    return { ok: false, error: '--intent-id is required (or use --all-pending)', intent_id: null };
  }

  if (!VALID_VERIFY_STATUSES.has(status)) {
    return {
      ok: false,
      error: `invalid status "${status}" — must be SUCCESS or FAILED`,
      intent_id: intentId,
    };
  }

  const now = utcNow();
  const result = db.prepare(
    "UPDATE agent_intents SET status = ?, updated_at = ? WHERE intent_id = ? AND agent_id = ? AND status = 'PENDING'",
  ).run(status, now, intentId, agentId) as { changes: number };

  if (result.changes === 0) {
    // Distinguish: no such intent / wrong agent / not PENDING
    const row = db.prepare(
      'SELECT agent_id, status FROM agent_intents WHERE intent_id = ?',
    ).get(intentId) as unknown as AgentStatusRow | undefined;

    if (!row) {
      return {
        ok: false,
        error: `no intent found with intent_id=${intentId}`,
        intent_id: intentId,
      };
    }
    if (row.agent_id !== agentId) {
      return {
        ok: false,
        error: `intent ${intentId} belongs to agent "${row.agent_id}", not "${agentId}"`,
        intent_id: intentId,
      };
    }
    return {
      ok: false,
      error: `intent ${intentId} has status "${row.status}" — only PENDING intents can be verified`,
      intent_id: intentId,
    };
  }

  return {
    ok: true,
    intent_id: intentId,
    status: status as IntentStatus,
    updated_at: now,
  };
}
