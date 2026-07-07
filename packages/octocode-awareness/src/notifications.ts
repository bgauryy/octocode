/**
 * notifications.ts — Agent-to-agent workspace messaging.
 *
 * Mirrors Python awareness.py's notify / notify-get / notify-resolve / notify-prune.
 * Uses the `notifications` + `notification_reads` tables already in the schema.
 */

import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { utcNow, parseJsonList } from './helpers.js';
import { fillScope } from './git.js';
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
  notification_id: string;
  workspace_path: string;
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
  in_reply_to: string | null;
  importance: number;
  status: string;
  created_at: string;
}

function rowToNotification(r: NotificationRow): NotificationRecord {
  return {
    notification_id: r.notification_id,
    workspace_path: r.workspace_path,
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
    in_reply_to: r.in_reply_to,
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

  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd(),
  );

  const notificationId = 'ntf_' + randomUUID().replace(/-/g, '');
  const createdAt = utcNow();
  const wsPath = scope.workspace_path ?? process.cwd();

  // Thread: inherit from parent or start new
  let threadId: string;
  if (inReplyTo) {
    const parent = db.prepare(
      'SELECT thread_id FROM notifications WHERE notification_id = ?'
    ).get(inReplyTo) as { thread_id: string } | undefined;
    if (!parent) {
      throw new Error(`insertNotification: parent notification ${inReplyTo} not found (deleted?). Omit inReplyTo to start a new thread.`);
    }
    threadId = parent.thread_id;
  } else {
    threadId = notificationId;
  }

  db.prepare(
    `INSERT INTO notifications
     (notification_id, workspace_path, repo, ref, from_agent, to_agent, kind, subject, body,
      files_json, refs_json, thread_id, in_reply_to, importance, status, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', ?)`
  ).run(
    notificationId, wsPath, scope.repo, scope.ref,
    agentId, toAgent, kind, subject, body,
    JSON.stringify(files), JSON.stringify(refIds),
    threadId, inReplyTo, importance, createdAt,
  );

  return { notification_id: notificationId, thread_id: threadId, workspace_path: wsPath };
}

// ─── getNotifications ──────────────────────────────────────────────────────────

export function getNotifications(
  db: DatabaseSync,
  params: GetNotificationsParams,
): GetNotificationsResult {
  const {
    agentId,
    kinds = [],
    threadId = null,
    unreadOnly = true,
    markRead = false,
    limit = 20,
    cwd,
  } = params;

  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd(),
  );

  const where: string[] = [];
  const binds: (string | number)[] = [];

  if (scope.workspace_path) {
    where.push('(n.workspace_path = ? OR n.workspace_path IS NULL)');
    binds.push(scope.workspace_path);
  }

  if (threadId) {
    where.push('n.thread_id = ?');
    binds.push(threadId);
    // NOTIF-2: Apply unreadOnly filter for thread fetches too. Previously the threadId
    // branch skipped the LEFT JOIN and status/read checks entirely, returning all messages
    // including already-read ones while still reporting unread_only:true.
    if (unreadOnly) {
      where.push("n.status = 'open'");
      where.push('nr.notification_id IS NULL');
    }
  } else {
    // inbox: addressed to me OR broadcasts (to_agent IS NULL)
    where.push('(n.to_agent IS NULL OR n.to_agent = ?)');
    binds.push(agentId);

    if (unreadOnly) {
      where.push("n.status = 'open'");
      // NOTIF-1: Replace O(N×M) correlated subquery with a LEFT JOIN. The subquery
      // ran NOT EXISTS(...) for every notification row against notification_reads,
      // making it O(N×M). A LEFT JOIN + IS NULL check is a single hash/merge step.
      where.push('nr.notification_id IS NULL');
      // agentId for the JOIN ON clause is prepended to allBinds below — not added to WHERE binds
    }
  }

  if (kinds.length > 0) {
    where.push(`n.kind IN (${kinds.map(() => '?').join(',')})`);
    binds.push(...kinds);
  }

  const whereClause = where.length > 0 ? `WHERE ${where.join(' AND ')}` : '';
  // NOTIF-1/NOTIF-2: LEFT JOIN notification_reads whenever unreadOnly is true,
  // regardless of whether threadId is set. The join is needed for the IS NULL check.
  const joinClause = unreadOnly
    ? `LEFT JOIN notification_reads nr ON nr.notification_id = n.notification_id AND nr.agent_id = ?`
    : '';
  // Move the agentId bind for the LEFT JOIN to the right position (before WHERE binds)
  const allBinds = unreadOnly
    ? [agentId, ...binds]
    : binds;
  const sql = `
    SELECT n.* FROM notifications n
    ${joinClause}
    ${whereClause}
    ORDER BY n.created_at DESC
    LIMIT ?
  `;
  const rows = db.prepare(sql).all(...allBinds, limit) as unknown as NotificationRow[];
  const notifications = rows.map(rowToNotification);

  if (markRead && notifications.length > 0) {
    const now = utcNow();
    const insertRead = db.prepare(
      'INSERT OR IGNORE INTO notification_reads(notification_id, agent_id, read_at) VALUES (?, ?, ?)'
    );
    for (const n of notifications) {
      insertRead.run(n.notification_id, agentId, now);
    }
  }

  return { count: notifications.length, notifications, unread_only: unreadOnly };
}

// ─── resolveNotification ───────────────────────────────────────────────────────

