/**
 * notifications.ts — Agent-to-agent workspace messaging.
 *
 * Mirrors Python awareness.py's notify / notify-get / notify-resolve / notify-prune.
 * Uses the `signals` + `signal_reads` tables in the schema.
 */

import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, normalizeNotificationKind, utcNow, parseJsonList } from './helpers.js';
import { fillScope } from './git.js';
import {
  SIGNALS_SELECT_THREAD_ID,
  SIGNALS_INSERT,
  SIGNALS_SELECT_BASE,
  SIGNALS_SELECT_LEFT_JOIN_READS,
  SIGNALS_SELECT_ORDER_LIMIT,
  SIGNALS_DELETE_BY_IDS,
  SIGNAL_READS_INSERT_IGNORE,
  SIGNAL_READS_DELETE_BY_SIGNAL_IDS,
} from './sql/index.js';
import type {
  InsertNotificationParams, InsertNotificationResult,
  GetNotificationsParams, GetNotificationsResult,
  ResolveNotificationParams, ResolveNotificationResult,
  PruneNotificationsParams, PruneNotificationsResult,
  NotificationRecord, NotificationKind, NotificationStatus,
  AgentSignalParams, AgentSignalResult, AgentSignalRecord,
} from './types.js';

// ─── Internal row type ────────────────────────────────────────────────────────

interface NotificationRow {
  signal_id: string;
  workspace_path: string;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  from_agent: string;
  to_agent: string | null;
  kind: string;
  subject: string;
  body: string | null;
  files_json: string;
  refs_json: string;
  thread_id: string;
  reply_to: string | null;
  importance: number;
  status: string;
  created_at: string;
}

function rowToNotification(r: NotificationRow): NotificationRecord {
  return {
    signal_id: r.signal_id,
    workspace_path: r.workspace_path,
    artifact: r.artifact,
    repo: r.repo,
    ref: r.ref,
    from_agent: r.from_agent,
    to_agent: r.to_agent,
    kind: r.kind as NotificationKind,
    subject: r.subject,
    body: r.body,
    // ARCH-7: Use shared parseJsonList helper instead of duplicated inline IIFEs
    files: parseJsonList(r.files_json),
    refs: parseJsonList(r.refs_json),
    thread_id: r.thread_id,
    reply_to: r.reply_to,
    importance: r.importance,
    status: r.status as NotificationStatus,
    created_at: r.created_at,
  };
}

// ─── insertNotification ────────────────────────────────────────────────────────

export function insertNotification(
  db: DatabaseSync,
  params: InsertNotificationParams,
): InsertNotificationResult {
  const {
    agentId,
    toAgent = null,
    kind,
    subject,
    body = null,
    files = [],
    refIds = [],
    inReplyTo = null,
    importance = 5,
    cwd,
  } = params;

  const normalizedKind = normalizeNotificationKind(kind);
  if (!Number.isInteger(importance) || importance < 1 || importance > 10) {
    throw new Error(`importance must be an integer between 1 and 10, got ${String(importance)}`);
  }

  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd(),
  );

  const signalId = 'ntf_' + randomUUID().replace(/-/g, '');
  const createdAt = utcNow();
  const wsPath = scope.workspace_path ?? process.cwd();

  // Thread: inherit from parent or start new
  let threadId: string;
  if (inReplyTo) {
    const parent = db.prepare(SIGNALS_SELECT_THREAD_ID).get(inReplyTo) as { thread_id: string } | undefined;
    if (!parent) {
      throw new Error(`insertNotification: parent signal ${inReplyTo} not found (deleted?). Omit inReplyTo to start a new thread.`);
    }
    if (!canReadOrJoinThread(db, parent.thread_id, agentId)) {
      throw new Error(`insertNotification: agent ${agentId} is not a participant in thread ${parent.thread_id}`);
    }
    threadId = parent.thread_id;
  } else {
    threadId = signalId;
  }

  db.prepare(SIGNALS_INSERT).run(
    signalId, wsPath, scope.artifact, scope.repo, scope.ref,
    agentId, toAgent, normalizedKind, subject, body,
    JSON.stringify(files), JSON.stringify(refIds),
    threadId, inReplyTo, importance, createdAt,
  );

  return { signal_id: signalId, thread_id: threadId, workspace_path: wsPath, artifact: scope.artifact };
}

