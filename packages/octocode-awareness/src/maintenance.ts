/**
 * maintenance.ts — Background maintenance, smart briefing, and session lifecycle operations.
 *
 * pruneStale:          deletes expired file locks, sets affected intents to PENDING.
 * notifyGet:           returns a smart workspace briefing (top memories + weakness + refinements).
 * digest:              archives expired memories, prunes stale rows/locks, rebuilds FTS.
 * getWorkspaceStatus:  returns active locks, agents, and memory store stats.
 * exportMemoryDoc:     queries all active memories and returns a markdown report string.
 * exportHarness:       returns top recurring lessons as an AGENTS.md block.
 * sessionCapture:      records unresolved session work as an open handoff refinement.
 * waitForLock:         polls active exclusive locks until clear or timeout.
 */

import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { hasFts, rebuildFts, evictExpiredLocks } from './db.js';
import { fillScope } from './git.js';
import { parseJsonList, utcNow } from './helpers.js';
import { getNotifications } from './notifications.js';

export interface PruneStaleResult {
  pruned_locks: number;
  updated_intents: number;
  dry_run?: true;
  would_prune?: number;
}

export interface NotifyGetResult {
  ok: true;
  count: 0;
  notifications: [];
  schema_version: 1;
}

export interface SessionCaptureResult {
  ok: true;
  captured: boolean;
  refinement_id: string | null;
  pending_intents: number;
  active_intents: number;
  files: string[];
  dirty_files: string[];
  reason: string | null;
  consolidation_opportunities: number; // memories with novelty_score < 0.2 (candidates for supersede)
}

export interface WaitForLockResult {
  ok: true;
  waited_ms: number;
  lock_free: boolean;
  conflicts?: Array<{ file_path: string; agent_id: string; expires_at: string | null }>;
}

/** REAL: Delete expired file locks and set parent intents to PENDING. */
export function pruneStale(db: DatabaseSync, params: Record<string, unknown> = {}): PruneStaleResult {
  const dryRun = Boolean(params.dry_run ?? params.dryRun);
  const expiredOnly = Boolean(params.expired_only ?? params.expiredOnly);
  const olderThanMinutes = params.older_than_minutes != null ? Number(params.older_than_minutes) :
    params.olderThanMinutes != null ? Number(params.olderThanMinutes) : null;
  const agentId = typeof params.agent_id === 'string' ? params.agent_id :
    typeof params.agentId === 'string' ? params.agentId : null;
  const rawTarget = params.target_file ?? params.targetFile;
  const targetFiles = (Array.isArray(rawTarget) ? rawTarget : rawTarget != null ? [rawTarget] : [])
    .map(String).filter(Boolean);
  const now = utcNow();
  // Age cutoff: locks older than N minutes are considered stale even if not expired.
  const ageCutoff = olderThanMinutes != null && !expiredOnly
    ? new Date(Date.now() - olderThanMinutes * 60000).toISOString()
    : null;

  // Selection must be identical for dry-run and real prune, so previews are honest.
  const conditions: string[] = [];
  const binds: string[] = [];
  const staleClauses = ['(expires_at IS NOT NULL AND expires_at < ?)'];
  binds.push(now);
  if (ageCutoff) {
    staleClauses.push('(acquired_at < ?)');
    binds.push(ageCutoff);
  }
  conditions.push(`(${staleClauses.join(' OR ')})`);
  if (agentId) { conditions.push('agent_id = ?'); binds.push(agentId); }
  if (targetFiles.length > 0) {
    conditions.push(`file_path IN (${targetFiles.map(() => '?').join(',')})`);
    binds.push(...targetFiles);
  }
  const where = conditions.join(' AND ');

  let staleLocks: Array<{ lock_id: string; intent_id: string }> = [];
  try {
    staleLocks = db.prepare(
      `SELECT lock_id, intent_id FROM file_locks WHERE ${where}`
    ).all(...binds) as Array<{ lock_id: string; intent_id: string }>;
  } catch { /* ignore — table may be mid-migration */ }

  if (dryRun) {
    return { pruned_locks: 0, updated_intents: 0, dry_run: true, would_prune: staleLocks.length };
  }
  if (staleLocks.length === 0) {
    return { pruned_locks: 0, updated_intents: 0 };
  }

  db.exec('BEGIN');
  try {
    const ph = staleLocks.map(() => '?').join(',');
    db.prepare(`DELETE FROM file_locks WHERE lock_id IN (${ph})`).run(...staleLocks.map(l => l.lock_id));
    db.exec('COMMIT');
  } catch (e) {
    try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
    throw e;
  }

  const affectedIntentIds = [...new Set(staleLocks.map(l => l.intent_id))];
  let updatedIntents = 0;
  for (const iid of affectedIntentIds) {
    const remaining = db.prepare('SELECT 1 FROM file_locks WHERE intent_id = ? LIMIT 1').get(iid);
    if (!remaining) {
      const r = db.prepare(
        "UPDATE agent_intents SET status = 'PENDING', updated_at = ? WHERE intent_id = ? AND status = 'ACTIVE'"
      ).run(now, iid) as { changes: number };
      if (r.changes) updatedIntents++;
    }
  }

  return { pruned_locks: staleLocks.length, updated_intents: updatedIntents };
}

