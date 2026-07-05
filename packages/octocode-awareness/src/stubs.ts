/**
 * stubs.ts — Mixed maintenance implementations plus remaining compatibility stubs.
 *
 * pruneStale:          REAL — deletes expired file locks, sets affected intents to PENDING.
 * notifyGet:           REAL — returns a smart lean briefing for hooks.
 * digest:              REAL — archives expired memories, prunes stale rows/locks, rebuilds FTS.
 * getWorkspaceStatus:  REAL — returns active locks, agents, memory stats for the workspace.
 * exportMemoryDoc:     REAL — queries all active memories and returns a markdown report string.
 * sessionCapture:      REAL — records unresolved session work as an open refinement.
 * waitForLock:         REAL — polls active exclusive locks until clear or timeout.
 */

import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { hasFts, rebuildFts } from './db.js';
import { fillScope } from './git.js';
import { parseJsonList, utcNow } from './helpers.js';

export interface PruneStaleResult {
  pruned_locks: number;
  updated_intents: number;
  dry_run?: true;
  would_prune?: number;
}

export interface NotifyGetResult {
  ok: true;
  count: 0;
  notifications: never[];
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
  const olderThanMinutes = params.older_than_minutes != null ? Number(params.older_than_minutes) :
    params.olderThanMinutes != null ? Number(params.olderThanMinutes) : null;
  const now = utcNow();
  // Age cutoff: locks older than N minutes are considered stale even if no expires_at
  const ageCutoff = olderThanMinutes != null
    ? new Date(Date.now() - olderThanMinutes * 60000).toISOString()
    : null;

  if (dryRun) {
    let count = 0;
    try {
      const row = db.prepare(
        `SELECT COUNT(*) AS c FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?`
      ).get(now) as { c: number };
      count += row.c;
      if (ageCutoff) {
        const row2 = db.prepare(
          `SELECT COUNT(*) AS c FROM file_locks WHERE acquired_at < ? AND (expires_at IS NULL OR expires_at >= ?)`
        ).get(ageCutoff, now) as { c: number };
        count += row2.c;
      }
    } catch { /* ignore */ }
    return { pruned_locks: 0, updated_intents: 0, dry_run: true, would_prune: count };
  }

  const expiredLocks = db.prepare(`
    SELECT fl.lock_id, fl.intent_id
    FROM file_locks fl
    WHERE fl.expires_at IS NOT NULL AND fl.expires_at < ?
  `).all(now) as Array<{ lock_id: string; intent_id: string }>;

  if (expiredLocks.length === 0) {
    return { pruned_locks: 0, updated_intents: 0 };
  }

  db.prepare(
    'DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?'
  ).run(now);

  const affectedIntentIds = [...new Set(expiredLocks.map(l => l.intent_id))];
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

  return { pruned_locks: expiredLocks.length, updated_intents: updatedIntents };
}

// ─── Smart briefing ─────────────────────────────────────────────────────────

