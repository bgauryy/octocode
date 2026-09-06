import type { DatabaseSync } from 'node:sqlite';
import { normalizeWorkspacePath } from './git.js';
import { assertKnownOptions, normalizeArtifact, parseJsonList, utcNow } from './helpers.js';
import { openRefinementCount } from './maintenance-stale.js';
import { toSimpleLock } from './intents-preflight.js';
import { countReadyTasks } from './tasks-ready.js';
import type { FileLock } from './types/identity-memory.js';

// ─── Workspace status ──────────────────────────────────────────────────────


export interface WorkspaceStatusResult {
  ok: true;
  active_memories: number;
  pending_runs: number;
  active_runs: number;
  active_plans: number;
  ready_tasks: number;
  in_progress_tasks: number;
  verify_tasks: number;
  actionable_refinements: number;
  all_open_refinements: number;
  lock_count: number;
  locks: FileLock[];
}

/**
 * Returns a non-mutating snapshot of plans, durable tasks, execution runs, locks,
 * and memory stats. Explicit maintenance owns expired-row deletion.
 */
export function getWorkspaceStatus(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): WorkspaceStatusResult {
  assertKnownOptions(params, ['workspace_path', 'artifact', 'repo', 'cwd'], 'workspace status');
  const rawWsPath = (params.workspace_path as string | undefined) ?? null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact = normalizeArtifact(params.artifact);

  const memoryScope: string[] = ["state = 'ACTIVE'"];
  const memoryScopeParams: (string | number)[] = [];
  if (wsPath) { memoryScope.push('(workspace_path = ? OR workspace_path IS NULL)'); memoryScopeParams.push(wsPath); }
  if (artifact) { memoryScope.push('(artifact = ? OR artifact IS NULL)'); memoryScopeParams.push(artifact); }
  const activeMemories = (db.prepare(
    `SELECT COUNT(*) AS c FROM awareness_memories WHERE ${memoryScope.join(' AND ')}`
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
    `SELECT COUNT(*) AS c FROM awareness_plans p WHERE p.status IN ('DRAFT','ACTIVE','PAUSED')${planScope}`,
  ).get(...planScopeParams) as { c: number }).c;
  const readyTasks = countReadyTasks(db, { workspacePath: wsPath, artifact });
  const inProgressTasks = (db.prepare(
    `SELECT COUNT(*) AS c FROM awareness_tasks t JOIN awareness_plans p ON p.plan_id = t.plan_id WHERE t.status = 'IN_PROGRESS'${planScope}`,
  ).get(...planScopeParams) as { c: number }).c;
  const verifyTasks = (db.prepare(
    `SELECT COUNT(*) AS c FROM awareness_tasks t JOIN awareness_plans p ON p.plan_id = t.plan_id WHERE t.status = 'VERIFY'${planScope}`,
  ).get(...planScopeParams) as { c: number }).c;

  const actionableRefinements = openRefinementCount(db, {
    workspacePath: wsPath,
    artifact,
    repo: params.repo as string | undefined,
    cwd: params.cwd as string | undefined,
  });
  const allOpenRefinements = openRefinementCount(db, {
    workspacePath: wsPath,
    artifact,
    repo: params.repo as string | undefined,
    cwd: params.cwd as string | undefined,
    includeHandoffs: true,
  });

  type LockRow = { file_path: string; agent_id: string; run_id: string; reason: string; expires_at: string | null };
  const lockWhereParts: string[] = ['(fl.expires_at IS NULL OR fl.expires_at > ?)', "ai.status = 'ACTIVE'"];
  const lockParams: (string | number)[] = [utcNow()];
  if (wsPath) { lockWhereParts.push('ai.workspace_path = ?'); lockParams.push(wsPath); }
  if (artifact) { lockWhereParts.push('(ai.artifact = ? OR ai.artifact IS NULL)'); lockParams.push(artifact); }
  const lockWhere = lockWhereParts.length > 0 ? `WHERE ${lockWhereParts.join(' AND ')}` : '';
  const lockCount = (db.prepare(
    `SELECT COUNT(*) AS count
     FROM awareness_locks fl
     JOIN task_runs ai ON ai.run_id = fl.run_id
     ${lockWhere}`
  ).get(...lockParams) as { count: number }).count;
  const lockRows = db.prepare(
    `SELECT fl.file_path, ai.agent_id, fl.run_id, ai.rationale AS reason, fl.expires_at
     FROM awareness_locks fl
     JOIN task_runs ai ON ai.run_id = fl.run_id
     ${lockWhere}
     ORDER BY fl.acquired_at DESC
     LIMIT 50`
  ).all(...lockParams) as unknown as LockRow[];
  const locks = lockRows.map((row) => toSimpleLock({
    filePath: row.file_path,
    agentId: row.agent_id,
    runId: row.run_id,
    reason: row.reason,
    expiresAt: row.expires_at,
  }));

  return {
    ok: true,
    active_memories: activeMemories,
    pending_runs: pendingRuns,
    active_runs: activeRuns,
    active_plans: activePlans,
    ready_tasks: readyTasks,
    in_progress_tasks: inProgressTasks,
    verify_tasks: verifyTasks,
    actionable_refinements: actionableRefinements,
    all_open_refinements: allOpenRefinements,
    lock_count: lockCount,
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
  assertKnownOptions(params, ['workspace_path', 'artifact'], 'export memory document');
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
     FROM awareness_memories m
     WHERE ${conds.join(' AND ')}
     ORDER BY m.importance DESC, m.created_at DESC`
  ).all(...bindParams) as unknown as MemRow[];
  if (rows.length > 0) {
    const refs = db.prepare(
      `SELECT r.memory_id, r.reference
       FROM memory_refs r
       JOIN awareness_memories m ON m.memory_id = r.memory_id
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
  assertKnownOptions(params, [
    'limit', 'min_importance', 'workspace_path', 'artifact', 'harness_only',
  ], 'export harness');
  const limit = Number(params.limit ?? 10);
  const minImportance = Number(params.min_importance ?? 7);
  const rawWsPath = (params.workspace_path as string | undefined) ?? null;
  const wsPath = rawWsPath ? normalizeWorkspacePath(rawWsPath, rawWsPath) : null;
  const artifact = normalizeArtifact(params.artifact);
  const harnessOnly = Boolean(params.harness_only ?? false);

  const scopeConds: string[] = [];
  const scopeParams: (string | number)[] = [];
  if (wsPath) { scopeConds.push('(workspace_path = ? OR workspace_path IS NULL)'); scopeParams.push(wsPath); }
  if (artifact) { scopeConds.push('(artifact = ? OR artifact IS NULL)'); scopeParams.push(artifact); }
  const scopeSql = scopeConds.length > 0 ? `AND ${scopeConds.join(' AND ')}` : '';

  type MemRow = { memory_id: string; label: string; importance: number; observation: string };

  // Tier 1: harness-tagged memories (explicit skill improvement proposals)
  const harnessRows = db.prepare(
    `SELECT memory_id, label, importance, observation
     FROM awareness_memories
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
       FROM awareness_memories
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
      next: 'No harness proposals yet. Use npx @octocodeai/octocode-awareness reflect record --fix-harness "<proposal>" after evidence shows a reusable harness gap.',
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
    next: 'Human review required: apply approved guidance to its owning AGENTS.md, SKILL.md, or doc; run that surface\'s verification and skill review; then record the outcome with npx @octocodeai/octocode-awareness reflect record.',
  };
}