// ─── Smart briefing ─────────────────────────────────────────────────────────

export interface BriefItem {
  kind: 'memory' | 'weakness' | 'refinement' | 'notification';
  text: string;
  importance?: number;
}

export interface NotifyGetBriefResult {
  ok: true;
  count: number;
  notifications: BriefItem[];
  additionalContext?: string;  // set when format:hook is requested
  schema_version: 1;
}

function openRefinementCount(
  db: DatabaseSync,
  params: { workspacePath?: string | null; repo?: string | null; cwd?: string; includeHandoffs?: boolean } = {},
): number {
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, repo: params.repo ?? null },
    params.cwd ?? process.cwd(),
  );
  const queryParams: (string | number)[] = [];
  let sql = "SELECT COUNT(*) AS c FROM refinements WHERE state IN ('open','ongoing')";
  if (!params.includeHandoffs) sql += " AND quality <> 'handoff'";
  if (scope.repo) {
    sql += ' AND (repo = ? OR repo IS NULL)';
    queryParams.push(scope.repo);
  } else if (scope.workspace_path) {
    sql += ' AND (workspace_path = ? OR workspace_path IS NULL)';
    queryParams.push(scope.workspace_path);
  }
  return (db.prepare(sql).get(...queryParams) as { c: number }).c;
}

/**
 * Returns a smart workspace briefing instead of an empty inbox.
 * — Unread agent notifications addressed to this agent (or broadcasts)
 * — Top memories (GOTCHA/BUG/DECISION, importance ≥6, scoped to workspace)
 * — Top mine-weakness cluster (failure_signature with count ≥2)
 * — Count of open refinements
 * Designed to be called by notify-deliver.sh before every user prompt.
 */
// MAINT-3: Briefing label allowlist as a named constant — previously buried inside
// notifyGet making it invisible and hard to tune.
const BRIEFING_LABELS = ['GOTCHA', 'BUG', 'DECISION', 'IMPROVEMENT', 'ARCHITECTURE', 'SECURITY'] as const;

