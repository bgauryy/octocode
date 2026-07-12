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
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';
import { AuditUnverifiedParams, AuditUnverifiedResult, IntentDbRow, StaleActiveIntent, targetFilesForRuns, UnverifiedIntent } from './verify-shared.js';

// ─── Exports ──────────────────────────────────────────────────────────────────

/**
 * Return all run rows with status='PENDING', optionally scoped to an
 * agent and/or workspace. A non-zero count means the Stop hook should block
 * conclude.
 */
export function auditUnverified(
  db: DatabaseSync,
  params: AuditUnverifiedParams = {},
): AuditUnverifiedResult {
  // Normalize (git-root + symlink canonicalized) so this matches the same
  // scope key that preFlightIntent/releaseFileLock wrote, regardless of
  // symlinks or whether the workspace became a git repo after the lock
  // was recorded — see canonicalizePath in git.ts.
  const workspacePath = params.workspacePath ? normalizeWorkspacePath(params.workspacePath, params.workspacePath) : null;
  const where: string[] = ["status = 'PENDING'"];
  const binds: (string | number)[] = [];
  let ageCutoff: string | null = null;
  if (params.olderThanDays != null) {
    if (!Number.isFinite(params.olderThanDays) || params.olderThanDays < 1) {
      throw new Error('olderThanDays must be a finite number >= 1');
    }
    ageCutoff = new Date(Date.now() - Math.floor(params.olderThanDays) * 86400000).toISOString();
    where.push('updated_at < ?');
    binds.push(ageCutoff);
  }
  if (params.origins?.length) {
    const origins = [...new Set(params.origins)];
    if (origins.some((origin) => !['TASK', 'WORK', 'HOOK'].includes(origin))) {
      throw new Error('origins must contain only TASK, WORK, or HOOK');
    }
    where.push(`origin IN (${origins.map(() => '?').join(',')})`);
    binds.push(...origins);
  }
  let before: string | null = null;
  if (params.before) {
    const parsed = new Date(params.before);
    if (Number.isNaN(parsed.getTime())) throw new Error('before must be a valid ISO timestamp');
    before = parsed.toISOString();
    where.push('created_at < ?');
    binds.push(before);
  }

  if (params.agentId) {
    where.push('agent_id = ?');
    binds.push(params.agentId);
  }
  if (workspacePath) {
    where.push('workspace_path = ?');
    binds.push(workspacePath);
  }
  const artifact = normalizeArtifact(params.artifact);
  if (artifact) {
    where.push('(artifact = ? OR artifact IS NULL)');
    binds.push(artifact);
  }

  const rows = db.prepare(
    `SELECT run_id, agent_id, status, test_plan, context_ref, rationale, workspace_path, artifact, created_at
     FROM task_runs
     WHERE ${where.join(' AND ')}
     ORDER BY created_at ASC`,
  ).all(...binds) as unknown as IntentDbRow[];

  const unverifiedFiles = targetFilesForRuns(db, rows.map((r) => r.run_id));
  const unverified: UnverifiedIntent[] = rows.map(r => ({
    run_id: r.run_id,
    agent_id: r.agent_id,
    status: r.status,
    test_plan: r.test_plan,
    context_ref: r.context_ref,
    rationale: r.rationale,
    target_files: unverifiedFiles.get(r.run_id) ?? [],
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    created_at: r.created_at,
  }));

  // VER-2: Detect standalone ACTIVE runs whose file presence expired, plus task
  // runs whose claim lease and file presence both expired. Ordinary work may
  // validly have no lock, so lock absence is never verification debt.
  const staleActive: StaleActiveIntent[] = [];
  try {
    const nowIso = utcNow();
    const staleWhere: string[] = [
      "ai.status = 'ACTIVE'",
      'EXISTS (SELECT 1 FROM run_files any_rf WHERE any_rf.run_id = ai.run_id)',
      `NOT EXISTS (
        SELECT 1 FROM run_files active_rf
        WHERE active_rf.run_id = ai.run_id AND active_rf.ended_at IS NULL
          AND active_rf.expires_at > ?
      )`,
      `NOT EXISTS (
        SELECT 1 FROM task_claims tc
        WHERE tc.run_id = ai.run_id AND tc.expires_at > ?
      )`,
    ];
    const staleBinds: (string | number)[] = [nowIso, nowIso];
    if (params.agentId) { staleWhere.push('ai.agent_id = ?'); staleBinds.push(params.agentId); }
    if (workspacePath) { staleWhere.push('ai.workspace_path = ?'); staleBinds.push(workspacePath); }
    if (artifact) { staleWhere.push('(ai.artifact = ? OR ai.artifact IS NULL)'); staleBinds.push(artifact); }
    if (ageCutoff) { staleWhere.push('ai.updated_at < ?'); staleBinds.push(ageCutoff); }
    if (params.origins?.length) {
      const origins = [...new Set(params.origins)];
      staleWhere.push(`ai.origin IN (${origins.map(() => '?').join(',')})`);
      staleBinds.push(...origins);
    }
    if (before) { staleWhere.push('ai.created_at < ?'); staleBinds.push(before); }

    const staleRows = db.prepare(
      `SELECT ai.run_id, ai.agent_id, ai.rationale, ai.context_ref, ai.workspace_path, ai.artifact, ai.created_at
       FROM task_runs ai
       WHERE ${staleWhere.join(' AND ')}
       ORDER BY ai.created_at ASC`
    ).all(...staleBinds) as unknown as IntentDbRow[];

    const staleFiles = targetFilesForRuns(db, staleRows.map((r) => r.run_id));
    for (const r of staleRows) {
      const ageMs = Date.now() - new Date(r.created_at).getTime();
      staleActive.push({
        run_id: r.run_id,
        agent_id: r.agent_id,
        status: 'ACTIVE',
        rationale: r.rationale,
        context_ref: r.context_ref,
        target_files: staleFiles.get(r.run_id) ?? [],
        workspace_path: r.workspace_path,
        artifact: r.artifact,
        created_at: r.created_at,
        age_hours: Math.round(ageMs / 3600000 * 10) / 10,
      });
    }
  } catch (e) { if (!(e instanceof Error && e.message.includes('no such table'))) throw e; }

  const total = unverified.length + staleActive.length;
  return { ok: true, unverified, stale_active: staleActive, count: total };
}