function appendSignalScope(
  where: string[],
  binds: (string | number)[],
  scope: { workspace_path: string | null; artifact: string | null; repo: string | null; ref: string | null },
  alias = 'n',
): void {
  const prefix = alias ? `${alias}.` : '';
  if (scope.workspace_path) {
    where.push(`(${prefix}workspace_path = ? OR ${prefix}workspace_path IS NULL)`);
    binds.push(scope.workspace_path);
  }
  if (scope.artifact) {
    where.push(`(${prefix}artifact = ? OR ${prefix}artifact IS NULL)`);
    binds.push(scope.artifact);
  }
  if (scope.repo) {
    where.push(`(${prefix}repo = ? OR ${prefix}repo IS NULL)`);
    binds.push(scope.repo);
  }
  if (scope.ref) {
    where.push(`(${prefix}ref = ? OR ${prefix}ref IS NULL)`);
    binds.push(scope.ref);
  }
}

function isBroadcastThread(db: DatabaseSync, threadId: string): boolean {
  return db.prepare(`SELECT 1 FROM signals
    WHERE thread_id = ? AND reply_to IS NULL AND to_agent IS NULL
    LIMIT 1`).get(threadId) != null;
}

function isThreadParticipant(db: DatabaseSync, threadId: string, agentId: string): boolean {
  const addressed = db.prepare(`SELECT 1 FROM signals
    WHERE thread_id = ? AND (from_agent = ? OR to_agent = ?)
    LIMIT 1`).get(threadId, agentId, agentId) != null;
  if (addressed) return true;
  if (!isBroadcastThread(db, threadId)) return false;
  return db.prepare(`SELECT 1 FROM signal_reads read
    JOIN signals signal ON signal.signal_id = read.signal_id
    WHERE signal.thread_id = ? AND read.agent_id = ?
    LIMIT 1`).get(threadId, agentId) != null;
}

function canReadOrJoinThread(db: DatabaseSync, threadId: string, agentId: string): boolean {
  return isBroadcastThread(db, threadId) || isThreadParticipant(db, threadId, agentId);
}

function inferReplyTargets(db: DatabaseSync, inReplyTo: string, agentId: string): string[] {
  const parent = db.prepare('SELECT thread_id FROM signals WHERE signal_id = ?')
    .get(inReplyTo) as { thread_id: string } | undefined;
  if (!parent) {
    throw new Error(`insertNotification: parent signal ${inReplyTo} not found (deleted?). Omit inReplyTo to start a new thread.`);
  }
  const rows = db.prepare('SELECT from_agent, to_agent FROM signals WHERE thread_id = ?')
    .all(parent.thread_id) as unknown as Array<{ from_agent: string; to_agent: string | null }>;
  const participants = new Set<string>();
  for (const row of rows) {
    participants.add(row.from_agent);
    if (row.to_agent) participants.add(row.to_agent);
  }
  participants.delete(agentId);
  if (participants.size === 0) {
    throw new Error('agent_signal reply has no inferred recipient; pass --to-agent');
  }
  return [...participants].sort();
}

// ─── getNotifications ──────────────────────────────────────────────────────────

