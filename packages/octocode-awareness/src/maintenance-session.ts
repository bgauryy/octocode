import { randomUUID } from 'node:crypto';
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { evictExpiredLocks } from './db.js';
import { canonicalizePath, fillScope, normalizeWorkspacePath } from './git.js';
import { normalizeArtifact, parseJsonList, utcNow } from './helpers.js';
import { boundedMs, compactText, DEFAULT_RETRY_MS, DEFAULT_WAIT_MS, listSummary, MAX_RETRY_MS, MAX_WAIT_MS, SESSION_CAPTURE_FILE_LIMIT, SESSION_CAPTURE_RUN_DETAIL_LIMIT, SESSION_CAPTURE_RUN_FILE_LIMIT, SessionCaptureResult, WaitForLockResult } from './maintenance-stale.js';
import { gitDirtyFiles } from './maintenance-briefing.js';

/** REAL: Capture unresolved session state as an open handoff refinement. */
export function sessionCapture(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): SessionCaptureResult {
  const agentId = String(params.agent_id ?? params.agentId ?? 'agent');
  const reason = params.reason ? String(params.reason) : null;
  const workspaceInput = (params.workspace ?? params.workspace_path ?? params.workspacePath) as string | null | undefined;
  const rawWorkspacePath = typeof workspaceInput === 'string' && workspaceInput.trim()
    ? resolve(workspaceInput.trim())
    : null;
  const scope = fillScope(
    {
      workspace_path: rawWorkspacePath,
      artifact: normalizeArtifact(params.artifact),
      repo: (params.repo as string | null | undefined) ?? null,
      ref: (params.ref as string | null | undefined) ?? null,
    },
    (params.cwd as string | undefined) ?? process.cwd(),
  );
  const workspacePath = scope.workspace_path ?? rawWorkspacePath ?? process.cwd();
  const runWorkspaceCandidates = [...new Set([workspacePath, rawWorkspacePath].filter((value): value is string => Boolean(value)))];
  const artifact = scope.artifact;
  const workspacePlaceholders = runWorkspaceCandidates.map(() => '?').join(',');

  const runRows = db.prepare(
    `SELECT tr.run_id, tr.rationale, tr.test_plan, tr.context_ref, tr.status, tr.created_at, tr.updated_at,
            COALESCE((SELECT json_group_array(rf.file_path)
              FROM run_files rf WHERE rf.run_id = tr.run_id), '[]') AS files_json
     FROM task_runs tr
     WHERE tr.agent_id = ?
       AND status IN ('ACTIVE', 'PENDING')
       AND (workspace_path IN (${workspacePlaceholders}) OR workspace_path IS NULL)
       AND (? IS NULL OR artifact = ? OR artifact IS NULL)
     ORDER BY updated_at DESC, created_at DESC
     LIMIT 20`
  ).all(agentId, ...runWorkspaceCandidates, artifact, artifact) as Array<{
    run_id: string;
    rationale: string;
    test_plan: string;
    context_ref: string | null;
    status: string;
    files_json: string;
    created_at: string;
    updated_at: string;
  }>;

  const files = [...new Set(runRows.flatMap(row => parseJsonList(row.files_json)))];
  const dirtyFiles = gitDirtyFiles(workspacePath);
  const activeRuns = runRows.filter(row => row.status === 'ACTIVE').length;
  const pendingRuns = runRows.filter(row => row.status === 'PENDING').length;

  // Count memories with low novelty (< 0.2) that are candidates for supersede/consolidation.
  // This is a hint to the agent that memory_digest or manual supersede may be overdue.
  let consolidationOpportunities = 0;
  try {
    const cConds: string[] = ["novelty_score IS NOT NULL", "novelty_score < 0.2", "state = 'ACTIVE'"];
    const cBinds: (string | number)[] = [];
    if (workspacePath) { cConds.push('(workspace_path = ? OR workspace_path IS NULL)'); cBinds.push(workspacePath); }
    if (artifact) { cConds.push('(artifact = ? OR artifact IS NULL)'); cBinds.push(artifact); }
    consolidationOpportunities = (db.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE ${cConds.join(' AND ')}`
    ).get(...cBinds) as { c: number }).c;
  } catch { /* non-fatal */ }

  if (runRows.length === 0 && dirtyFiles.length === 0) {
    return {
      ok: true,
      captured: false,
      refinement_id: null,
      pending_runs: 0,
      active_runs: 0,
      files: [],
      dirty_files: [],
      reason,
      consolidation_opportunities: consolidationOpportunities,
    };
  }

  const now = utcNow();
  const refinementId = 'ref_' + randomUUID().replace(/-/g, '');
  const allCapturedFiles = [...new Set([...files, ...dirtyFiles])];
  const capturedFiles = allCapturedFiles.slice(0, SESSION_CAPTURE_FILE_LIMIT);
  const capturedDirtyFiles = dirtyFiles.slice(0, SESSION_CAPTURE_FILE_LIMIT);
  const statusSummary = runRows.slice(0, SESSION_CAPTURE_RUN_DETAIL_LIMIT).map(row => {
    const rowFiles = parseJsonList(row.files_json);
    const shownFiles = rowFiles.slice(0, SESSION_CAPTURE_RUN_FILE_LIMIT);
    const omittedFiles = rowFiles.length - shownFiles.length;
    const fileSuffix = rowFiles.length > 0
      ? ` files=${shownFiles.join(', ')}${omittedFiles > 0 ? ` (+${omittedFiles} more)` : ''}`
      : '';
    const planSuffix = row.context_ref ? ` plan=${row.context_ref}` : '';
    return `${row.status} ${row.run_id}: ${compactText(row.rationale)}; verify=${compactText(row.test_plan)}${planSuffix}${fileSuffix}`;
  });
  const omittedRunDetails = runRows.length - statusSummary.length;
  const reasoning = [
    `Session capture for ${agentId}${reason ? ` (${reason})` : ''}.`,
    `Unresolved runs: ${runRows.length} (${activeRuns} active, ${pendingRuns} pending).`,
    listSummary('Dirty files', dirtyFiles),
    statusSummary.length > 0
      ? `Run details: ${statusSummary.join(' | ')}${omittedRunDetails > 0 ? ` | ${omittedRunDetails} more runs omitted` : ''}`
      : null,
  ].filter(Boolean).join(' ');
  const remember = [
    `Review session handoff for ${agentId}: ${activeRuns} active and ${pendingRuns} pending runs remain.`,
    listSummary('Touched files', allCapturedFiles),
    dirtyFiles.length > 0 ? 'Check dirty git state before continuing.' : null,
    pendingRuns > 0 ? 'Run the recorded verification before claiming completion.' : null,
  ].filter(Boolean).join(' ');

  const existing = db.prepare(
    `SELECT refinement_id FROM refinements
      WHERE agent_id = ? AND workspace_path = ? AND artifact IS ? AND repo IS ? AND ref IS ?
        AND quality = 'handoff' AND state IN ('open', 'ongoing')
        AND files_json = ? AND reasoning = ? AND remember = ?
      ORDER BY datetime(updated_at) DESC LIMIT 1`,
  ).get(
    agentId,
    workspacePath,
    artifact,
    scope.repo,
    scope.ref,
    JSON.stringify(capturedFiles),
    reasoning,
    remember,
  ) as { refinement_id: string } | undefined;
  if (existing) {
    return {
      ok: true,
      captured: false,
      deduplicated: true,
      refinement_id: existing.refinement_id,
      pending_runs: pendingRuns,
      active_runs: activeRuns,
      files: capturedFiles,
      dirty_files: capturedDirtyFiles,
      file_count: allCapturedFiles.length,
      dirty_file_count: dirtyFiles.length,
      omitted_files: Math.max(0, allCapturedFiles.length - capturedFiles.length),
      omitted_dirty_files: Math.max(0, dirtyFiles.length - capturedDirtyFiles.length),
      reason,
      consolidation_opportunities: consolidationOpportunities,
    };
  }

  db.prepare(
    `INSERT INTO refinements (
       refinement_id, agent_id, workspace_path, repo, ref,
       artifact, files_json, reasoning, remember, quality, state, created_at, updated_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'handoff', 'open', ?, ?)`
  ).run(
    refinementId,
    agentId,
    workspacePath,
    scope.repo,
    scope.ref,
    artifact,
    JSON.stringify(capturedFiles),
    reasoning,
    remember,
    now,
    now,
  );

  return {
    ok: true,
    captured: true,
    refinement_id: refinementId,
    pending_runs: pendingRuns,
    active_runs: activeRuns,
    files: capturedFiles,
    dirty_files: capturedDirtyFiles,
    file_count: allCapturedFiles.length,
    dirty_file_count: dirtyFiles.length,
    omitted_files: Math.max(0, allCapturedFiles.length - capturedFiles.length),
    omitted_dirty_files: Math.max(0, dirtyFiles.length - capturedDirtyFiles.length),
    reason,
    consolidation_opportunities: consolidationOpportunities,
  };
}

/**
 * Poll until target file locks clear, bounded by waitMs.
 * Retries every retryIntervalMs using Atomics.wait (no busy-spin, no CPU waste).
 */
export function waitForLock(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): WaitForLockResult {
  const targetFiles = Array.isArray(params.target_files) ? params.target_files as string[] :
    Array.isArray(params.targetFiles) ? params.targetFiles as string[] : [];
  const agentId = (params.agent_id ?? params.agentId) as string | undefined ?? 'agent';
  const rawWorkspacePath = typeof params.workspace === 'string' ? params.workspace :
    typeof params.workspace_path === 'string' ? params.workspace_path :
      typeof params.workspacePath === 'string' ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const waitMs = boundedMs(params.wait_ms ?? params.waitMs, DEFAULT_WAIT_MS, 0, MAX_WAIT_MS);
  const retryMs = boundedMs(params.retry_interval_ms ?? params.retryIntervalMs, DEFAULT_RETRY_MS, 1, MAX_RETRY_MS);
  const start = Date.now();

  if (targetFiles.length === 0) {
    return { ok: true, waited_ms: 0, lock_free: true };
  }
  const root = rawWorkspacePath ? resolve(rawWorkspacePath) : process.cwd();
  const absTargetFiles = targetFiles.map((file) => canonicalizePath(isAbsolute(file) ? resolve(file) : resolve(root, file)));

  // FIX #11 (P2): Hoist db.prepare() outside the closure so the statement is compiled
  // once and reused on each poll iteration instead of being re-compiled every loop tick.
  // Also hoist the SharedArrayBuffer/Int32Array allocation before the loop so the same
  // buffer is reused across all Atomics.wait calls.
  const ph = absTargetFiles.map(() => '?').join(',');
  const scopeClauses: string[] = [];
  const scopeBinds: string[] = [];
  if (workspacePath) { scopeClauses.push('AND ai.workspace_path = ?'); scopeBinds.push(workspacePath); }
  if (artifact) { scopeClauses.push('AND (ai.artifact = ? OR ai.artifact IS NULL)'); scopeBinds.push(artifact); }
  const lockStmt = db.prepare(
    `SELECT fl.file_path, ai.agent_id, fl.expires_at
     FROM locks fl
     JOIN task_runs ai ON ai.run_id = fl.run_id
     WHERE fl.file_path IN (${ph})
       AND ai.agent_id <> ?
       AND ai.status = 'ACTIVE'
       ${scopeClauses.join('\n       ')}
       AND (fl.expires_at IS NULL OR fl.expires_at > ?)`
  );
  // Single Int32Array reused across all Atomics.wait calls — avoids repeated allocation.
  const sleepBuf = new Int32Array(new SharedArrayBuffer(4));

  type LockRow = { file_path: string; agent_id: string; expires_at: string | null };

  const checkLocks = (): LockRow[] => {
    evictExpiredLocks(db);
    const now = new Date().toISOString();
    return lockStmt.all(...absTargetFiles, agentId, ...scopeBinds, now) as unknown as LockRow[];
  };

  // Synchronous sleep via Atomics.wait on the pre-allocated buffer.
  // Yields the thread to the OS for the full duration instead of busy-spinning,
  // eliminating the 100% CPU usage the previous spin loop caused during lock waits.
  // SharedArrayBuffer is unconditionally available in Node.js (no COOP/COEP headers needed).
  function sleepMs(ms: number): void {
    Atomics.wait(sleepBuf, 0, 0, ms);
  }

  let conflicts = checkLocks();
  const waited = () => Date.now() - start;

  while (conflicts.length > 0 && waited() < waitMs) {
    sleepMs(Math.min(retryMs, waitMs - waited()));
    conflicts = checkLocks();
  }

  const elapsed = waited();
  if (conflicts.length === 0) {
    return { ok: true, waited_ms: elapsed, lock_free: true };
  }
  return {
    ok: true,
    waited_ms: elapsed,
    lock_free: false,
    conflicts: conflicts.map(c => ({ file_path: c.file_path, agent_id: c.agent_id, expires_at: c.expires_at })),
  };
}