export interface BriefItem {
  kind: 'memory' | 'weakness' | 'refinement';
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
 * — Top memories (GOTCHA/BUG/DECISION, importance ≥6, scoped to workspace)
 * — Top mine-weakness cluster (failure_signature with count ≥2)
 * — Count of open refinements
 * Designed to be called by notify-deliver.sh before every user prompt.
 */
export function notifyGet(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): NotifyGetResult | NotifyGetBriefResult {
  const wsPath = (params.workspace as string | undefined) ?? null;
  const format  = (params.format as string | undefined) ?? 'json';

  const items: BriefItem[] = [];

  // Each query is isolated — one failure does not wipe the others.

  // 1. Top actionable memories for this workspace (EXPERIENCE/reflections excluded)
  try {
    type MemRow = { memory_id: string; observation: string; label: string; importance_score: number };
    const conditions: string[] = ["state = 'ACTIVE'", "importance_score >= 6",
      "label IN ('GOTCHA','BUG','DECISION','IMPROVEMENT','ARCHITECTURE','SECURITY')"];
    const bindParams: (string | number)[] = [];
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
    const refCount = openRefinementCount(db, { workspacePath: wsPath, cwd: process.cwd() });
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
  };
}

/**
 * REAL: Poll until target file locks clear, bounded by waitMs.
 * Uses Atomics.wait for efficient sleeping without busy-spin.
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
  const start = Date.now();

  if (targetFiles.length === 0) {
    return { ok: true, waited_ms: 0, lock_free: true };
  }

  const checkLocks = () => {
    const now = new Date().toISOString();
    const ph = targetFiles.map(() => '?').join(',');
    type LockRow = { file_path: string; agent_id: string; expires_at: string | null };
    const locks = db.prepare(
      `SELECT fl.file_path, ai.agent_id, fl.expires_at
       FROM file_locks fl
       JOIN agent_intents ai ON ai.intent_id = fl.intent_id
       WHERE fl.file_path IN (${ph})
         AND ai.agent_id <> ?
         AND ai.status = 'ACTIVE'
         AND fl.lock_type = 'EXCLUSIVE'
         AND (fl.expires_at IS NULL OR fl.expires_at > ?)`
    ).all(...targetFiles, agentId, now) as unknown as LockRow[];
    return locks;
  };

  let conflicts = checkLocks();
  const waited = () => Date.now() - start;

  while (conflicts.length > 0 && waited() < waitMs) {
    // Atomics.wait blocks the thread cleanly (no spin)
    const buf = new Int32Array(new SharedArrayBuffer(4));
    Atomics.wait(buf, 0, 0, Math.min(retryMs, waitMs - waited()));
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

  const refinementRetentionSql = `SELECT COUNT(*) AS c FROM refinements
     WHERE (quality = 'handoff' AND created_at < ?)
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
     SET state = 'SUPERSEDED', expired_at = ?
     WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'`
  ).run(now, now) as { changes: number };

  // 2. Hard-delete old SUPERSEDED entries to keep the DB lean
  const deleteRes = db.prepare(
    `DELETE FROM agent_memories
     WHERE state = 'SUPERSEDED' AND updated_at < ?`
  ).run(cutoff) as { changes: number };

  // 3. Prune expired locks (reuse existing function)
  const { pruned_locks } = pruneStale(db, {});

  // 4. Prune old session handoffs and completed repo-fix refinements.
  const pruneRefinementsRes = db.prepare(
    `DELETE FROM refinements
     WHERE (quality = 'handoff' AND created_at < ?)
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
  const now = utcNow();
  const wsPath = (params.workspace_path as string | undefined) ?? null;

  // Evict expired locks so they don't show as "active"
  db.prepare(
    'DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?'
  ).run(now);

  const activeMemories = (db.prepare(
    `SELECT COUNT(*) AS c FROM agent_memories WHERE state = 'ACTIVE'`
  ).get() as { c: number }).c;

  const pendingIntents = (db.prepare(
    `SELECT COUNT(*) AS c FROM agent_intents WHERE status = 'PENDING'`
  ).get() as { c: number }).c;

  const activeIntents = (db.prepare(
    `SELECT COUNT(*) AS c FROM agent_intents WHERE status = 'ACTIVE'`
  ).get() as { c: number }).c;

  const openRefinements = openRefinementCount(db, {
    workspacePath: wsPath,
    repo: params.repo as string | undefined,
    cwd: params.cwd as string | undefined,
  });

  type LockRow = { file_path: string; agent_id: string; lock_type: string; acquired_at: string; expires_at: string | null };
  const locks = db.prepare(
    `SELECT fl.file_path, ai.agent_id, fl.lock_type, fl.acquired_at, fl.expires_at
     FROM file_locks fl
     JOIN agent_intents ai ON ai.intent_id = fl.intent_id
     ORDER BY fl.acquired_at DESC
     LIMIT 50`
  ).all() as unknown as LockRow[];

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
      const tags: string[] = (() => { try { return JSON.parse(m.tags_json) as string[]; } catch { return []; } })();
      const refs: string[] = (() => { try { return JSON.parse(m.references_json) as string[]; } catch { return []; } })();
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
 * Returns top recurring lessons formatted as an AGENTS.md block.
 * Never writes files — caller decides where to put the output.
 */
export function exportHarness(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): { count: number; markdown: string; memories: Array<{ memory_id: string; label: string; importance: number; observation: string }> } {
  const limit = Number(params.limit ?? 10);
  const minImportance = Number(params.min_importance ?? params.minImportance ?? 7);
  const wsPath = (params.workspace_path as string | undefined) ?? null;

  const conds: string[] = ["state = 'ACTIVE'", 'importance_score >= ?'];
  const bindParams: (string | number)[] = [minImportance];
  if (wsPath) { conds.push('(workspace_path = ? OR workspace_path IS NULL)'); bindParams.push(wsPath); }

  type MemRow = { memory_id: string; label: string; importance_score: number; observation: string };
  const rows = db.prepare(
    `SELECT memory_id, label, importance_score, observation
     FROM agent_memories
     WHERE ${conds.join(' AND ')}
     ORDER BY importance_score DESC, access_count DESC, last_accessed_at DESC
     LIMIT ?`
  ).all(...bindParams, limit) as unknown as MemRow[];

  const memories = rows.map(r => ({
    memory_id: r.memory_id,
    label: r.label,
    importance: r.importance_score,
    observation: r.observation,
  }));

  if (memories.length === 0) {
    return { count: 0, markdown: '<!-- No high-importance memories to export -->', memories: [] };
  }

  const lines = [
    '## Agent lessons (auto-generated by octocode-awareness export-harness)',
    '',
    '<!-- Do not edit manually. Re-run `awareness export-harness` to refresh. -->',
    '',
  ];
  for (const m of memories) {
    lines.push(`- **[${m.label}:${m.importance}]** ${m.observation}`);
  }
  lines.push('');

  return { count: memories.length, markdown: lines.join('\n'), memories };
}