export function getNotifications(
  db: DatabaseSync,
  params: GetNotificationsParams,
): GetNotificationsResult {
  const {
    agentId,
    kinds = [],
    signalIds = [],
    threadId = null,
    unreadOnly = true,
    markRead = false,
    limit = 20,
    cwd,
  } = params;

  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd(),
  );

  // A targeted thread is private to its senders and recipients. Reading a
  // thread must never create participation: that made a guessed thread id a
  // capability token and let an outsider read, reply, then resolve it.
  if (threadId && !canReadOrJoinThread(db, threadId, agentId)) {
    return { count: 0, signals: [], unread_only: unreadOnly };
  }

  const where: string[] = [];
  const binds: (string | number)[] = [];

  appendSignalScope(where, binds, scope);

  if (threadId) {
    where.push('n.thread_id = ?');
    binds.push(threadId);
    // NOTIF-2: Apply unreadOnly filter for thread fetches too. Previously the threadId
    // branch skipped the LEFT JOIN and status/read checks entirely, returning all messages
    // including already-read ones while still reporting unread_only:true.
    if (unreadOnly) {
      where.push("n.status = 'open'");
      where.push('nr.signal_id IS NULL');
    }
  } else {
    // inbox: addressed to me OR broadcasts (to_agent IS NULL)
    where.push('(n.to_agent IS NULL OR n.to_agent = ?)');
    binds.push(agentId);
    where.push('n.from_agent <> ?');
    binds.push(agentId);

    if (unreadOnly) {
      where.push("n.status = 'open'");
      // NOTIF-1: Replace O(N×M) correlated subquery with a LEFT JOIN. The subquery
      // ran NOT EXISTS(...) for every notification row against signal_reads,
      // making it O(N×M). A LEFT JOIN + IS NULL check is a single hash/merge step.
      where.push('nr.signal_id IS NULL');
      // agentId for the JOIN ON clause is prepended to allBinds below — not added to WHERE binds
    }
  }

  if (kinds.length > 0) {
    where.push(`n.kind IN (${kinds.map(() => '?').join(',')})`);
    binds.push(...kinds);
  }
  if (signalIds.length > 0) {
    where.push(`n.signal_id IN (${signalIds.map(() => '?').join(',')})`);
    binds.push(...signalIds);
  }

  const whereClause = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  // NOTIF-1/NOTIF-2: LEFT JOIN signal_reads whenever unreadOnly is true,
  // regardless of whether threadId is set. The join is needed for the IS NULL check.
  const joinClause = unreadOnly ? SIGNALS_SELECT_LEFT_JOIN_READS : '';
  // Move the agentId bind for the LEFT JOIN to the right position (before WHERE binds)
  const allBinds = unreadOnly
    ? [agentId, ...binds]
    : binds;
  const sql = `
    ${SIGNALS_SELECT_BASE}
    ${joinClause}
    ${whereClause}
    ${SIGNALS_SELECT_ORDER_LIMIT}
  `;
  const boundedLimit = Math.min(200, Math.max(1, Math.floor(Number.isFinite(limit) ? limit : 20)));
  const rows = db.prepare(sql).all(...allBinds, boundedLimit) as unknown as NotificationRow[];
  const signals = rows.map(rowToNotification);

  if (markRead && signals.length > 0) {
    const now = utcNow();
    const insertRead = db.prepare(SIGNAL_READS_INSERT_IGNORE);
    for (const n of signals) {
      insertRead.run(n.signal_id, agentId, now);
    }
  }

  return { count: signals.length, signals, unread_only: unreadOnly };
}

// ─── resolveNotification ───────────────────────────────────────────────────────

