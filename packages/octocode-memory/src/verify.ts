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

export interface AuditUnverifiedResult {
  ok: true;
  unverified: UnverifiedIntent[];
  count: number;
}

export interface AuditUnverifiedParams {
  agentId?: string | null;
  workspacePath?: string | null;
}

export type VerifyStatus = 'SUCCESS' | 'FAILED';

export interface MarkVerifiedParams {
  intentId: string;
  agentId?: string;
  status?: VerifyStatus;
}

export interface MarkVerifiedOk {
  ok: true;
  intent_id: string;
  status: IntentStatus;
  updated_at: string;
}

export interface MarkVerifiedErr {
  ok: false;
  error: string;
  intent_id: string;
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

  return { ok: true, unverified, count: unverified.length };
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
  const { intentId, agentId = 'agent' } = params;
  const status = params.status ?? 'SUCCESS';

  if (!intentId) {
    return { ok: false, error: '--intent-id is required', intent_id: intentId };
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