export function resolveNotification(
  db: DatabaseSync,
  params: ResolveNotificationParams,
): ResolveNotificationResult {
  const { notificationIds = [], threadId = null } = params;
  const resolved: string[] = [];
  const now = utcNow();

  if (notificationIds.length > 0) {
    const ph = notificationIds.map(() => '?').join(',');
    const rows = db.prepare(
      `UPDATE notifications SET status = 'resolved' WHERE notification_id IN (${ph}) AND status = 'open' RETURNING notification_id`
    ).all(...notificationIds) as unknown as Array<{ notification_id: string }>;
    resolved.push(...rows.map(r => r.notification_id));
  }

  if (threadId) {
    const rows = db.prepare(
      `UPDATE notifications SET status = 'resolved' WHERE thread_id = ? AND status = 'open' RETURNING notification_id`
    ).all(threadId) as unknown as Array<{ notification_id: string }>;
    resolved.push(...rows.map(r => r.notification_id));
  }

  void now; // timestamp available if needed for audit
  return { resolved: resolved.length, notification_ids: [...new Set(resolved)] };
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

function acknowledgeNotifications(
  db: DatabaseSync,
  agentId: string,
  notificationIds: string[] = [],
  threadId: string | null = null,
): { acknowledged: number; notification_ids: string[] } {
  const ids = [...notificationIds];
  if (threadId) {
    const rows = db.prepare(
      `SELECT notification_id FROM notifications
       WHERE thread_id = ? AND status = 'open' AND (to_agent IS NULL OR to_agent = ?)`
    ).all(threadId, agentId) as unknown as Array<{ notification_id: string }>;
    ids.push(...rows.map((r) => r.notification_id));
  }
  const uniqueIds = [...new Set(ids)];
  if (uniqueIds.length === 0) return { acknowledged: 0, notification_ids: [] };

  const now = utcNow();
  const insertRead = db.prepare(
    'INSERT OR IGNORE INTO notification_reads(notification_id, agent_id, read_at) VALUES (?, ?, ?)'
  );
  let acknowledged = 0;
  for (const id of uniqueIds) {
    const result = insertRead.run(id, agentId, now) as { changes: number };
    acknowledged += result.changes;
  }
  return { acknowledged, notification_ids: uniqueIds };
}

export function agentSignal(db: DatabaseSync, params: AgentSignalParams): AgentSignalResult {
  switch (params.action) {
    case 'publish':
    case 'reply': {
      const toAgents = params.toAgents?.length ? params.toAgents : [null];
      const results = toAgents.map((toAgent) => insertNotification(db, {
        agentId: params.agentId,
        workspacePath: params.workspacePath,
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
        notification_id: results[0]!.notification_id,
        notification_ids: results.map((r) => r.notification_id),
        thread_id: results[0]!.thread_id,
        workspace_path: results[0]!.workspace_path,
      };
    }
    case 'list': {
      const result = getNotifications(db, {
        agentId: params.agentId,
        workspacePath: params.workspacePath,
        repo: params.repo,
        ref: params.ref,
        kinds: params.kinds ?? [],
        threadId: params.threadId ?? null,
        unreadOnly: params.unreadOnly ?? true,
        markRead: params.markRead ?? false,
        limit: params.limit ?? 20,
        cwd: params.cwd,
      });
      return {
        action: 'list',
        count: result.count,
        signals: result.notifications.map(signalRecord),
        unread_only: result.unread_only,
      };
    }
    case 'resolve': {
      const result = resolveNotification(db, {
        notificationIds: params.notificationIds ?? [],
        threadId: params.threadId ?? null,
        workspacePath: params.workspacePath,
        cwd: params.cwd,
      });
      return { action: 'resolve', ...result };
    }
    case 'ack': {
      return {
        action: 'ack',
        ...acknowledgeNotifications(db, params.agentId, params.notificationIds ?? [], params.threadId ?? null),
      };
    }
  }
}

export function pruneNotifications(
  db: DatabaseSync,
  params: PruneNotificationsParams,
): PruneNotificationsResult {
  const { notificationIds = [], resolvedOnly = false, olderThanDays, dryRun = false, cwd } = params;

  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, repo: null, ref: null },
    cwd ?? process.cwd(),
  );

  const where: string[] = [];
  const binds: (string | number)[] = [];

  if (notificationIds.length > 0) {
    where.push(`notification_id IN (${notificationIds.map(() => '?').join(',')})`);
    binds.push(...notificationIds);
  }
  if (resolvedOnly) {
    where.push("status = 'resolved'");
  }
  if (olderThanDays != null) {
    const cutoff = new Date(Date.now() - olderThanDays * 86400000).toISOString();
    where.push('created_at < ?');
    binds.push(cutoff);
  }
  if (scope.workspace_path && notificationIds.length === 0) {
    where.push('(workspace_path = ? OR workspace_path IS NULL)');
    binds.push(scope.workspace_path);
  }

  if (where.length === 0) {
    return { deleted: 0, notification_ids: [] };
  }

  const whereClause = where.join(' AND ');
  const rows = db.prepare(
    `SELECT notification_id FROM notifications WHERE ${whereClause}`
  ).all(...binds) as unknown as Array<{ notification_id: string }>;
  const ids = rows.map(r => r.notification_id);

  if (dryRun) {
    return { deleted: 0, dry_run: true, would_delete: ids.length, notification_ids: ids };
  }

  if (ids.length > 0) {
    const ph = ids.map(() => '?').join(',');
    db.prepare(`DELETE FROM notifications WHERE notification_id IN (${ph})`).run(...ids);
    db.prepare(`DELETE FROM notification_reads WHERE notification_id IN (${ph})`).run(...ids);
  }

  return { deleted: ids.length, notification_ids: ids };
}