export function resolveNotification(
  db: DatabaseSync,
  params: ResolveNotificationParams,
): ResolveNotificationResult {
  const { notificationIds = [], threadId = null, cwd, agentId = null } = params;
  assertSignalsExist(db, notificationIds);
  const hasExplicitScope = params.workspacePath != null || params.artifact != null;
  const scope = hasExplicitScope
    ? fillScope(
      { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
      cwd ?? process.cwd(),
    )
    : { workspace_path: null, artifact: null, repo: null, ref: null };
  const resolved: string[] = [];
  const now = utcNow();

  if (notificationIds.length > 0) {
    const ph = notificationIds.map(() => '?').join(',');
    const where = [`signal_id IN (${ph})`, "status = 'open'"];
    const binds: (string | number)[] = [...notificationIds];
    appendSignalScope(where, binds, scope, '');
    if (agentId) {
      const authorizedIds = notificationIds.filter((signalId) => {
        const row = db.prepare('SELECT thread_id FROM signals WHERE signal_id = ?')
          .get(signalId) as { thread_id: string } | undefined;
        return row ? isThreadParticipant(db, row.thread_id, agentId) : false;
      });
      if (authorizedIds.length === 0) return { resolved: 0, signal_ids: [] };
      where.push(`signal_id IN (${authorizedIds.map(() => '?').join(',')})`);
      binds.push(...authorizedIds);
    }
    const rows = db.prepare(
      `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE ${where.join(' AND ')} RETURNING signal_id`
    ).all(now, ...binds) as unknown as Array<{ signal_id: string }>;
    resolved.push(...rows.map(r => r.signal_id));
  }

  if (threadId) {
    if (agentId && !isThreadParticipant(db, threadId, agentId)) {
      return { resolved: resolved.length, signal_ids: [...new Set(resolved)] };
    }
    const where = ['thread_id = ?', "status = 'open'"];
    const binds: (string | number)[] = [threadId];
    appendSignalScope(where, binds, scope, '');
    const rows = db.prepare(
      `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE ${where.join(' AND ')} RETURNING signal_id`
    ).all(now, ...binds) as unknown as Array<{ signal_id: string }>;
    resolved.push(...rows.map(r => r.signal_id));
  }

  return { resolved: resolved.length, signal_ids: [...new Set(resolved)] };
}

// ─── pruneNotifications ────────────────────────────────────────────────────────

function signalRecord(n: NotificationRecord): AgentSignalRecord {
  return { ...n, to_agents: n.to_agent ? [n.to_agent] : [] };
}

function requireSignalText(value: string | null | undefined, field: string): string {
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error(`agent_signal ${field} is required`);
  }
  return value;
}

/**
 * Explicitly named ids must exist: a typo'd --signal-id otherwise ack/resolves
 * zero rows and reports ok, so the caller believes the signal was handled.
 */
function assertSignalsExist(db: DatabaseSync, signalIds: string[]): void {
  if (signalIds.length === 0) return;
  const unique = [...new Set(signalIds)];
  const rows = db.prepare(
    `SELECT signal_id FROM signals WHERE signal_id IN (${unique.map(() => '?').join(',')})`,
  ).all(...unique) as unknown as Array<{ signal_id: string }>;
  const found = new Set(rows.map((r) => r.signal_id));
  const missing = unique.filter((id) => !found.has(id));
  if (missing.length > 0) {
    throw new Error(`signal(s) not found: ${missing.join(', ')}`);
  }
}

function acknowledgeNotifications(
  db: DatabaseSync,
  agentId: string,
  signalIds: string[] = [],
  threadId: string | null = null,
  params: { workspacePath?: string | null; artifact?: string | null; cwd?: string } = {},
): { acknowledged: number; signal_ids: string[] } {
  assertSignalsExist(db, signalIds);
  const where: string[] = ["status = 'open'", '(to_agent IS NULL OR to_agent = ?)', 'from_agent <> ?'];
  const binds: (string | number)[] = [agentId, agentId];
  if (signalIds.length > 0) {
    where.push(`signal_id IN (${signalIds.map(() => '?').join(',')})`);
    binds.push(...signalIds);
  }
  if (threadId) {
    where.push('thread_id = ?');
    binds.push(threadId);
  }
  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    params.cwd ?? process.cwd(),
  );
  if (signalIds.length === 0) {
    appendSignalScope(where, binds, scope, '');
  }
  const rows = db.prepare(`SELECT signal_id FROM signals WHERE ${where.join(' AND ')}`)
    .all(...binds) as unknown as Array<{ signal_id: string }>;
  const ids = rows.map((r) => r.signal_id);
  const uniqueIds = [...new Set(ids)];
  if (uniqueIds.length === 0) return { acknowledged: 0, signal_ids: [] };

  const now = utcNow();
  const insertRead = db.prepare(SIGNAL_READS_INSERT_IGNORE);
  let acknowledged = 0;
  for (const id of uniqueIds) {
    const result = insertRead.run(id, agentId, now) as { changes: number };
    acknowledged += result.changes;
  }
  return { acknowledged, signal_ids: uniqueIds };
}