export function notifyGet(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): NotifyGetResult | NotifyGetBriefResult {
  const wsPath = (params.workspace as string | undefined) ?? null;
  const format  = (params.format as string | undefined) ?? 'json';
  const agentId = String(params.agent_id ?? params.agentId ?? 'agent');
  // MAINT-2: Use the cwd from params (workspace path) not process.cwd() which
  // would be the shell directory, potentially different from the actual workspace.
  const notifyCwd = wsPath ?? (params.cwd as string | undefined) ?? process.cwd();

  const items: BriefItem[] = [];

  // Each query is isolated — one failure does not wipe the others.

  // 0. Unread notifications for this agent. Hook fetch does not ack; agents call agent_signal action:'ack' after acting.
  try {
    const inbox = getNotifications(db, {
      agentId,
      workspacePath: wsPath,
      unreadOnly: true,
      markRead: false,
      limit: 5,
      cwd: notifyCwd,
    });
    for (const n of inbox.notifications) {
      const target = n.to_agent ? `to ${n.to_agent}` : 'broadcast';
      const fileSuffix = n.files.length > 0 ? ` files=${n.files.join(', ')}` : '';
      const bodySuffix = n.body ? ` — ${n.body.slice(0, 120)}` : '';
      items.push({
        kind: 'notification',
        text: `📨 ${n.kind} from ${n.from_agent} (${target}): ${n.subject}${bodySuffix}${fileSuffix}`,
        importance: n.importance,
      });
    }
  } catch { /* skip notifications on error */ }

  // 1a. OVERRIDE memories — always surfaced regardless of importance (they contradict model defaults)
  try {
    type MemRow = { memory_id: string; observation: string; importance_score: number };
    const overrideConds: string[] = ["state = 'ACTIVE'", "label = 'OVERRIDE'"];
    const overrideBinds: (string | number)[] = [];
    if (wsPath) { overrideConds.push('(workspace_path = ? OR workspace_path IS NULL)'); overrideBinds.push(wsPath); }
    const overrideRows = db.prepare(
      `SELECT memory_id, observation, importance_score
       FROM agent_memories
       WHERE ${overrideConds.join(' AND ')}
       ORDER BY importance_score DESC, last_accessed_at DESC
       LIMIT 2`
    ).all(...overrideBinds) as unknown as MemRow[];
    for (const m of overrideRows) {
      items.push({
        kind: 'memory',
        text: `OVERRIDE(${m.importance_score}): ${m.observation.slice(0, 120)}`,
        importance: m.importance_score,
      });
    }
  } catch { /* skip this section on error */ }

  // 1b. Top actionable memories for this workspace (EXPERIENCE/reflections excluded)
  try {
    type MemRow = { memory_id: string; observation: string; label: string; importance_score: number };
    const conditions: string[] = ["state = 'ACTIVE'", "importance_score >= 6",
      `label IN (${BRIEFING_LABELS.map(() => '?').join(',')})`];
    // BRIEFING_LABELS binds must be pushed before wsPath so they match the IN(?) order in WHERE
    const bindParams: (string | number)[] = [...BRIEFING_LABELS];
    if (wsPath) { conditions.push('(workspace_path = ? OR workspace_path IS NULL)'); bindParams.push(wsPath); }
    const memRows = db.prepare(
      `SELECT memory_id, observation, label, importance_score
       FROM agent_memories
       WHERE ${conditions.join(' AND ')}
       ORDER BY importance_score DESC, last_accessed_at DESC
       LIMIT 3`
    ).all(...bindParams) as unknown as MemRow[];
    for (const m of memRows) {
      items.push({
        kind: 'memory',
        text: `${m.label}(${m.importance_score}): ${m.observation.slice(0, 120)}`,
        importance: m.importance_score,
      });
    }
  } catch { /* skip this section on error */ }

  // 2. Top mine-weakness cluster
  try {
    type WkRow = { failure_signature: string; freq: number; avg_imp: number };
    const wkConditions = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
    const wkParams: (string | number)[] = [];
    if (wsPath) { wkConditions.push('(workspace_path = ? OR workspace_path IS NULL)'); wkParams.push(wsPath); }
    const topWk = db.prepare(
      `SELECT failure_signature, count(*) AS freq, avg(importance_score) AS avg_imp
       FROM agent_memories
       WHERE ${wkConditions.join(' AND ')}
       GROUP BY failure_signature HAVING freq >= 2
       ORDER BY freq * avg_imp DESC LIMIT 1`
    ).get(...wkParams) as unknown as WkRow | undefined;
    if (topWk) {
      items.push({
        kind: 'weakness',
        text: `⚠️ Recurring: ${topWk.failure_signature} (${topWk.freq}x, avg imp ${Math.round(topWk.avg_imp)})`,
      });
    }
  } catch { /* skip this section on error */ }

  // 3. Open repo-fix refinements count (session handoffs are excluded by default)
  try {
    const refCount = openRefinementCount(db, { workspacePath: wsPath, cwd: notifyCwd });
    if (refCount > 0) {
      items.push({ kind: 'refinement', text: `📋 ${refCount} open refinement(s) pending` });
    }
  } catch { /* skip this section on error */ }

  if (items.length === 0) {
    return { ok: true, count: 0, notifications: [], schema_version: 1 };
  }

  const result: NotifyGetBriefResult = {
    ok: true,
    count: items.length,
    notifications: items,
    schema_version: 1,
  };

  // Hook format: wrap top items as additionalContext for pi injection
  if (format === 'hook') {
    const lines = [
      `🧠 Memory brief (${items.length}):`,
      ...items.map(i => `  • ${i.text}`),
    ];
    result.additionalContext = lines.join('\n');
  }

  return result;
}

function gitDirtyFiles(workspacePath: string | null): string[] {
  if (!workspacePath) return [];
  try {
    const result = spawnSync('git', ['-C', workspacePath, 'status', '--short'], {
      encoding: 'utf8',
      timeout: 5000,
    });
    if (result.status !== 0) return [];
    return String(result.stdout)
      .split('\n')
      .map(line => line.trim())
      .filter(Boolean)
      .map(line => line.slice(3).trim())
      .filter(Boolean);
  } catch {
    return [];
  }
}

