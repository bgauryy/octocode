import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db.js';
import { insertNotification, getNotifications, resolveNotification, pruneNotifications } from '../src/notifications.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('notifications', () => {
  it('default unread inbox excludes resolved notifications', () => {
    const db = freshDb();
    const first = insertNotification(db, {
      agentId: 'agent-a',
      toAgent: 'agent-b',
      kind: 'handoff',
      subject: 'done',
      workspacePath: '/repo',
    });
    insertNotification(db, {
      agentId: 'agent-a',
      toAgent: 'agent-b',
      kind: 'handoff',
      subject: 'still open',
      workspacePath: '/repo',
    });

    resolveNotification(db, { notificationIds: [first.notification_id] });

    const unread = getNotifications(db, { agentId: 'agent-b', workspacePath: '/repo' });
    expect(unread.notifications).toHaveLength(1);
    expect(unread.notifications[0]!.subject).toBe('still open');
    expect(unread.notifications[0]!.status).toBe('open');

    const all = getNotifications(db, { agentId: 'agent-b', workspacePath: '/repo', unreadOnly: false });
    expect(all.notifications.map(n => n.status)).toEqual(expect.arrayContaining(['open', 'resolved']));
  });

  it('prunes explicit notification ids regardless of inferred workspace', () => {
    const db = freshDb();
    const notification = insertNotification(db, {
      agentId: 'agent-a',
      toAgent: 'agent-b',
      kind: 'request',
      subject: 'please verify',
      workspacePath: '/repo-a',
    });

    const dryRun = pruneNotifications(db, {
      notificationIds: [notification.notification_id],
      workspacePath: '/repo-b',
      dryRun: true,
    });
    expect(dryRun.would_delete).toBe(1);
    expect(dryRun.notification_ids).toEqual([notification.notification_id]);

    const deleted = pruneNotifications(db, {
      notificationIds: [notification.notification_id],
      workspacePath: '/repo-b',
    });
    expect(deleted.deleted).toBe(1);
  });
});
