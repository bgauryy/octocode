/**
 * maintenance.ts — Background maintenance, smart briefing, and session lifecycle operations.
 *
 * pruneStale:          deletes expired exclusive locks; work/run lifecycle stays independent.
 * notifyGet:           returns a smart workspace briefing (top memories + weakness + refinements).
 * digest:              archives expired memories, prunes stale rows/locks, rebuilds FTS.
 * getWorkspaceStatus:  reads active locks, agents, and memory store stats.
 * exportMemoryDoc:     queries all active memories and returns a markdown report string.
 * exportHarness:       returns top recurring lessons as an AGENTS.md block.
 * sessionCapture:      records unresolved session work as an open handoff refinement.
 * waitForLock:         polls active exclusive locks until clear or timeout.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { getDeliveryFingerprint, setDeliveryFingerprint } from './db.js';
import { fillScope } from './git.js';
import { normalizeArtifact, summarizeText } from './helpers.js';
import { getMemory } from './memory.js';
import { getNotifications } from './notifications.js';
import { BriefItem, NotifyGetBriefResult, NotifyGetResult, openRefinementCount } from './maintenance-stale.js';

/**
 * Returns a smart workspace briefing instead of an empty inbox.
 * — Unread agent signals addressed to this agent (or broadcasts)
 * — Top memories (GOTCHA/BUG/DECISION, importance >=6, scoped to workspace)
 * — Top mine-weakness cluster (failure_signature with count >=2)
 * — Count of open refinements
 * Designed to be called by notify-deliver.sh before supported user prompts.
 * Optional prompt-time maintenance preview is controlled by
 * OCTOCODE_NOTIFY_RUN_DIGEST=1; it never applies the digest.
 */
// MAINT-3: Briefing label allowlist as a named constant — previously buried inside
// notifyGet making it invisible and hard to tune.
export const BRIEFING_LABELS = ['GOTCHA', 'BUG', 'DECISION', 'IMPROVEMENT', 'ARCHITECTURE', 'SECURITY'] as const;
export const INTERVENTION_CANDIDATE_LIMIT = 50;
export const HOOK_BRIEF_ITEM_MAX_BYTES = 180;

export const INTERVENTION_STOP_WORDS = new Set([
  'the', 'and', 'for', 'with', 'from', 'into', 'this', 'that', 'about',
  'before', 'after', 'fix', 'update', 'change', 'make', 'during',
]);

export function interventionTokens(text: string): Set<string> {
  return new Set(
    (text.toLowerCase().match(/[a-z0-9]{3,}/g) ?? [])
      .filter(token => !INTERVENTION_STOP_WORDS.has(token)),
  );
}

export function summarizeUtf8(value: string, maxBytes: number): string {
  const flat = value.replace(/\s+/g, ' ').trim();
  if (Buffer.byteLength(flat, 'utf8') <= maxBytes) return flat;
  const suffix = '...';
  const suffixBytes = Buffer.byteLength(suffix, 'utf8');
  let bytes = 0;
  let output = '';
  for (const character of flat) {
    const characterBytes = Buffer.byteLength(character, 'utf8');
    if (bytes + characterBytes + suffixBytes > maxBytes) break;
    output += character;
    bytes += characterBytes;
  }
  return output.trimEnd() + suffix;
}

export function isPromptGroundedMemory(
  query: string,
  memory: { task_context: string; observation: string; label: string; failure_signature?: string | null },
): boolean {
  const queryTokens = interventionTokens(query);
  if (queryTokens.size < 2) return false;
  const memoryTokens = interventionTokens([
    memory.task_context,
    memory.observation,
    memory.label,
    memory.failure_signature ?? '',
  ].join(' '));
  let overlap = 0;
  for (const token of queryTokens) {
    if (memoryTokens.has(token) && ++overlap >= 2) return true;
  }
  return false;
}

export function notifyGet(
  db: DatabaseSync,
  params: Record<string, unknown> = {},
): NotifyGetResult | NotifyGetBriefResult {
  const wsPath = (params.workspace as string | undefined) ?? null;
  const artifact = normalizeArtifact(params.artifact);
  const format  = (params.format as string | undefined) ?? 'json';
  const interventionQuery = String(params.query ?? '').trim().slice(0, 4_000);
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
      const fileSuffix = n.files.length > 0
        ? ` files=${n.files.length}[${summarizeText(n.files[0]!, 48)}]`
        : '';
      const bodySuffix = n.body ? ` — ${summarizeText(n.body, 60)}` : '';
      items.push({
        kind: 'notification',
        text: `📨 ${n.kind} from ${n.from_agent} (${target})${fileSuffix}: ${summarizeText(n.subject, 72)}${bodySuffix}`,
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

  // 1b. Hook delivery is a selective intervention: one query-grounded memory
  // lead or silence. Non-hook callers keep the general briefing view.
  try {
    type BriefMemory = {
      memory_id: string;
      task_context: string;
      observation: string;
      label: string;
      importance: number;
      failure_signature?: string | null;
    };
    let memRows: BriefMemory[] = [];
    if (format === 'hook') {
      if (interventionQuery) {
        const recall = getMemory(db, {
          query: interventionQuery,
          // Grounding is stricter than retrieval. Inspect the full normal recall
          // budget so high-importance one-token hits cannot starve a lower-ranked
          // memory that satisfies the two-token intervention gate.
          limit: INTERVENTION_CANDIDATE_LIMIT,
          minImportance: 6,
          label: [...BRIEFING_LABELS],
          workspacePath: wsPath,
          artifact,
          repo: (params.repo as string | null | undefined) ?? null,
          ref: (params.ref as string | null | undefined) ?? null,
          recordAccess: false,
          cwd: notifyCwd,
        });
        const selected = recall.memories.find(memory => isPromptGroundedMemory(interventionQuery, memory));
        if (selected) memRows = [selected];
      }
    } else {
      const conditions: string[] = ["state = 'ACTIVE'", "importance >= 6",
        `label IN (${BRIEFING_LABELS.map(() => '?').join(',')})`];
      // BRIEFING_LABELS binds must be pushed before wsPath so they match the IN(?) order in WHERE
      const bindParams: (string | number)[] = [...BRIEFING_LABELS];
      if (wsPath) { conditions.push('(workspace_path = ? OR workspace_path IS NULL)'); bindParams.push(wsPath); }
      if (artifact) { conditions.push('(artifact = ? OR artifact IS NULL)'); bindParams.push(artifact); }
      memRows = db.prepare(
        `SELECT memory_id, task_context, observation, label, importance, failure_signature
         FROM memories
         WHERE ${conditions.join(' AND ')}
         ORDER BY importance DESC, last_accessed_at DESC
         LIMIT 3`
      ).all(...bindParams) as unknown as BriefMemory[];
    }
    for (const m of memRows) {
      items.push({
        kind: 'memory',
        text: `Memory lead — verify: ${m.label}(${m.importance}): ${m.observation.slice(0, 120)}`,
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
    const hookItems = items.slice(0, 5).map(item => ({
      ...item,
      text: summarizeUtf8(item.text, HOOK_BRIEF_ITEM_MAX_BYTES),
    }));
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

export function gitDirtyFiles(workspacePath: string | null): string[] {
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