/** REAL: Capture unresolved session state as an open handoff refinement. */
export function sessionCapture(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): SessionCaptureResult {
  const agentId = String(params.agent_id ?? params.agentId ?? 'agent');
  const reason = params.reason ? String(params.reason) : null;
  const scope = fillScope(
    {
      workspace_path: (params.workspace ?? params.workspace_path ?? params.workspacePath) as string | null | undefined,
      repo: (params.repo as string | null | undefined) ?? null,
      ref: (params.ref as string | null | undefined) ?? null,
    },
    (params.cwd as string | undefined) ?? process.cwd(),
  );
  const workspacePath = scope.workspace_path ?? process.cwd();

  const intentRows = db.prepare(
    `SELECT intent_id, rationale, test_plan, status, files_json, created_at, updated_at
     FROM agent_intents
     WHERE agent_id = ?
       AND status IN ('ACTIVE', 'PENDING')
       AND (workspace_path = ? OR workspace_path IS NULL)
     ORDER BY updated_at DESC, created_at DESC
     LIMIT 20`
  ).all(agentId, workspacePath) as Array<{
    intent_id: string;
    rationale: string;
    test_plan: string;
    status: string;
    files_json: string;
    created_at: string;
    updated_at: string;
  }>;

  const files = [...new Set(intentRows.flatMap(row => parseJsonList(row.files_json)))];
  const dirtyFiles = gitDirtyFiles(workspacePath);
  const activeIntents = intentRows.filter(row => row.status === 'ACTIVE').length;
  const pendingIntents = intentRows.filter(row => row.status === 'PENDING').length;

  // Count memories with low novelty (< 0.2) that are candidates for supersede/consolidation.
  // This is a hint to the agent that memory_digest or manual supersede may be overdue.
  let consolidationOpportunities = 0;
  try {
    const cConds: string[] = ["novelty_score IS NOT NULL", "novelty_score < 0.2", "state = 'ACTIVE'"];
    const cBinds: (string | number)[] = [];
    if (workspacePath) { cConds.push('(workspace_path = ? OR workspace_path IS NULL)'); cBinds.push(workspacePath); }
    consolidationOpportunities = (db.prepare(
      `SELECT COUNT(*) AS c FROM agent_memories WHERE ${cConds.join(' AND ')}`
    ).get(...cBinds) as { c: number }).c;
  } catch { /* non-fatal */ }

  if (intentRows.length === 0 && dirtyFiles.length === 0) {
    return {
      ok: true,
      captured: false,
      refinement_id: null,
      pending_intents: 0,
      active_intents: 0,
      files: [],
      dirty_files: [],
      reason,
      consolidation_opportunities: consolidationOpportunities,
    };
  }

  const now = utcNow();
  const refinementId = 'ref_' + randomUUID().replace(/-/g, '');
  const capturedFiles = [...new Set([...files, ...dirtyFiles])];
  const statusSummary = intentRows.map(row => {
    const rowFiles = parseJsonList(row.files_json);
    const fileSuffix = rowFiles.length > 0 ? ` files=${rowFiles.join(', ')}` : '';
    return `${row.status} ${row.intent_id}: ${row.rationale}; verify=${row.test_plan}${fileSuffix}`;
  });
  const reasoning = [
    `Session capture for ${agentId}${reason ? ` (${reason})` : ''}.`,
    `Unresolved intents: ${intentRows.length} (${activeIntents} active, ${pendingIntents} pending).`,
    dirtyFiles.length > 0 ? `Dirty files: ${dirtyFiles.join(', ')}.` : null,
    statusSummary.length > 0 ? `Intent details: ${statusSummary.join(' | ')}` : null,
  ].filter(Boolean).join(' ');
  const remember = [
    `Review session handoff for ${agentId}: ${activeIntents} active and ${pendingIntents} pending intents remain.`,
    capturedFiles.length > 0 ? `Touched files: ${capturedFiles.join(', ')}.` : null,
    dirtyFiles.length > 0 ? 'Check dirty git state before continuing.' : null,
    pendingIntents > 0 ? 'Run the recorded verification before claiming completion.' : null,
  ].filter(Boolean).join(' ');

  db.prepare(
    `INSERT INTO refinements (
       refinement_id, agent_id, workspace_path, repo, ref,
       files_json, reasoning, remember, quality, state, created_at, updated_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'handoff', 'open', ?, ?)`
  ).run(
    refinementId,
    agentId,
    workspacePath,
    scope.repo,
    scope.ref,
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
    pending_intents: pendingIntents,
    active_intents: activeIntents,
    files: capturedFiles,
    dirty_files: dirtyFiles,
    reason,
    consolidation_opportunities: consolidationOpportunities,
  };
}

