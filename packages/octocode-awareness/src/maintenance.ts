/**
 * maintenance.ts — Background maintenance, smart briefing, and session lifecycle operations.
 *
 * pruneStale:          deletes expired exclusive locks; work/run lifecycle stays independent.
 * notifyGet:           returns a smart workspace briefing (top memories + weakness + refinements).
 * digest:              archives expired memories, prunes stale rows/locks, rebuilds FTS.
 * getWorkspaceStatus:  returns active locks, agents, and memory store stats.
 * exportMemoryDoc:     queries all active memories and returns a markdown report string.
 * exportHarness:       returns top recurring lessons as an AGENTS.md block.
 * sessionCapture:      records unresolved session work as an open handoff refinement.
 * waitForLock:         polls active exclusive locks until clear or timeout.
 */

import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import {
  checkpointWal,
  evictExpiredLocks,
  getDeliveryFingerprint,
  hasFts,
  rebuildFts,
  setDeliveryFingerprint,
} from './db.js';
import { canonicalizePath, fillScope, normalizeWorkspacePath } from './git.js';
import { normalizeArtifact, parseJsonList, utcNow } from './helpers.js';
import { getNotifications } from './notifications.js';

const SESSION_CAPTURE_FILE_LIMIT = 20;
const SESSION_CAPTURE_VISIBLE_FILE_LIMIT = 10;
const SESSION_CAPTURE_RUN_DETAIL_LIMIT = 3;
const SESSION_CAPTURE_RUN_FILE_LIMIT = 3;
const SESSION_CAPTURE_TEXT_LIMIT = 120;
const MAX_WAIT_MS = 3600_000;
const MAX_RETRY_MS = 300_000;
const DEFAULT_WAIT_MS = 60_000;
const DEFAULT_RETRY_MS = 5_000;

export interface PruneStaleResult {
  pruned_locks: number;
  dry_run?: true;
  would_prune?: number;
}

export interface NotifyGetResult {
  ok: true;
  count: 0;
  notifications: [];
}

export interface SessionCaptureResult {
  ok: true;
  captured: boolean;
  refinement_id: string | null;
  pending_runs: number;
  active_runs: number;
  files: string[];
  dirty_files: string[];
  file_count?: number;
  dirty_file_count?: number;
  omitted_files?: number;
  omitted_dirty_files?: number;
  reason: string | null;
  consolidation_opportunities: number; // memories with novelty_score < 0.2 (candidates for supersede)
  deduplicated?: true;
}

export interface WaitForLockResult {
  ok: true;
  waited_ms: number;
  lock_free: boolean;
  conflicts?: Array<{ file_path: string; agent_id: string; expires_at: string | null }>;
}

function compactText(value: string, max = SESSION_CAPTURE_TEXT_LIMIT): string {
  const normalized = value.replace(/\s+/g, ' ').trim();
  return normalized.length > max ? `${normalized.slice(0, max - 3)}...` : normalized;
}

function listSummary(label: string, items: string[], visibleLimit = SESSION_CAPTURE_VISIBLE_FILE_LIMIT): string | null {
  if (items.length === 0) return null;
  const shown = items.slice(0, visibleLimit);
  const omitted = items.length - shown.length;
  return `${label}${omitted > 0 ? ` (showing ${shown.length} of ${items.length})` : ''}: ${shown.join(', ')}${omitted > 0 ? `; ${omitted} omitted` : ''}.`;
}

function boundedMs(value: unknown, defaultMs: number, minMs: number, maxMs: number): number {
  if (value == null) return defaultMs;
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) return minMs;
  return Math.min(Math.max(numeric, minMs), maxMs);
}

