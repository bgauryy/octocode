import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { DatabaseSync } from '@octocodeai/octocode-shared/sqlite';
import { initDb } from '../src/db-init.js';
import { insertNotification } from '../src/notifications-core.js';
import { acknowledgeNotifications, pruneNotifications } from '../src/notifications-signals.js';

describe('signal entity connections', () => {
  let db: DatabaseSync;
  beforeEach(() => { db = new DatabaseSync(':memory:'); initDb(db); });
  afterEach(() => { db.close(); });

  it('rejects a valid parent ID from another workspace without inserting a reply', () => {
    const parent = insertNotification(db, {
      agentId: 'alice', kind: 'fyi', subject: 'Workspace A', workspacePath: '/tmp/entity-a',
    });
    expect(() => insertNotification(db, {
      agentId: 'bob', kind: 'reply', subject: 'Workspace B', workspacePath: '/tmp/entity-b',
      inReplyTo: parent.signal_id,
    })).toThrow(/parent signal.*workspace/i);
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals').get()).toEqual({ count: 1 });
  });

  it('preserves parent and thread IDs for replies in the same normalized workspace', () => {
    const parent = insertNotification(db, {
      agentId: 'alice', kind: 'fyi', subject: 'Parent', workspacePath: '/tmp/entity-a',
    });
    const reply = insertNotification(db, {
      agentId: 'bob', kind: 'reply', subject: 'Reply', workspacePath: '/tmp/entity-a/./',
      inReplyTo: parent.signal_id,
    });
    expect(reply.thread_id).toBe(parent.signal_id);
    expect(reply.workspace_path).toBe(parent.workspace_path);
    expect(db.prepare('SELECT reply_to FROM signals WHERE signal_id = ?').get(reply.signal_id))
      .toEqual({ reply_to: parent.signal_id });
  });
  it('keeps explicit acknowledgements inside the requested workspace', () => {
    const message = insertNotification(db, { agentId: 'alice', toAgent: 'bob', kind: 'fyi', subject: 'A', workspacePath: '/tmp/entity-a' });
    expect(acknowledgeNotifications(db, 'bob', [message.signal_id], null, { workspacePath: '/tmp/entity-b' }).acknowledged).toBe(0);
    expect(acknowledgeNotifications(db, 'bob', [message.signal_id], null, { workspacePath: '/tmp/entity-a' }).acknowledged).toBe(1);
  });

  it('retains the complete ancestry of a live reply, then prunes the resolved thread together', () => {
    const root = insertNotification(db, { agentId: 'alice', kind: 'fyi', subject: 'Root', workspacePath: '/tmp/entity-a' });
    const parent = insertNotification(db, { agentId: 'bob', kind: 'reply', subject: 'Parent', inReplyTo: root.signal_id, workspacePath: '/tmp/entity-a' });
    const leaf = insertNotification(db, { agentId: 'alice', kind: 'reply', subject: 'Leaf', inReplyTo: parent.signal_id, workspacePath: '/tmp/entity-a' });
    db.prepare("UPDATE signals SET status = 'resolved', created_at = '2020-01-01T00:00:00Z' WHERE signal_id != ?").run(leaf.signal_id);
    const params = { agentId: 'alice', workspacePath: '/tmp/entity-a', resolvedOnly: true, olderThanDays: 1 };
    expect(pruneNotifications(db, { ...params, dryRun: true }).would_delete).toBe(0);
    expect(pruneNotifications(db, params).deleted).toBe(0);
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals').get()).toEqual({ count: 3 });
    db.exec("UPDATE signals SET status = 'resolved', created_at = '2020-01-01T00:00:00Z'");
    expect(pruneNotifications(db, params).deleted).toBe(3);
  });
});
