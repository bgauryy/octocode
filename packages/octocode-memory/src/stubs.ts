/**
 * stubs.ts — Mixed maintenance implementations plus remaining compatibility stubs.
 *
 * pruneStale:          REAL — deletes expired file locks, sets affected intents to PENDING.
 * notifyGet:           REAL — returns a smart lean briefing for hooks.
 * digest:              REAL — archives expired memories, prunes stale rows/locks, rebuilds FTS.
 * getWorkspaceStatus:  REAL — returns active locks, agents, memory stats for the workspace.
 * exportMemoryDoc:     REAL — queries all active memories and returns a markdown report string.
 * sessionCapture:      STUB — no-op.
 * waitForLock:         STUB — returns immediately.
 *
 * Remaining stubs write "[stub] <command>: not yet implemented" to stderr.
 */

import type { DatabaseSync } from 'node:sqlite';
import { hasFts, rebuildFts } from './db.js';
import { utcNow } from './helpers.js';

export interface PruneStaleResult {
  pruned_locks: number;
  updated_intents: number;
}

export interface NotifyGetResult {
  ok: true;
  count: 0;
  notifications: never[];
  schema_version: 1;
}

export interface SessionCaptureResult {
  ok: true;
  captured: false;
}

export interface WaitForLockResult {
  ok: true;
  waited_ms: 0;
  lock_free: true;
}

/** REAL: Delete expired file locks and set parent intents to PENDING. */
export function pruneStale(db: DatabaseSync, _params: Record<string, unknown> = {}): PruneStaleResult {
  const now = utcNow();

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

  // 3. Open refinements count
  try {
    const refConds = ["state = 'open'"];
    const refParams: (string | number)[] = [];
    if (wsPath) { refConds.push('(workspace_path = ? OR workspace_path IS NULL)'); refParams.push(wsPath); }
    type RefRow = { cnt: number };
    const refRow = db.prepare(
      `SELECT count(*) AS cnt FROM refinements WHERE ${refConds.join(' AND ')}`
    ).get(...refParams) as unknown as RefRow | undefined;
    const refCount = refRow?.cnt ?? 0;
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

/** STUB: No-op session capture. */
export function sessionCapture(
  _db: DatabaseSync,
  _params: Record<string, unknown> = {},
): SessionCaptureResult {
  process.stderr.write('[stub] session-capture: not yet implemented; skipping\n');
  return { ok: true, captured: false };
}

/** STUB: Returns immediately (lock considered free). */
export function waitForLock(
  _db: DatabaseSync,
  _params: Record<string, unknown> = {},
): WaitForLockResult {
  process.stderr.write('[stub] wait-for-lock: not yet implemented; returning immediately\n');
  return { ok: true, waited_ms: 0, lock_free: true };
}

// ─── Background digest ────────────────────────────────────────────────────

export interface DigestResult {
  ok: true;
  archived_memories: number;   // valid_to expired (or would_archive in dry_run)
  pruned_old: number;          // SUPERSEDED older than retention_days
  pruned_locks: number;        // expired file locks
  fts_rebuilt: boolean;
  schema_version: 1;
  dry_run?: true;
  would_archive?: number;
  would_prune_old?: number;
  would_prune_locks?: number;
}

/**
 * Background consolidation — designed to run non-blocking every few hours.
 * 1. Archive memories whose valid_to has passed
 * 2. Hard-delete SUPERSEDED memories older than retention_days
 * 3. Prune expired file locks
 * 4. Rebuild / optimize the FTS5 index
 */
export function digest(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): DigestResult {
  const retentionDays = Number(params.retention_days ?? 90);
  const now = new Date().toISOString();
  const cutoff = new Date(Date.now() - retentionDays * 86400000).toISOString();

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
    return {
      ok: true,
      archived_memories: 0,
      pruned_old: 0,
      pruned_locks: 0,
      fts_rebuilt: false,
      schema_version: 1,
      dry_run: true,
      would_archive: wouldArchive,
      would_prune_old: wouldPruneOld,
      would_prune_locks: wouldPruneLocks,
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

  // 4. Rebuild FTS5 index from the agent_memories source of truth.
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

  const refConds = ["state IN ('open','ongoing')"];
  const refParams: (string | number)[] = [];
  if (wsPath) { refConds.push('(workspace_path = ? OR workspace_path IS NULL)'); refParams.push(wsPath); }
  const openRefinements = (db.prepare(
    `SELECT COUNT(*) AS c FROM refinements WHERE ${refConds.join(' AND ')}`
  ).get(...refParams) as { c: number }).c;

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