/** Delete expired exclusive locks without changing the independent work lifecycle. */
export function pruneStale(db: DatabaseSync, params: Record<string, unknown> = {}): PruneStaleResult {
  const dryRun = Boolean(params.dry_run ?? params.dryRun);
  const expiredOnly = Boolean(params.expired_only ?? params.expiredOnly);
  const olderThanMinutes = params.older_than_minutes != null ? Number(params.older_than_minutes) :
    params.olderThanMinutes != null ? Number(params.olderThanMinutes) : null;
  const agentId = typeof params.agent_id === 'string' ? params.agent_id :
    typeof params.agentId === 'string' ? params.agentId : null;
  const rawWorkspacePath = typeof params.workspace === 'string' ? params.workspace :
    typeof params.workspace_path === 'string' ? params.workspace_path :
      typeof params.workspacePath === 'string' ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const rawTarget = params.target_file ?? params.targetFile;
  const targetFiles = (Array.isArray(rawTarget) ? rawTarget : rawTarget != null ? [rawTarget] : [])
    .map(String)
    .filter(Boolean)
    .map((file) => {
      const base = rawWorkspacePath ? resolve(rawWorkspacePath) : process.cwd();
      return canonicalizePath(isAbsolute(file) ? resolve(file) : resolve(base, file));
    });
  const now = utcNow();
  // Age cutoff: locks older than N minutes are considered stale even if not expired.
  const ageCutoff = olderThanMinutes != null && !expiredOnly
    ? new Date(Date.now() - olderThanMinutes * 60000).toISOString()
    : null;

  // Selection must be identical for dry-run and real prune, so previews are honest.
  const conditions: string[] = [];
  const binds: string[] = [];
  const staleClauses = ['(l.expires_at IS NOT NULL AND l.expires_at < ?)'];
  binds.push(now);
  if (ageCutoff) {
    staleClauses.push('(l.acquired_at < ?)');
    binds.push(ageCutoff);
  }
  conditions.push(`(${staleClauses.join(' OR ')})`);
  if (agentId) { conditions.push('t.agent_id = ?'); binds.push(agentId); }
  if (targetFiles.length > 0) {
    conditions.push(`l.file_path IN (${targetFiles.map(() => '?').join(',')})`);
    binds.push(...targetFiles);
  }
  if (workspacePath) { conditions.push('t.workspace_path = ?'); binds.push(workspacePath); }
  if (artifact) { conditions.push('(t.artifact = ? OR t.artifact IS NULL)'); binds.push(artifact); }
  const where = conditions.join(' AND ');
  const from = 'locks l JOIN task_runs t ON t.run_id = l.run_id';

  let staleLocks: Array<{ lock_id: string; run_id: string }> = [];
  try {
    staleLocks = db.prepare(
      `SELECT l.lock_id, l.run_id FROM ${from} WHERE ${where}`
    ).all(...binds) as Array<{ lock_id: string; run_id: string }>;
  } catch { /* non-critical stale-lock scan */ }

  if (dryRun) {
    return { pruned_locks: 0, dry_run: true, would_prune: staleLocks.length };
  }
  if (staleLocks.length === 0) {
    return { pruned_locks: 0 };
  }

  const ownsTransaction = !db.isTransaction;
  if (ownsTransaction) db.exec('BEGIN IMMEDIATE');
  try {
    staleLocks = db.prepare(
      `SELECT l.lock_id, l.run_id FROM ${from} WHERE ${where}`
    ).all(...binds) as Array<{ lock_id: string; run_id: string }>;
    if (staleLocks.length === 0) {
      if (ownsTransaction) db.exec('COMMIT');
      return { pruned_locks: 0 };
    }
    const ph = staleLocks.map(() => '?').join(',');
    db.prepare(`DELETE FROM locks WHERE lock_id IN (${ph})`).run(...staleLocks.map(l => l.lock_id));
    if (ownsTransaction) db.exec('COMMIT');
  } catch (e) {
    if (ownsTransaction) {
      try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
    }
    throw e;
  }

  return { pruned_locks: staleLocks.length };
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
}

function openRefinementCount(
  db: DatabaseSync,
  params: { workspacePath?: string | null; artifact?: string | null; repo?: string | null; ref?: string | null; cwd?: string; includeHandoffs?: boolean } = {},
): number {
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    params.cwd ?? process.cwd(),
  );
  const queryParams: (string | number)[] = [];
  let sql = "SELECT COUNT(*) AS c FROM refinements WHERE state IN ('open','ongoing')";
  if (!params.includeHandoffs) sql += " AND quality NOT IN ('handoff','instructions')";
  if (scope.workspace_path) {
    sql += ' AND (workspace_path = ? OR workspace_path IS NULL)';
    queryParams.push(scope.workspace_path);
  }
  if (scope.artifact) {
    sql += ' AND (artifact = ? OR artifact IS NULL)';
    queryParams.push(scope.artifact);
  }
  if (scope.repo) {
    sql += ' AND (repo = ? OR repo IS NULL)';
    queryParams.push(scope.repo);
  }
  if (scope.ref) {
    sql += ' AND (ref = ? OR ref IS NULL)';
    queryParams.push(scope.ref);
  }
  return (db.prepare(sql).get(...queryParams) as { c: number }).c;
}