export function agentSignal(db: DatabaseSync, params: AgentSignalParams): AgentSignalResult {
  switch (params.action) {
    case 'publish':
    case 'reply': {
      const toAgents = params.toAgents?.length
        ? params.toAgents
        : params.action === 'reply'
          ? inferReplyTargets(db, requireSignalText(params.inReplyTo, 'inReplyTo'), params.agentId)
          : [null];
      const results = toAgents.map((toAgent) => insertNotification(db, {
        agentId: params.agentId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        repo: params.repo,
        ref: params.ref,
        toAgent,
        kind: params.action === 'reply' ? 'reply' : params.kind ?? 'fyi',
        subject: requireSignalText(params.subject, 'subject'),
        body: params.body ?? null,
        files: params.files ?? [],
        refIds: params.refs ?? [],
        inReplyTo: params.inReplyTo ?? null,
        importance: params.importance ?? 5,
        cwd: params.cwd,
      }));
      return {
        action: params.action,
        signal_id: results[0]!.signal_id,
        signal_ids: results.map((r) => r.signal_id),
        thread_id: results[0]!.thread_id,
        workspace_path: results[0]!.workspace_path,
        artifact: results[0]!.artifact,
      };
    }
    case 'list': {
      const result = getNotifications(db, {
        agentId: params.agentId,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        repo: params.repo,
        ref: params.ref,
        kinds: params.kinds ?? [],
        signalIds: params.signalIds ?? [],
        threadId: params.threadId ?? null,
        unreadOnly: params.unreadOnly ?? true,
        markRead: params.markRead ?? false,
        limit: params.limit ?? 20,
        cwd: params.cwd,
      });
      return {
        action: 'list',
        count: result.count,
        signals: result.signals.map(signalRecord),
        unread_only: result.unread_only,
      };
    }
    case 'resolve': {
      const result = resolveNotification(db, {
        agentId: params.agentId,
        notificationIds: params.signalIds ?? [],
        threadId: params.threadId ?? null,
        workspacePath: params.workspacePath,
        artifact: params.artifact,
        cwd: params.cwd,
      });
      return { action: 'resolve', ...result };
    }
    case 'ack': {
      return {
        action: 'ack',
        ...acknowledgeNotifications(db, params.agentId, params.signalIds ?? [], params.threadId ?? null, {
          workspacePath: params.workspacePath,
          artifact: params.artifact,
          cwd: params.cwd,
        }),
      };
    }
  }
}

export function pruneNotifications(
  db: DatabaseSync,
  params: PruneNotificationsParams,
): PruneNotificationsResult {
  const { agentId, notificationIds = [], resolvedOnly = false, olderThanDays, dryRun = false, cwd } = params;

  if (!resolvedOnly) throw new Error('signal prune only deletes resolved messages');
  if (olderThanDays == null || !Number.isFinite(olderThanDays) || olderThanDays < 1) {
    throw new Error('signal prune requires --older-than-days >= 1');
  }

  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    cwd ?? process.cwd(),
  );

  const where: string[] = ["status = 'resolved'", 'created_at < ?'];
  const binds: (string | number)[] = [];
  binds.push(new Date(Date.now() - Math.floor(olderThanDays) * 86400000).toISOString());

  if (notificationIds.length > 0) {
    where.push(`signal_id IN (${notificationIds.map(() => '?').join(',')})`);
    binds.push(...notificationIds);
  }
  appendSignalScope(where, binds, scope, '');

  const whereClause = where.join(' AND ');
  const rows = db.prepare(`SELECT signal_id, thread_id FROM signals WHERE ${whereClause}`)
    .all(...binds) as unknown as Array<{ signal_id: string; thread_id: string }>;
  const ids = rows
    .filter((row) => isThreadParticipant(db, row.thread_id, agentId))
    .map((row) => row.signal_id);

  if (dryRun) {
    return { deleted: 0, dry_run: true, would_delete: ids.length, signal_ids: ids };
  }

  if (ids.length > 0) {
    const ph = ids.map(() => '?').join(',');
    db.prepare(SIGNALS_DELETE_BY_IDS(ph)).run(...ids);
    db.prepare(SIGNAL_READS_DELETE_BY_SIGNAL_IDS(ph)).run(...ids);
  }

  return { deleted: ids.length, signal_ids: ids };
}
