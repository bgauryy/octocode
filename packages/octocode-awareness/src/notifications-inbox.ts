import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact, utcNow } from './helpers.js';
import { readScope } from './git.js';
import { SIGNALS_SELECT_BASE, SIGNALS_SELECT_LEFT_JOIN_READS, SIGNALS_SELECT_ORDER_LIMIT, SIGNAL_READS_INSERT_IGNORE } from './sql/signals.js';
import type { GetNotificationsParams, GetNotificationsResult, ResolveNotificationParams, ResolveNotificationResult } from './types/notifications-agents.js';
import { appendSignalScope, assertSignalsExist, canReadOrJoinThread, isThreadParticipant, NotificationRow, rowToNotification } from './notifications-core.js';
import { decodeSignalCursor, encodeSignalCursor } from './signal-pagination.js';
import { beginWrite } from './db-transaction.js';
import { appendDomainEvent } from './event-outbox.js';

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
  const cursor = decodeSignalCursor(params.cursor);

  const scope = readScope(
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
    // inbox: explicitly addressed to me OR broadcasts. Own broadcasts stay
    // excluded EXCEPT handoffs — a returning session must see its own workspace
    // handoff (identities churn, so handoffs broadcast and bind to no recipient).
    where.push("(n.to_agent = ? OR (n.to_agent IS NULL AND (n.from_agent <> ? OR n.kind = 'handoff')))");
    binds.push(agentId, agentId);

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

  if (cursor) {
    where.push('(n.created_at < ? OR (n.created_at = ? AND n.signal_id < ?))');
    binds.push(cursor.createdAt, cursor.createdAt, cursor.signalId);
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
  const rows = db.prepare(sql).all(...allBinds, boundedLimit + 1) as unknown as NotificationRow[];
  // Inspect the sentinel before marking reads; never consume an undisplayed row.
  const signals = rows.slice(0, boundedLimit).map(rowToNotification);
  const partial = rows.length > boundedLimit;
  const last = signals.at(-1);

  if (markRead && signals.length > 0) {
    const now = utcNow();
    const insertRead = db.prepare(SIGNAL_READS_INSERT_IGNORE);
    for (const n of signals) {
      insertRead.run(n.signal_id, agentId, now);
    }
  }

  return { count: signals.length, signals, unread_only: unreadOnly, partial, partialReasons: partial ? ['limit'] : [],
    ...(partial && last ? { next: { list: { operation: 'agent_signal' as const, request: {
      action: 'list', agent_id: agentId,
      ...(scope.workspace_path ? { workspace_path: scope.workspace_path } : {}),
      ...(scope.artifact ? { artifact: scope.artifact } : {}),
      ...(scope.repo ? { repo: scope.repo } : {}), ...(scope.ref ? { ref: scope.ref } : {}),
      ...(threadId ? { thread_id: threadId } : {}),
      kinds, signal_id: signalIds, unread_only: unreadOnly, mark_read: markRead, limit: boundedLimit,
      cursor: encodeSignalCursor(last.created_at, last.signal_id),
    } } } } : {}),
  };
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
    ? readScope(
      { workspace_path: params.workspacePath ?? null, artifact: normalizeArtifact(params.artifact), repo: null, ref: null },
      cwd ?? process.cwd(),
    )
    : { workspace_path: null, artifact: null, repo: null, ref: null };
  const resolved: string[] = [];
  const now = utcNow();
  const fallbackWorkspace = readScope(
    { workspace_path: null, artifact: null, repo: null, ref: null }, cwd ?? process.cwd(),
  ).workspace_path ?? process.cwd();
  const canResolveThread = !threadId || !agentId || isThreadParticipant(db, threadId, agentId);
  const transaction = beginWrite(db);

  try {
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
        if (authorizedIds.length === 0) {
          transaction.commit();
          return { resolved: 0, signal_ids: [] };
        }
        where.push(`signal_id IN (${authorizedIds.map(() => '?').join(',')})`);
        binds.push(...authorizedIds);
      }
      const rows = db.prepare(
        `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE ${where.join(' AND ')}
         RETURNING signal_id, workspace_path, thread_id`
      ).all(now, ...binds) as unknown as Array<{ signal_id: string; workspace_path: string | null; thread_id: string }>;
      for (const row of rows) {
        resolved.push(row.signal_id);
        appendDomainEvent(db, {
          workspace: row.workspace_path ?? scope.workspace_path ?? fallbackWorkspace,
          eventType: 'peer.message.resolved', retentionClass: 'delivery',
          actorId: agentId ?? 'system', actorKind: agentId ? 'agent' : 'system',
          source: agentId ? 'tool' : 'harness',
          aggregateKind: 'message', aggregateId: row.signal_id, aggregateRevision: now,
          createdAt: now, payload: { thread_id: row.thread_id }, eventIdPrefix: 'revt',
        });
      }
    }

    if (threadId && canResolveThread) {
      const where = ['thread_id = ?', "status = 'open'"];
      const binds: (string | number)[] = [threadId];
      appendSignalScope(where, binds, scope, '');
      const rows = db.prepare(
        `UPDATE signals SET status = 'resolved', resolved_at = ? WHERE ${where.join(' AND ')}
         RETURNING signal_id, workspace_path, thread_id`
      ).all(now, ...binds) as unknown as Array<{ signal_id: string; workspace_path: string | null; thread_id: string }>;
      for (const row of rows) {
        resolved.push(row.signal_id);
        appendDomainEvent(db, {
          workspace: row.workspace_path ?? scope.workspace_path ?? fallbackWorkspace,
          eventType: 'peer.message.resolved', retentionClass: 'delivery',
          actorId: agentId ?? 'system', actorKind: agentId ? 'agent' : 'system',
          source: agentId ? 'tool' : 'harness',
          aggregateKind: 'message', aggregateId: row.signal_id, aggregateRevision: now,
          createdAt: now, payload: { thread_id: row.thread_id }, eventIdPrefix: 'revt',
        });
      }
    }
    transaction.commit();
  } catch (error) {
    try { transaction.rollback(); } catch { /* transaction did not open */ }
    throw error;
  }

  return { resolved: resolved.length, signal_ids: [...new Set(resolved)] };
}