/**
 * Returns a smart workspace briefing instead of an empty inbox.
 * — Unread agent signals addressed to this agent (or broadcasts)
 * — Top memories (GOTCHA/BUG/DECISION, importance >=6, scoped to workspace)
 * — Top mine-weakness cluster (failure_signature with count >=2)
 * — Count of open refinements
 * Designed to be called by notify-deliver.sh before supported user prompts.
 * Periodic digest cleanup is a separate opt-in path controlled by
 * OCTOCODE_NOTIFY_RUN_DIGEST=1.
 */
// MAINT-3: Briefing label allowlist as a named constant — previously buried inside
// notifyGet making it invisible and hard to tune.
const BRIEFING_LABELS = ['GOTCHA', 'BUG', 'DECISION', 'IMPROVEMENT', 'ARCHITECTURE', 'SECURITY'] as const;

export function notifyGet(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): NotifyGetResult | NotifyGetBriefResult {
  const wsPath = (params.workspace as string | undefined) ?? null;
  const artifact = normalizeArtifact(params.artifact);
  const format  = (params.format as string | undefined) ?? 'json';
  const agentId = String(params.agent_id ?? params.agentId ?? 'agent');
  // MAINT-2: Use the cwd from params (workspace path) not process.cwd() which
  // would be the shell directory, potentially different from the actual workspace.
  const notifyCwd = wsPath ?? (params.cwd as string | undefined) ?? process.cwd();

  const items: BriefItem[] = [];

  // Each query is isolated — one failure does not wipe the others.

  // 0. Unread signals for this agent (signals table). Hook fetch does not ack; agents call agent_signal action:'ack' after acting.
  try {
    const inbox = getNotifications(db, {
      agentId,
      workspacePath: wsPath,
      artifact,
      unreadOnly: true,
      markRead: false,
      limit: 5,
      cwd: notifyCwd,
    });
    for (const n of inbox.signals) {
      const target = n.to_agent ? `to ${n.to_agent}` : 'broadcast';
      const shownFiles = n.files.slice(0, 3);
      const fileSuffix = shownFiles.length > 0
        ? ` files=${shownFiles.join(', ')}${n.files.length > shownFiles.length ? ` (+${n.files.length - shownFiles.length})` : ''}`
        : '';
      const bodySuffix = n.body ? ` — ${n.body.slice(0, 120)}` : '';
      items.push({
        kind: 'notification',
        text: `📨 ${n.kind} from ${n.from_agent} (${target}): ${n.subject}${bodySuffix}${fileSuffix}`,
        importance: n.importance,
      });
    }
  } catch { /* skip signals on error */ }

  // 1a. OVERRIDE memories — always surfaced regardless of importance (they contradict model defaults)
  try {
    type MemRow = { memory_id: string; observation: string; importance: number };
    const overrideConds: string[] = ["state = 'ACTIVE'", "label = 'OVERRIDE'"];
    const overrideBinds: (string | number)[] = [];
    if (wsPath) { overrideConds.push('(workspace_path = ? OR workspace_path IS NULL)'); overrideBinds.push(wsPath); }
    if (artifact) { overrideConds.push('(artifact = ? OR artifact IS NULL)'); overrideBinds.push(artifact); }
    const overrideRows = db.prepare(
      `SELECT memory_id, observation, importance
       FROM memories
       WHERE ${overrideConds.join(' AND ')}
       ORDER BY importance DESC, last_accessed_at DESC
       LIMIT 2`
    ).all(...overrideBinds) as unknown as MemRow[];
    for (const m of overrideRows) {
      items.push({
        kind: 'memory',
        text: `OVERRIDE(${m.importance}): ${m.observation.slice(0, 120)}`,
        importance: m.importance,
      });
    }
  } catch { /* skip this section on error */ }

  // 1b. Top actionable memories for this workspace (EXPERIENCE/reflections excluded)
  try {
    type MemRow = { memory_id: string; observation: string; label: string; importance: number };
    const conditions: string[] = ["state = 'ACTIVE'", "importance >= 6",
      `label IN (${BRIEFING_LABELS.map(() => '?').join(',')})`];
    // BRIEFING_LABELS binds must be pushed before wsPath so they match the IN(?) order in WHERE
    const bindParams: (string | number)[] = [...BRIEFING_LABELS];
    if (wsPath) { conditions.push('(workspace_path = ? OR workspace_path IS NULL)'); bindParams.push(wsPath); }
    if (artifact) { conditions.push('(artifact = ? OR artifact IS NULL)'); bindParams.push(artifact); }
    const memRows = db.prepare(
      `SELECT memory_id, observation, label, importance
       FROM memories
       WHERE ${conditions.join(' AND ')}
       ORDER BY importance DESC, last_accessed_at DESC
       LIMIT 3`
    ).all(...bindParams) as unknown as MemRow[];
    for (const m of memRows) {
      items.push({
        kind: 'memory',
        text: `${m.label}(${m.importance}): ${m.observation.slice(0, 120)}`,
        importance: m.importance,
      });
    }
  } catch { /* skip this section on error */ }

  // 2. Top mine-weakness cluster
  try {
    type WkRow = { failure_signature: string; freq: number; avg_imp: number };
    const wkConditions = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
    const wkParams: (string | number)[] = [];
    if (wsPath) { wkConditions.push('(workspace_path = ? OR workspace_path IS NULL)'); wkParams.push(wsPath); }
    if (artifact) { wkConditions.push('(artifact = ? OR artifact IS NULL)'); wkParams.push(artifact); }
    const topWk = db.prepare(
      `SELECT failure_signature, count(*) AS freq, avg(importance) AS avg_imp
       FROM memories
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
    const refCount = openRefinementCount(db, { workspacePath: wsPath, artifact, cwd: notifyCwd });
    if (refCount > 0) {
      items.push({ kind: 'refinement', text: `📋 ${refCount} open refinement(s) pending` });
    }
  } catch { /* skip this section on error */ }

  if (items.length === 0) {
    return { ok: true, count: 0, notifications: [] };
  }

  const result: NotifyGetBriefResult = {
    ok: true,
    count: items.length,
    notifications: items,
  };

  // Hook format: wrap top items as additionalContext for pi injection
  if (format === 'hook') {
    const hookItems = items.slice(0, 5);
    result.count = hookItems.length;
    result.notifications = hookItems;
    const lines = [
      `🧠 Brief (${hookItems.length}${items.length > hookItems.length ? `/${items.length}` : ''}):`,
      ...hookItems.map(i => `  • ${i.text}`),
    ];
    const additionalContext = lines.join('\n');
    const sessionId = String(params.session_id ?? params.sessionId ?? '-');
    const normalizedScope = fillScope(
      {
        workspace_path: wsPath,
        artifact,
        repo: (params.repo as string | null | undefined) ?? null,
        ref: (params.ref as string | null | undefined) ?? null,
      },
      notifyCwd,
    );
    const scopeKey = JSON.stringify([
      sessionId,
      normalizedScope.workspace_path,
      normalizedScope.artifact,
      normalizedScope.repo,
      normalizedScope.ref,
    ]);
    const fingerprint = createHash('sha256').update(additionalContext).digest('hex');
    const delivery = { consumerId: agentId, channel: 'briefing', scopeKey };
    if (getDeliveryFingerprint(db, delivery) === fingerprint) {
      return { ok: true, count: 0, notifications: [] };
    }
    setDeliveryFingerprint(db, { ...delivery, fingerprint });
    result.additionalContext = additionalContext;
  }

  return result;
}

/**
 * Parse `git status --porcelain=v1` / `--short` lines into paths.
 * Do NOT trim before reading the XY columns — a leading space is significant
 * (`" M file.txt"` must become `file.txt`, not `ile.txt`).
 */
export function parseGitStatusShortLines(stdout: string): string[] {
  const files: string[] = [];
  for (const rawLine of String(stdout).split('\n')) {
    if (!rawLine || rawLine.length < 4) continue;
    const xy = rawLine.slice(0, 2);
    let pathPart = rawLine.slice(3);
    // Rename/copy: keep the destination path after " -> ".
    if (xy.includes('R') || xy.includes('C')) {
      const arrow = pathPart.indexOf(' -> ');
      if (arrow >= 0) pathPart = pathPart.slice(arrow + 4);
    }
    const filePath = pathPart.trim();
    if (filePath) files.push(filePath);
  }
  return files;
}

function gitDirtyFiles(workspacePath: string | null): string[] {
  if (!workspacePath) return [];
  try {
    const result = spawnSync('git', ['-C', workspacePath, 'status', '--porcelain=v1'], {
      encoding: 'utf8',
      timeout: 5000,
    });
    if (result.status !== 0) return [];
    return parseGitStatusShortLines(String(result.stdout));
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

// ─── Background digest ────────────────────────────────────────────────────

export interface DigestResult {
  ok: true;
  archived_memories: number;   // valid_to expired (or would_archive in dry_run)
  pruned_old: number;          // SUPERSEDED older than retention_days
  pruned_locks: number;        // expired file locks
  pruned_refinements: number;  // old handoffs and done refinements
  fts_rebuilt: boolean;
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
  const rawWorkspacePath = typeof params.workspace === 'string' ? params.workspace :
    typeof params.workspace_path === 'string' ? params.workspace_path :
      typeof params.workspacePath === 'string' ? params.workspacePath : null;
  const workspacePath = rawWorkspacePath ? normalizeWorkspacePath(rawWorkspacePath, rawWorkspacePath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const now = new Date().toISOString();
  const cutoff = new Date(Date.now() - retentionDays * 86400000).toISOString();
  const handoffCutoff = new Date(Date.now() - handoffRetentionDays * 86400000).toISOString();
  const doneCutoff = new Date(Date.now() - doneRetentionDays * 86400000).toISOString();
  const memoryScope: string[] = [];
  const memoryScopeBinds: string[] = [];
  if (workspacePath) { memoryScope.push('workspace_path = ?'); memoryScopeBinds.push(workspacePath); }
  if (artifact) { memoryScope.push('artifact = ?'); memoryScopeBinds.push(artifact); }
  const memoryScopeSql = memoryScope.length > 0 ? ` AND ${memoryScope.join(' AND ')}` : '';
  const refinementScope: string[] = [];
  const refinementScopeBinds: string[] = [];
  if (workspacePath) { refinementScope.push('workspace_path = ?'); refinementScopeBinds.push(workspacePath); }
  if (artifact) { refinementScope.push('artifact = ?'); refinementScopeBinds.push(artifact); }
  const refinementScopeSql = refinementScope.length > 0 ? ` AND ${refinementScope.join(' AND ')}` : '';

  // dry_run: count what would change without mutating anything
  if (params.dry_run) {
    const wouldArchive = (db.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
    ).get(now, ...memoryScopeBinds) as { c: number }).c;
    const wouldPruneOld = (db.prepare(
      `SELECT COUNT(*) AS c FROM memories WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
    ).get(cutoff, ...memoryScopeBinds) as { c: number }).c;
    const lockDryRun = pruneStale(db, {
      ...(workspacePath ? { workspace: workspacePath } : {}),
      ...(artifact ? { artifact } : {}),
      expired_only: true,
      dry_run: true,
    });
    const wouldPruneLocks = lockDryRun.would_prune ?? 0;
    const wouldPruneRefinements = (db.prepare(`SELECT COUNT(*) AS c FROM refinements
       WHERE ((quality = 'handoff' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`)
      .get(handoffCutoff, doneCutoff, ...refinementScopeBinds) as { c: number }).c;
    return {
      ok: true,
      archived_memories: 0,
      pruned_old: 0,
      pruned_locks: 0,
      pruned_refinements: 0,
      fts_rebuilt: false,
      dry_run: true,
      would_archive: wouldArchive,
      would_prune_old: wouldPruneOld,
      would_prune_locks: wouldPruneLocks,
      would_prune_refinements: wouldPruneRefinements,
    };
  }

  let archiveRes: { changes: number } = { changes: 0 };
  let deleteRes: { changes: number } = { changes: 0 };
  let prunedLocks = 0;
  let pruneRefinementsRes: { changes: number } = { changes: 0 };
  let ftsRebuilt = false;
  const ownsDigestTransaction = !db.isTransaction;
  if (ownsDigestTransaction) db.exec('BEGIN IMMEDIATE');
  try {
    // 1. Archive expired memories (valid_to < now)
    archiveRes = db.prepare(
      `UPDATE memories
       SET state = 'SUPERSEDED', expired_at = ?, updated_at = ?
       WHERE valid_to IS NOT NULL AND valid_to < ? AND state = 'ACTIVE'${memoryScopeSql}`
    ).run(now, now, now, ...memoryScopeBinds) as { changes: number };

    // 2. Hard-delete old SUPERSEDED entries to keep the DB lean
    deleteRes = db.prepare(
      `DELETE FROM memories
       WHERE state = 'SUPERSEDED' AND updated_at < ?${memoryScopeSql}`
    ).run(cutoff, ...memoryScopeBinds) as { changes: number };

    // 3. Prune expired locks inside the same caller-owned transaction.
    prunedLocks = pruneStale(db, {
      ...(workspacePath ? { workspace: workspacePath } : {}),
      ...(artifact ? { artifact } : {}),
      expired_only: true,
    }).pruned_locks;

    // 4. Prune old session handoffs and completed repo-fix refinements.
    pruneRefinementsRes = db.prepare(
      `DELETE FROM refinements
       WHERE ((quality = 'handoff' AND updated_at < ?)
          OR (quality IN ('good','bad') AND state = 'done' AND updated_at < ?))${refinementScopeSql}`
    ).run(handoffCutoff, doneCutoff, ...refinementScopeBinds) as { changes: number };

    // 5. Rebuild FTS5 from the same committed memory snapshot. A failure is
    // fatal so deleted source rows and the index can never diverge.
    if (hasFts(db)) {
      rebuildFts(db);
      ftsRebuilt = true;
    }
    if (ownsDigestTransaction) db.exec('COMMIT');
  } catch (error) {
    if (ownsDigestTransaction) {
      try { db.exec('ROLLBACK'); } catch { /* already rolled back */ }
    }
    throw error;
  }

  // 6. Absorb WAL pages after bulk maintenance writes (non-fatal on :memory:).
  if (ownsDigestTransaction) checkpointWal(db);

  return {
    ok: true,
    archived_memories: archiveRes.changes,
    pruned_old: deleteRes.changes,
    pruned_locks: prunedLocks,
    pruned_refinements: pruneRefinementsRes.changes,
    fts_rebuilt: ftsRebuilt,
  };
}

