/**
 * notifications.ts — Agent-to-agent workspace messaging.
 *
 * Mirrors Python awareness.py's notify / notify-get / notify-resolve / notify-prune.
 * Uses the `signals` + `signal_reads` tables in the schema.
 */

import { randomUUID } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow, parseJsonList } from './helpers.js';
import { fillScope } from './git.js';
import {
  SIGNALS_SELECT_THREAD_ID,
  SIGNALS_INSERT,
  SIGNALS_SELECT_BASE,
  SIGNALS_SELECT_LEFT_JOIN_READS,
  SIGNALS_SELECT_ORDER_LIMIT,
  SIGNALS_DELETE_BY_IDS,
  SIGNALS_SELECT_IDS_FOR_PRUNE,
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
    threadId = parent.thread_id;
  } else {
    threadId = signalId;
  }

  db.prepare(SIGNALS_INSERT).run(
    signalId, wsPath, scope.artifact, scope.repo, scope.ref,
    agentId, toAgent, kind, subject, body,
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
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: params.repo ?? null, ref: params.ref ?? null },
    cwd ?? process.cwd(),
  );

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
  const rows = db.prepare(sql).all(...allBinds, limit) as unknown as NotificationRow[];
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
      where.push('(from_agent = ? OR to_agent = ? OR to_agent IS NULL)');
      binds.push(agentId, agentId);
    }
    const rows = db.prepare(
      `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE ${where.join(' AND ')} RETURNING signal_id`
    ).all(now, ...binds) as unknown as Array<{ signal_id: string }>;
    resolved.push(...rows.map(r => r.signal_id));
  }

  if (threadId) {
    const where = ['thread_id = ?', "status = 'open'"];
    const binds: (string | number)[] = [threadId];
    appendSignalScope(where, binds, scope, '');
    if (agentId) {
      where.push('(from_agent = ? OR to_agent = ? OR to_agent IS NULL)');
      binds.push(agentId, agentId);
    }
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

function acknowledgeNotifications(
  db: DatabaseSync,
  agentId: string,
  signalIds: string[] = [],
  threadId: string | null = null,
  params: { workspacePath?: string | null; artifact?: string | null; cwd?: string } = {},
): { acknowledged: number; signal_ids: string[] } {
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
      const toAgents = params.toAgents?.length ? params.toAgents : [null];
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
  const { notificationIds = [], resolvedOnly = false, olderThanDays, dryRun = false, cwd } = params;

  const scope = fillScope(
    { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
    cwd ?? process.cwd(),
  );

  const where: string[] = [];
  const binds: (string | number)[] = [];

  if (notificationIds.length > 0) {
    where.push(`signal_id IN (${notificationIds.map(() => '?').join(',')})`);
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
  if (notificationIds.length === 0) {
    appendSignalScope(where, binds, scope, '');
  }

  if (where.length === 0) {
    return { deleted: 0, signal_ids: [] };
  }

  const whereClause = where.join(' AND ');
  const rows = db.prepare(
    `${SIGNALS_SELECT_IDS_FOR_PRUNE} ${whereClause}`
  ).all(...binds) as unknown as Array<{ signal_id: string }>;
  const ids = rows.map(r => r.signal_id);

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
