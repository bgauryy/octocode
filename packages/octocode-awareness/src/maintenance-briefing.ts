/**
 * maintenance.ts — Background maintenance, smart briefing, and session lifecycle operations.
 *
 * pruneStale:          deletes expired exclusive locks; work/run lifecycle stays independent.
 * notifyGet:           returns a smart workspace briefing (top memories + weakness + refinements).
 * digest:              archives expired memories, prunes stale rows/locks, rebuilds FTS.
 * getWorkspaceStatus:  reads active locks, agents, and memory store stats.
 * exportMemoryDoc:     queries all active memories and returns a markdown report string.
 * exportHarness:       returns top recurring lessons as an AGENTS.md block.
 * sessionCapture:      publishes unresolved session work as an open self-addressed handoff signal.
 * waitForLock:         polls active exclusive locks until clear or timeout.
 */
import { createHash } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { getDeliveryFingerprint, setDeliveryFingerprint } from './db-runtime.js';
import { fillScope } from './git.js';
import { assertKnownOptions, normalizeArtifact, summarizeText } from './helpers.js';
import { getMemory } from './memory-recall.js';
import { getNotifications } from './notifications-inbox.js';
import { compactBriefItems, notificationBriefText, summarizeUtf8 } from './maintenance-brief-format.js';
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
export const HOOK_BRIEF_MAX_ITEMS = 5;

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
  assertKnownOptions(params, [
    'workspace_path', 'artifact', 'format', 'query', 'agent_id', 'cwd', 'repo', 'ref', 'session_id',
  ], 'maintenance briefing');
  const wsPath = (params.workspace_path as string | undefined) ?? null;
  const artifact = normalizeArtifact(params.artifact);
  const format  = (params.format as string | undefined) ?? 'json';
  const interventionQuery = String(params.query ?? '').trim().slice(0, 4_000);
  const agentId = String(params.agent_id ?? 'agent');
  // MAINT-2: Use the cwd from params (workspace path) not process.cwd() which
  // would be the shell directory, potentially different from the actual workspace.
  const notifyCwd = wsPath ?? (params.cwd as string | undefined) ?? process.cwd();

  const items: BriefItem[] = [];
  let observationFailed = false;

  // Each query is isolated — one failure does not wipe the others.

  // 0. Unread signals for this agent (signals table). Hook fetch does not ack; agents call agent_signal action:'ack' after acting.
  try {
    const inbox = getNotifications(db, {
      agentId,
      workspacePath: wsPath,
      artifact,
      unreadOnly: true,
      markRead: false,
      // Fetch more than the brief displays so repeated handoff broadcasts can
      // collapse before the top-5 maintenance rows are selected.
      limit: 50,
      cwd: notifyCwd,
    });
    type HandoffCluster = {
      count: number;
      from: string;
      target: string;
      subject: string;
      body: string;
      files: string[];
      importance: number;
    };
    const handoffClusters = new Map<string, HandoffCluster>();
    const normalizeHandoffSubject = (subject: string): string => {
      const normalized = subject.replace(/Review session handoff for pi:[^\s:]+(?::[^\s]+)?/g, 'Review session handoff');
      return /^Review session handoff(?:\b|:)/.test(normalized) ? 'Review session handoff' : normalized;
    };
    const normalizeHandoffBody = (body: string): string =>
      summarizeText(body.replace(/pi:[^\s]+/g, 'pi:<session>'), 120);
    for (const n of inbox.signals) {
      const target = n.to_agent ? `to ${n.to_agent}` : 'broadcast';
      if (n.kind === 'handoff') {
        const normalizedSubject = normalizeHandoffSubject(n.subject);
        const sessionHandoff = normalizedSubject === 'Review session handoff';
        const normalizedBody = sessionHandoff ? '' : normalizeHandoffBody(n.body ?? '');
        const key = JSON.stringify([
          n.kind,
          n.to_agent ?? '',
          normalizedSubject,
          sessionHandoff ? '' : normalizedBody,
        ]);
        const existing = handoffClusters.get(key);
        if (existing) {
          existing.count += 1;
          existing.files.push(...n.files);
          existing.importance = Math.max(existing.importance, n.importance);
          continue;
        }
        handoffClusters.set(key, {
          count: 1,
          from: n.from_agent,
          target,
          subject: normalizedSubject,
          body: normalizedBody,
          files: [...n.files],
          importance: n.importance,
        });
        continue;
      }
      const text = notificationBriefText({
        kind: n.kind,
        from: n.from_agent,
        target,
        files: n.files,
        subject: n.subject,
        body: n.body ?? undefined,
        workspacePath: wsPath,
      });
      items.push({ kind: 'notification', text, importance: n.importance });
    }
    for (const cluster of handoffClusters.values()) {
      const from = cluster.count > 1 ? 'multiple agents' : cluster.from;
      items.push({
        kind: 'notification',
        text: notificationBriefText({
          kind: 'handoff',
          count: cluster.count,
          from,
          target: cluster.target,
          files: cluster.files,
          subject: cluster.subject,
          body: cluster.body,
          workspacePath: wsPath,
        }),
        importance: cluster.importance,
      });
    }
  } catch { observationFailed = true; /* skip signals on error */ }

  // 1a. OVERRIDE memories — always surfaced regardless of importance (they contradict model defaults)
  try {
    type MemRow = { memory_id: string; observation: string; importance: number };
    const overrideConds: string[] = ["state = 'ACTIVE'", "label = 'OVERRIDE'"];
    const overrideBinds: (string | number)[] = [];
    if (wsPath) { overrideConds.push('(workspace_path = ? OR workspace_path IS NULL)'); overrideBinds.push(wsPath); }
    if (artifact) { overrideConds.push('(artifact = ? OR artifact IS NULL)'); overrideBinds.push(artifact); }
    const overrideRows = db.prepare(
      `SELECT memory_id, observation, importance
       FROM awareness_memories
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
  } catch { observationFailed = true; /* skip this section on error */ }

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
         FROM awareness_memories
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
  } catch { observationFailed = true; /* skip this section on error */ }

  // 2. Top mine-weakness cluster
  try {
    type WkRow = { failure_signature: string; freq: number; avg_imp: number };
    const wkConditions = ["failure_signature IS NOT NULL", "state = 'ACTIVE'"];
    const wkParams: (string | number)[] = [];
    if (wsPath) { wkConditions.push('(workspace_path = ? OR workspace_path IS NULL)'); wkParams.push(wsPath); }
    if (artifact) { wkConditions.push('(artifact = ? OR artifact IS NULL)'); wkParams.push(artifact); }
    const topWk = db.prepare(
      `SELECT failure_signature, count(*) AS freq, avg(importance) AS avg_imp
       FROM awareness_memories
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
  } catch { observationFailed = true; /* skip this section on error */ }

  // 3. Open repo-fix refinements count (session handoffs are excluded by default)
  try {
    const refCount = openRefinementCount(db, { workspacePath: wsPath, artifact, cwd: notifyCwd });
    if (refCount > 0) {
      items.push({ kind: 'refinement', text: `📋 ${refCount} open refinement(s) pending` });
    }
  } catch { observationFailed = true; /* skip this section on error */ }

  const delivery = format === 'hook' ? (() => {
    const normalizedScope = fillScope({
      workspace_path: wsPath,
      artifact,
      repo: (params.repo as string | null | undefined) ?? null,
      ref: (params.ref as string | null | undefined) ?? null,
    }, notifyCwd);
    return {
      consumerId: agentId,
      channel: 'briefing',
      scopeKey: JSON.stringify([
        String(params.session_id ?? '-'),
        normalizedScope.workspace_path, normalizedScope.artifact,
        normalizedScope.repo, normalizedScope.ref,
      ]),
    };
  })() : undefined;

  if (items.length === 0) {
    // A healthy empty observation rearms this consumer after recovery. Missing
    // sensor data is not recovery, and initial/unchanged emptiness needs no write.
    if (delivery && !observationFailed) {
      const previous = getDeliveryFingerprint(db, delivery);
      const emptyFingerprint = createHash('sha256').update('').digest('hex');
      if (previous !== null && previous !== emptyFingerprint) {
        setDeliveryFingerprint(db, { ...delivery, fingerprint: emptyFingerprint });
      }
    }
    return { ok: true, count: 0, notifications: [] };
  }

  const result: NotifyGetBriefResult = {
    ok: true,
    count: items.length,
    notifications: items,
  };

  // Hook format: wrap top items as additionalContext for pi injection
  if (delivery) {
    const compactItems = compactBriefItems(items);
    const hookItems = compactItems.slice(0, HOOK_BRIEF_MAX_ITEMS).map(item => ({
      ...item,
      text: summarizeUtf8(item.text, HOOK_BRIEF_ITEM_MAX_BYTES),
    }));
    result.count = hookItems.length;
    result.notifications = hookItems;
    const hiddenCount = Math.max(0, compactItems.length - hookItems.length);
    const duplicateCount = Math.max(0, items.length - compactItems.length);
    const suffixParts = [
      items.length > hookItems.length ? `${items.length} total` : '',
      duplicateCount > 0 ? `${duplicateCount} duplicate${duplicateCount === 1 ? '' : 's'} collapsed` : '',
      hiddenCount > 0 ? `${hiddenCount} more not shown` : '',
    ].filter(Boolean);
    const suffix = suffixParts.length > 0 ? ` — ${suffixParts.join(' · ')}` : '';
    const lines = [
      `🧠 Brief — showing ${hookItems.length}/${Math.max(items.length, hookItems.length)}${suffix}:`,
      ...hookItems.map(i => `  • ${i.text}`),
    ];
    const additionalContext = lines.join('\n');
    const fingerprint = createHash('sha256').update(additionalContext).digest('hex');
    if (getDeliveryFingerprint(db, delivery) === fingerprint) {
      return { ok: true, count: 0, notifications: [] };
    }
    setDeliveryFingerprint(db, { ...delivery, fingerprint });
    result.additionalContext = additionalContext;
  }

  return result;
}