// ─── Workspace status ──────────────────────────────────────────────────────

export interface WorkspaceLockEntry {
  file_path: string;
  agent_id: string;
  session_id: string | null;
  workspace_path: string | null;
  artifact: string | null;
  run_id: string;
  lock_type: string;
  acquired_at: string;
  expires_at: string | null;
}

export interface WorkspaceStatusResult {
  ok: true;
  active_memories: number;
  pending_runs: number;
  active_runs: number;
  active_plans: number;
  ready_tasks: number;
  in_progress_tasks: number;
  verify_tasks: number;
  open_refinements: number;
  locks: WorkspaceLockEntry[];
}

/**
 * Returns a snapshot of plans, durable tasks, execution runs, locks, and memory stats.
 * Prunes expired locks first so stale entries don't pollute the view.
 */
export function getWorkspaceStatus(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): WorkspaceStatusResult {
  const rawWsPath = (params.workspace_path as string | undefined) ?? null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact = normalizeArtifact(params.artifact);

  // ARCH-3: Delegate lock eviction to the shared evictExpiredLocks function
  // instead of duplicating the DELETE statement.
  evictExpiredLocks(db);

  const memoryScope: string[] = ["state = 'ACTIVE'"];
  const memoryScopeParams: (string | number)[] = [];
  if (wsPath) { memoryScope.push('(workspace_path = ? OR workspace_path IS NULL)'); memoryScopeParams.push(wsPath); }
  if (artifact) { memoryScope.push('(artifact = ? OR artifact IS NULL)'); memoryScopeParams.push(artifact); }
  const activeMemories = (db.prepare(
    `SELECT COUNT(*) AS c FROM memories WHERE ${memoryScope.join(' AND ')}`
  ).get(...memoryScopeParams) as { c: number }).c;

  const runScopeParts: string[] = [];
  const runScopeParams: (string | number)[] = [];
  if (wsPath) { runScopeParts.push('workspace_path = ?'); runScopeParams.push(wsPath); }
  if (artifact) { runScopeParts.push('(artifact = ? OR artifact IS NULL)'); runScopeParams.push(artifact); }
  const runScope = runScopeParts.length > 0 ? ` AND ${runScopeParts.join(' AND ')}` : '';

  const pendingRuns = (db.prepare(
    `SELECT COUNT(*) AS c FROM task_runs WHERE status = 'PENDING'${runScope}`
  ).get(...runScopeParams) as { c: number }).c;

  const activeRuns = (db.prepare(
    `SELECT COUNT(*) AS c FROM task_runs WHERE status = 'ACTIVE'${runScope}`
  ).get(...runScopeParams) as { c: number }).c;

  const planScopeParts: string[] = [];
  const planScopeParams: (string | number)[] = [];
  if (wsPath) { planScopeParts.push('p.workspace_path = ?'); planScopeParams.push(wsPath); }
  if (artifact) { planScopeParts.push('(p.artifact = ? OR p.artifact IS NULL)'); planScopeParams.push(artifact); }
  const planScope = planScopeParts.length > 0 ? ` AND ${planScopeParts.join(' AND ')}` : '';
  const activePlans = (db.prepare(
    `SELECT COUNT(*) AS c FROM plans p WHERE p.status IN ('DRAFT','ACTIVE','PAUSED')${planScope}`,
  ).get(...planScopeParams) as { c: number }).c;
  const readyTasks = (db.prepare(`SELECT COUNT(*) AS c FROM tasks t JOIN plans p ON p.plan_id = t.plan_id
    WHERE t.status = 'OPEN'${planScope}
      AND NOT EXISTS (SELECT 1 FROM task_claims c WHERE c.task_id = t.task_id AND c.expires_at > ?)
      AND NOT EXISTS (
        SELECT 1 FROM task_dependencies td JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
        WHERE td.task_id = t.task_id AND dependency.status <> 'DONE'
      )`).get(...planScopeParams, utcNow()) as { c: number }).c;
  const inProgressTasks = (db.prepare(
    `SELECT COUNT(*) AS c FROM tasks t JOIN plans p ON p.plan_id = t.plan_id WHERE t.status = 'IN_PROGRESS'${planScope}`,
  ).get(...planScopeParams) as { c: number }).c;
  const verifyTasks = (db.prepare(
    `SELECT COUNT(*) AS c FROM tasks t JOIN plans p ON p.plan_id = t.plan_id WHERE t.status = 'VERIFY'${planScope}`,
  ).get(...planScopeParams) as { c: number }).c;

  const openRefinements = openRefinementCount(db, {
    workspacePath: wsPath,
    artifact,
    repo: params.repo as string | undefined,
    cwd: params.cwd as string | undefined,
  });

  type LockRow = { file_path: string; agent_id: string; session_id: string | null; workspace_path: string | null; artifact: string | null; run_id: string; lock_type: string; acquired_at: string; expires_at: string | null };
  const lockWhereParts: string[] = [];
  const lockParams: (string | number)[] = [];
  if (wsPath) { lockWhereParts.push('ai.workspace_path = ?'); lockParams.push(wsPath); }
  if (artifact) { lockWhereParts.push('(ai.artifact = ? OR ai.artifact IS NULL)'); lockParams.push(artifact); }
  const lockWhere = lockWhereParts.length > 0 ? `WHERE ${lockWhereParts.join(' AND ')}` : '';
  const locks = db.prepare(
    `SELECT fl.file_path, ai.agent_id, ai.session_id, ai.workspace_path, ai.artifact, fl.run_id,
            'EXCLUSIVE' AS lock_type, fl.acquired_at, fl.expires_at
     FROM locks fl
     JOIN task_runs ai ON ai.run_id = fl.run_id
     ${lockWhere}
     ORDER BY fl.acquired_at DESC
     LIMIT 50`
  ).all(...lockParams) as unknown as LockRow[];

  return {
    ok: true,
    active_memories: activeMemories,
    pending_runs: pendingRuns,
    active_runs: activeRuns,
    active_plans: activePlans,
    ready_tasks: readyTasks,
    in_progress_tasks: inProgressTasks,
    verify_tasks: verifyTasks,
    open_refinements: openRefinements,
    locks,
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
  const rawWsPath = (params.workspace_path as string | undefined) ?? null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const now = new Date().toISOString().slice(0, 10);

  const conds: string[] = ["m.state = 'ACTIVE'"];
  const bindParams: (string | number)[] = [];
  if (wsPath) { conds.push('(m.workspace_path = ? OR m.workspace_path IS NULL)'); bindParams.push(wsPath); }
  if (artifact) { conds.push('(m.artifact = ? OR m.artifact IS NULL)'); bindParams.push(artifact); }

  type MemRow = {
    memory_id: string; label: string; importance: number;
    task_context: string; observation: string;
    tags_json: string;
    references: string[];
    repo: string | null; ref: string | null;
    failure_signature: string | null; created_at: string;
  };

  const rows = db.prepare(
    `SELECT m.memory_id, m.label, m.importance, m.task_context, m.observation,
            m.tags_json, m.repo, m.ref, m.failure_signature, m.created_at
     FROM memories m
     WHERE ${conds.join(' AND ')}
     ORDER BY m.importance DESC, m.created_at DESC`
  ).all(...bindParams) as unknown as MemRow[];
  if (rows.length > 0) {
    const refs = db.prepare(
      `SELECT r.memory_id, r.reference
       FROM memory_refs r
       JOIN memories m ON m.memory_id = r.memory_id
       WHERE ${conds.join(' AND ')}
       ORDER BY r.memory_id, r.ordinal`
    ).all(...bindParams) as unknown as Array<{ memory_id: string; reference: string }>;
    const refsByMemory = new Map<string, string[]>();
    for (const ref of refs) {
      const list = refsByMemory.get(ref.memory_id) ?? [];
      list.push(ref.reference);
      refsByMemory.set(ref.memory_id, list);
    }
    for (const row of rows) row.references = refsByMemory.get(row.memory_id) ?? [];
  }

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
      const tags = parseJsonList(m.tags_json);
      lines.push(
        `### \`${m.memory_id}\` — importance ${m.importance}`,
        `**Context:** ${m.task_context}`,
        `**Observation:** ${m.observation}`,
      );
      if (tags.length) lines.push(`**Tags:** ${tags.join(', ')}`);
      if (m.references.length) lines.push(`**References:** ${m.references.join(', ')}`);
      if (m.failure_signature) lines.push(`**Failure signature:** ${m.failure_signature}`);
      if (m.repo) lines.push(`**Repo:** ${m.repo}${m.ref ? ` @ ${m.ref}` : ''}`);
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
 *   2. High-importance general lessons — importance >= minImportance, label != EXPERIENCE.
 *      Raw reflections (EXPERIENCE) are excluded: they are inputs to the harness loop,
 *      not standing guidance.
 * `harness_only:true` returns tier 1 only (proposed improvements, no general wisdom).
 */
export function exportHarness(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): { count: number; markdown: string; harness_count: number; memories: Array<{ memory_id: string; label: string; importance: number; observation: string; tier: 'harness' | 'general' }>; next: string } {
  const limit = Number(params.limit ?? 10);
  const minImportance = Number(params.min_importance ?? params.minImportance ?? 7);
  const rawWsPath = (params.workspace_path as string | undefined) ?? null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const harnessOnly = Boolean(params.harness_only ?? params.harnessOnly ?? false);

  const scopeConds: string[] = [];
  const scopeParams: (string | number)[] = [];
  if (wsPath) { scopeConds.push('(workspace_path = ? OR workspace_path IS NULL)'); scopeParams.push(wsPath); }
  if (artifact) { scopeConds.push('(artifact = ? OR artifact IS NULL)'); scopeParams.push(artifact); }
  const scopeSql = scopeConds.length > 0 ? `AND ${scopeConds.join(' AND ')}` : '';

  type MemRow = { memory_id: string; label: string; importance: number; observation: string };

  // Tier 1: harness-tagged memories (explicit skill improvement proposals)
  const harnessRows = db.prepare(
    `SELECT memory_id, label, importance, observation
     FROM memories
     WHERE state = 'ACTIVE'
       AND tags_json LIKE '%"harness"%'
       ${scopeSql}
     ORDER BY importance DESC, access_count DESC
     LIMIT ?`
  ).all(...scopeParams, limit) as unknown as MemRow[];

  const memories: Array<{ memory_id: string; label: string; importance: number; observation: string; tier: 'harness' | 'general' }> = [];

  for (const r of harnessRows) {
    memories.push({ memory_id: r.memory_id, label: r.label, importance: r.importance, observation: r.observation, tier: 'harness' });
  }

  // Tier 2: high-importance general lessons (not EXPERIENCE, not already in tier 1)
  if (!harnessOnly && memories.length < limit) {
    const harnessIds = new Set(memories.map(m => m.memory_id));
    const remaining = limit - memories.length;
    const generalRows = db.prepare(
      `SELECT memory_id, label, importance, observation
       FROM memories
       WHERE state = 'ACTIVE'
         AND importance >= ?
         AND label <> 'EXPERIENCE'
         AND tags_json NOT LIKE '%"harness"%'
         ${scopeSql}
       ORDER BY importance DESC, access_count DESC, last_accessed_at DESC
       LIMIT ?`
    ).all(minImportance, ...scopeParams, remaining * 2) as unknown as MemRow[];

    for (const r of generalRows) {
      if (!harnessIds.has(r.memory_id) && memories.length < limit) {
        memories.push({ memory_id: r.memory_id, label: r.label, importance: r.importance, observation: r.observation, tier: 'general' });
      }
    }
  }

  if (memories.length === 0) {
    return {
      count: 0,
      harness_count: 0,
      markdown: '<!-- No harness or high-importance memories to export -->',
      memories: [],
      next: 'No harness proposals yet. Use octocode-awareness reflect record --fix-harness "<proposal>" after evidence shows a reusable harness gap.',
    };
  }

  const harnessCount = memories.filter(m => m.tier === 'harness').length;
  const lines = [
    '## Agent lessons (generated by octocode-awareness · reflect export-harness)',
    '',
    '<!-- Tier 1: harness proposals from reflect record --fix-harness: -->',
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

  return {
    count: memories.length,
    harness_count: harnessCount,
    markdown: lines.join('\n'),
    memories,
    next: 'Human review required: apply approved guidance to its owning AGENTS.md, SKILL.md, or doc; run that surface\'s verification and skill review; then record the outcome with octocode-awareness reflect record. Run repo inject only when workspace projections should refresh.',
  };
}
