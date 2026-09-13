import { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import { initDb } from '../src/db-init.js';
import { pruneExpiredNotifications } from '../src/message-lifecycle.js';
import { insertNotification } from '../src/notifications-core.js';

describe('message lifecycle pagination', () => {
  it('prunes a thread leaf-first without starving its parent', () => {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    const parent = insertNotification(db, {
      agentId: 'agent-a', toAgent: 'agent-b', kind: 'decision', subject: 'parent', workspacePath: '/repo',
    });
    insertNotification(db, {
      agentId: 'agent-b', toAgent: 'agent-a', kind: 'reply', subject: 'child',
      inReplyTo: parent.signal_id, workspacePath: '/repo',
    });
    db.prepare('UPDATE signals SET expires_at = ?').run('2020-01-01T00:00:00.000Z');

    const first = pruneExpiredNotifications(db, {
      now: '2021-01-01T00:00:00.000Z', limit: 1, workspacePath: '/repo',
    });
    expect(first).toMatchObject({ transitioned: 1, deleted: 1, partial: true });
    const second = pruneExpiredNotifications(db, first.next!.pruneExpired);
    expect(second).toMatchObject({ transitioned: 1, deleted: 1 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM signals').get()).toEqual({ count: 0 });
    db.close();
  });
});