/**
 * Poll until target file locks clear, bounded by waitMs.
 * Retries every retryIntervalMs using a spin-sleep (MAINT-1: no SharedArrayBuffer dependency).
 */
export function waitForLock(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): WaitForLockResult {
  const targetFiles = Array.isArray(params.target_files) ? params.target_files as string[] :
    Array.isArray(params.targetFiles) ? params.targetFiles as string[] : [];
  const agentId = (params.agent_id ?? params.agentId) as string | undefined ?? 'agent';
  const waitMs = Number(params.wait_ms ?? params.waitMs ?? 60000);
  const retryMs = Number(params.retry_interval_ms ?? params.retryIntervalMs ?? 5000);
  // requestedLockType: EXCLUSIVE is blocked by any existing lock; SHARED is only blocked by EXCLUSIVE.
  const requestedLockType = String(
    params.requestedLockType ?? params.requested_lock_type ?? params.lockType ?? params.lock_type ?? 'EXCLUSIVE'
  ).toUpperCase();
  const start = Date.now();

  if (targetFiles.length === 0) {
    return { ok: true, waited_ms: 0, lock_free: true };
  }

  const checkLocks = () => {
    const now = new Date().toISOString();
    const ph = targetFiles.map(() => '?').join(',');
    type LockRow = { file_path: string; agent_id: string; expires_at: string | null };
    // EXCLUSIVE requests conflict with any lock type; SHARED requests only conflict with EXCLUSIVE locks.
    const lockTypeFilter = requestedLockType === 'EXCLUSIVE' ? '' : "AND fl.lock_type = 'EXCLUSIVE'";
    const locks = db.prepare(
      `SELECT fl.file_path, ai.agent_id, fl.expires_at
       FROM file_locks fl
       JOIN agent_intents ai ON ai.intent_id = fl.intent_id
       WHERE fl.file_path IN (${ph})
         AND ai.agent_id <> ?
         AND ai.status = 'ACTIVE'
         ${lockTypeFilter}
         AND (fl.expires_at IS NULL OR fl.expires_at > ?)`
    ).all(...targetFiles, agentId, now) as unknown as LockRow[];
    return locks;
  };

  let conflicts = checkLocks();
  const waited = () => Date.now() - start;

  // Synchronous sleep via Atomics.wait on a fresh SharedArrayBuffer.
  // This yields the thread to the OS for the full duration instead of busy-spinning,
  // eliminating the 100% CPU usage the previous spin loop caused during lock waits.
  // SharedArrayBuffer is unconditionally available in Node.js (no COOP/COEP headers needed).
  function sleepMs(ms: number): void {
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
  }

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

// ─── Background digest ────────────────────────────────────────────────────

export interface DigestResult {
  ok: true;
  archived_memories: number;   // valid_to expired (or would_archive in dry_run)
  pruned_old: number;          // SUPERSEDED older than retention_days
  pruned_locks: number;        // expired file locks
  pruned_refinements: number;  // old handoffs and done refinements
  fts_rebuilt: boolean;
  schema_version: 1;
  dry_run?: true;
  would_archive?: number;
  would_prune_old?: number;
  would_prune_locks?: number;
  would_prune_refinements?: number;
}

/**
 * Background consolidation — designed to run non-blocking every few hours.
 * 1. Archive memories whose valid_to has passed
 * 2. Hard-delete SUPERSEDED memories older than retention_days
 * 3. Prune expired file locks
 * 4. Prune old session handoffs and completed refinements
 * 5. Rebuild / optimize the FTS5 index
 */
export function digest(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): DigestResult {
  const retentionDays = Number(params.retention_days ?? 90);
  const handoffRetentionDays = Number(params.refinement_handoff_retention_days ?? params.refinementHandoffRetentionDays ?? 7);
  const doneRetentionDays = Number(params.refinement_done_retention_days ?? params.refinementDoneRetentionDays ?? 30);
  const now = new Date().toISOString();
  const cutoff = new Date(Date.now() - retentionDays * 86400000).toISOString();
  const handoffCutoff = new Date(Date.now() - handoffRetentionDays * 86400000).toISOString();
  const doneCutoff = new Date(Date.now() - doneRetentionDays * 86400000).toISOString();

  // MAINT-4: Use updated_at for both handoff and done retention.
  // The old code used created_at for handoffs and updated_at for done refinements.
  // A recently-updated handoff would be deleted if it was created long ago.
  // updated_at reflects the last meaningful activity and is the correct basis.
  const refinementRetentionSql = `SELECT COUNT(*) AS c FROM refinements
     WHERE (quality = 'handoff' AND updated_at < ?)
        OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?)`;

  // dry_run: count what would change without mutating anything
  if (params.dry_run) {
    const wouldArchive = (db.prepare(
      `SELECT COUNT(*) AS c FROM agent_memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'`
    ).get(now) as { c: number }).c;
    const wouldPruneOld = (db.prepare(
      `SELECT COUNT(*) AS c FROM agent_memories WHERE state = 'SUPERSEDED' AND updated_at < ?`
    ).get(cutoff) as { c: number }).c;
    const wouldPruneLocks = (db.prepare(
      `SELECT COUNT(*) AS c FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?`
    ).get(now) as { c: number }).c;
    const wouldPruneRefinements = (db.prepare(refinementRetentionSql)
      .get(handoffCutoff, doneCutoff) as { c: number }).c;
    return {
      ok: true,
      archived_memories: 0,
      pruned_old: 0,
      pruned_locks: 0,
      pruned_refinements: 0,
      fts_rebuilt: false,
      schema_version: 1,
      dry_run: true,
      would_archive: wouldArchive,
      would_prune_old: wouldPruneOld,
      would_prune_locks: wouldPruneLocks,
      would_prune_refinements: wouldPruneRefinements,
    };
  }

  // 1. Archive expired memories (valid_to < now)
  const archiveRes = db.prepare(
    `UPDATE agent_memories
     SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
     WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'`
  ).run(now, now, now) as { changes: number };

  // 2. Hard-delete old SUPERSEDED entries to keep the DB lean
  const deleteRes = db.prepare(
    `DELETE FROM agent_memories
     WHERE state = 'SUPERSEDED' AND updated_at < ?`
  ).run(cutoff) as { changes: number };

  // 3. Prune expired locks (reuse existing function)
  const { pruned_locks } = pruneStale(db, {});

  // 4. Prune old session handoffs and completed repo-fix refinements.
  // MAINT-4: Use updated_at for handoff retention (was created_at — see refinementRetentionSql above).
  const pruneRefinementsRes = db.prepare(
    `DELETE FROM refinements
     WHERE (quality = 'handoff' AND updated_at < ?)
        OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?)`
  ).run(handoffCutoff, doneCutoff) as { changes: number };

  // 5. Rebuild FTS5 index from the agent_memories source of truth.
  let ftsRebuilt = false;
  try {
    if (hasFts(db)) {
      rebuildFts(db);
      ftsRebuilt = true;
    }
  } catch {
    // FTS5 may not be available in all builds; non-fatal
  }

  return {
    ok: true,
    archived_memories: archiveRes.changes,
    pruned_old: deleteRes.changes,
    pruned_locks,
    pruned_refinements: pruneRefinementsRes.changes,
    fts_rebuilt: ftsRebuilt,
    schema_version: 1,
  };
}

// ─── Workspace status ──────────────────────────────────────────────────────

export interface WorkspaceLockEntry {
  file_path: string;
  agent_id: string;
  session_id: string | null;
  workspace_path: string | null;
  intent_id: string;
  lock_type: string;
  acquired_at: string;
  expires_at: string | null;
}

export interface WorkspaceStatusResult {
  ok: true;
  active_memories: number;
  pending_intents: number;
  active_intents: number;
  open_refinements: number;
  locks: WorkspaceLockEntry[];
  schema_version: 1;
}

/**
 * Returns a snapshot of active file locks, agent intents, and memory store stats.
 * Prunes expired locks first so stale entries don't pollute the view.
 */
export function getWorkspaceStatus(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): WorkspaceStatusResult {
  const wsPath = (params.workspace_path as string | undefined) ?? null;

  // ARCH-3: Delegate lock eviction to the shared evictExpiredLocks function
  // instead of duplicating the DELETE statement.
  evictExpiredLocks(db);

  const activeMemories = (db.prepare(
    `SELECT COUNT(*) AS c FROM agent_memories WHERE state = 'ACTIVE'`
  ).get() as { c: number }).c;

  const intentScope = wsPath ? ' AND workspace_path = ?' : '';
  const intentScopeParams = wsPath ? [wsPath] : [];

  const pendingIntents = (db.prepare(
    `SELECT COUNT(*) AS c FROM agent_intents WHERE status = 'PENDING'${intentScope}`
  ).get(...intentScopeParams) as { c: number }).c;

  const activeIntents = (db.prepare(
    `SELECT COUNT(*) AS c FROM agent_intents WHERE status = 'ACTIVE'${intentScope}`
  ).get(...intentScopeParams) as { c: number }).c;

  const openRefinements = openRefinementCount(db, {
    workspacePath: wsPath,
    repo: params.repo as string | undefined,
    cwd: params.cwd as string | undefined,
  });

  type LockRow = { file_path: string; agent_id: string; session_id: string | null; workspace_path: string | null; intent_id: string; lock_type: string; acquired_at: string; expires_at: string | null };
  const lockWhere = wsPath ? 'WHERE ai.workspace_path = ?' : '';
  const lockParams = wsPath ? [wsPath] : [];
  const locks = db.prepare(
    `SELECT fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path, fl.intent_id,
            fl.lock_type, fl.acquired_at, fl.expires_at
     FROM file_locks fl
     JOIN agent_intents ai ON ai.intent_id = fl.intent_id
     ${lockWhere}
     ORDER BY fl.acquired_at DESC
     LIMIT 50`
  ).all(...lockParams) as unknown as LockRow[];

  return {
    ok: true,
    active_memories: activeMemories,
    pending_intents: pendingIntents,
    active_intents: activeIntents,
    open_refinements: openRefinements,
    locks,
    schema_version: 1,
  };
}

// ─── Memory doc export ─────────────────────────────────────────────────────

/**
 * Generates a markdown report of all active memories.
 * Returns the markdown string — the caller is responsible for writing to disk.
 */
export function exportMemoryDoc(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): string {
  const wsPath = (params.workspace_path as string | undefined) ?? null;
  const now = new Date().toISOString().slice(0, 10);

  const conds: string[] = ["state = 'ACTIVE'"];
  const bindParams: (string | number)[] = [];
  if (wsPath) { conds.push('(workspace_path = ? OR workspace_path IS NULL)'); bindParams.push(wsPath); }

  type MemRow = {
    memory_id: string; label: string; importance_score: number;
    task_context: string; observation: string;
    tags_json: string; references_json: string;
    file: string | null; repo: string | null; ref: string | null;
    failure_signature: string | null; created_at: string;
  };

  const rows = db.prepare(
    `SELECT memory_id, label, importance_score, task_context, observation,
            tags_json, references_json, file, repo, ref, failure_signature, created_at
     FROM agent_memories
     WHERE ${conds.join(' AND ')}
     ORDER BY importance_score DESC, created_at DESC`
  ).all(...bindParams) as unknown as MemRow[];

  const byLabel: Record<string, MemRow[]> = {};
  for (const row of rows) {
    const label = row.label ?? 'OTHER';
    (byLabel[label] ??= []).push(row);
  }

  const lines: string[] = [
    `# Memory Store Report — ${now}`,
    '',
    `**Total active memories:** ${rows.length}`,
    `**By label:** ${Object.entries(byLabel).map(([l, ms]) => `${l}(${ms.length})`).join(', ')}`,
    '',
  ];

  for (const [label, mems] of Object.entries(byLabel)) {
    lines.push(`## ${label}`, '');
    for (const m of mems) {
      // MAINT-5 / ARCH-7: Use parseJsonList instead of duplicated inline IIFEs
      const tags = parseJsonList(m.tags_json);
      const refs = parseJsonList(m.references_json);
      lines.push(
        `### \`${m.memory_id}\` — importance ${m.importance_score}`,
        `**Context:** ${m.task_context}`,
        `**Observation:** ${m.observation}`,
      );
      if (tags.length) lines.push(`**Tags:** ${tags.join(', ')}`);
      if (m.failure_signature) lines.push(`**Failure signature:** ${m.failure_signature}`);
      if (m.file) lines.push(`**File:** ${m.file}`);
      if (m.repo) lines.push(`**Repo:** ${m.repo}${m.ref ? ` @ ${m.ref}` : ''}`);
      if (refs.length) lines.push(`**References:** ${refs.join(', ')}`);
      lines.push(`**Created:** ${m.created_at.slice(0, 10)}`, '');
    }
  }

  return lines.join('\n');
}

// ─── Export harness ─────────────────────────────────────────────────────────────

/**
 * Returns lessons formatted as an AGENTS.md block.
 * Never writes files — caller decides where to put the output.
 *
 * R-3: Two tiers, in priority order:
 *   1. Harness memories — `harness`-tagged via `reflect fix_harness:` (any importance).
 *      These are explicit agent-proposed skill improvements. Always included first.
 *   2. High-importance general lessons — importance ≥ minImportance, label ≠ EXPERIENCE.
 *      Raw reflections (EXPERIENCE) are excluded: they are inputs to the harness loop,
 *      not standing guidance.
 * `harness_only:true` returns tier 1 only (proposed improvements, no general wisdom).
 */
export function exportHarness(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): { count: number; markdown: string; harness_count: number; memories: Array<{ memory_id: string; label: string; importance: number; observation: string; tier: 'harness' | 'general' }> } {
  const limit = Number(params.limit ?? 10);
  const minImportance = Number(params.min_importance ?? params.minImportance ?? 7);
  const wsPath = (params.workspace_path as string | undefined) ?? null;
  const harnessOnly = Boolean(params.harness_only ?? params.harnessOnly ?? false);

  const scopeCond = wsPath ? '(workspace_path = ? OR workspace_path IS NULL)' : null;
  const scopeParams: (string | number)[] = wsPath ? [wsPath] : [];

  type MemRow = { memory_id: string; label: string; importance_score: number; observation: string; tags_text: string };

  // Tier 1: harness-tagged memories (explicit skill improvement proposals)
  const harnessRows = db.prepare(
    `SELECT memory_id, label, importance_score, observation, tags_text
     FROM agent_memories
     WHERE state = 'ACTIVE'
       AND tags_text LIKE '%,harness,%'
       ${ scopeCond ? `AND ${scopeCond}` : ''}
     ORDER BY importance_score DESC, access_count DESC
     LIMIT ?`
  ).all(...scopeParams, limit) as unknown as MemRow[];

  const memories: Array<{ memory_id: string; label: string; importance: number; observation: string; tier: 'harness' | 'general' }> = [];

  for (const r of harnessRows) {
    memories.push({ memory_id: r.memory_id, label: r.label, importance: r.importance_score, observation: r.observation, tier: 'harness' });
  }

  // Tier 2: high-importance general lessons (not EXPERIENCE, not already in tier 1)
  if (!harnessOnly && memories.length < limit) {
    const harnessIds = new Set(memories.map(m => m.memory_id));
    const remaining = limit - memories.length;
    const generalRows = db.prepare(
      `SELECT memory_id, label, importance_score, observation, tags_text
       FROM agent_memories
       WHERE state = 'ACTIVE'
         AND importance_score >= ?
         AND label <> 'EXPERIENCE'
         AND tags_text NOT LIKE '%,harness,%'
         ${ scopeCond ? `AND ${scopeCond}` : ''}
       ORDER BY importance_score DESC, access_count DESC, last_accessed_at DESC
       LIMIT ?`
    ).all(minImportance, ...scopeParams, remaining * 2) as unknown as MemRow[];

    for (const r of generalRows) {
      if (!harnessIds.has(r.memory_id) && memories.length < limit) {
        memories.push({ memory_id: r.memory_id, label: r.label, importance: r.importance_score, observation: r.observation, tier: 'general' });
      }
    }
  }

  if (memories.length === 0) {
    return { count: 0, harness_count: 0, markdown: '<!-- No harness or high-importance memories to export -->', memories: [] };
  }

  const harnessCount = memories.filter(m => m.tier === 'harness').length;
  const lines = [
    '## Agent lessons (generated by octocode-awareness · memory_digest export_doc:true)',
    '',
    '<!-- Tier 1: harness proposals from memory_reflect fix_harness: -->',
    '',
  ];

  const harnessMems = memories.filter(m => m.tier === 'harness');
  const generalMems = memories.filter(m => m.tier === 'general');

  for (const m of harnessMems) {
    lines.push(`- **[HARNESS:${m.importance}]** ${m.observation}`);
  }
  if (generalMems.length > 0) {
    lines.push('', '<!-- Tier 2: high-importance general lessons -->', '');
    for (const m of generalMems) {
      lines.push(`- **[${m.label}:${m.importance}]** ${m.observation}`);
    }
  }
  lines.push('');

  return { count: memories.length, harness_count: harnessCount, markdown: lines.join('\n'), memories };
}
