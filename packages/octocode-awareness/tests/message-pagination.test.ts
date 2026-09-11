import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../src/coordination/open.js';
import { readExternalAwarenessStatus } from '../src/coordination/external-status.js';
import { executeAwarenessCommand } from '../src/command-api.js';
import { connectDb } from '../src/db-runtime.js';
import { agentSignal } from '../src/notifications-signals.js';

let root: string;
let previousHome: string | undefined;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'aw-message-pages-'));
  previousHome = process.env.OCTOCODE_HOME;
  process.env.OCTOCODE_HOME = join(root, 'home');
});
afterEach(() => {
  if (previousHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = previousHome;
  rmSync(root, { recursive: true, force: true });
});

function fixture(count: number) {
  const store = openAwarenessStore({ workspace: root });
  const ids = Array.from({ length: count }, (_, index) => store.sendMessage({
    fromAgentId: 'sender', toAgentId: index % 2 ? 'reader' : null,
    topic: 'fixture', text: `message ${index}`,
  }).messageId);
  store.sendMessage({ fromAgentId: 'sender', toAgentId: 'someone-else', topic: 'fixture', text: 'private' });
  store.sendMessage({ fromAgentId: 'reader', topic: 'fixture', text: 'own broadcast' });
  // Equal timestamps force the secondary key to carry lossless pagination.
  const db = connectDb(store.dbPath);
  db.prepare('UPDATE signals SET created_at = ?').run('2026-01-01T00:00:00.000Z');
  db.close();
  return { store, ids };
}

describe('lossless recipient inbox pagination', () => {
  it('counts the entire unread inbox instead of the capped preview', () => {
    const { store, ids } = fixture(125);
    try {
      expect(readExternalAwarenessStatus({ workspace: root, agentId: 'reader' }).unreadInbox).toBe(125);
      store.markMessageRead({ messageId: ids[0]!, agentId: 'reader' });
      expect(readExternalAwarenessStatus({ workspace: root, agentId: 'reader' }).unreadInbox).toBe(124);
    } finally { store.close(); }
  });

  it.each([false, true])('executes API continuations with markRead=%s across tied timestamps', async (markRead) => {
    const { store, ids } = fixture(231);
    try {
      let command = 'signal list';
      let params: Record<string, unknown> = { limit: 50, mark_read: markRead };
      const seen: string[] = [];
      for (let page = 0; page < 10; page++) {
        const execution = await executeAwarenessCommand(
          { command, params },
          { database: store.dbPath, workspace: root, agentId: 'reader', continuations: 'api' },
        );
        expect(execution.exitCode, JSON.stringify(execution.payload)).toBe(0);
        const result = execution.payload as {
          signals: Array<{ signal_id: string }>; partial?: boolean; partialReasons?: string[];
          next?: { list: { command: { command: string; params: Record<string, unknown> } } };
        };
        seen.push(...result.signals.map(signal => signal.signal_id));
        if (!result.partial) break;
        expect(result.partialReasons).toEqual(['limit']);
        ({ command, params } = result.next!.list.command);
      }
      expect(new Set(seen).size).toBe(seen.length);
      expect([...seen].sort()).toEqual([...ids].sort());
      expect(readExternalAwarenessStatus({ workspace: root, agentId: 'reader' }).unreadInbox).toBe(markRead ? 0 : ids.length);
    } finally { store.close(); }
  });

  it('preserves targeted filters through the public operation continuation and rejects malformed cursors', async () => {
    const { store, ids } = fixture(231);
    const db = connectDb(store.dbPath);
    try {
      const selected = ids.slice(0, 111);
      let command = 'signal list';
      let params: Record<string, unknown> = { signal_id: selected, limit: 50 };
      const seen: string[] = [];
      for (let page = 0; page < 5; page++) {
        const execution = await executeAwarenessCommand(
          { command, params },
          { database: store.dbPath, workspace: root, agentId: 'reader', continuations: 'api' },
        );
        expect(execution.exitCode, JSON.stringify(execution.payload)).toBe(0);
        const result = execution.payload as {
          signals: Array<{ signal_id: string }>; partial?: boolean;
          next?: { list: { command: { command: string; params: Record<string, unknown> } } };
        };
        seen.push(...result.signals.map(signal => signal.signal_id));
        if (!result.partial) break;
        ({ command, params } = result.next!.list.command);
      }
      expect([...seen].sort()).toEqual([...selected].sort());
      expect(() => store.listMessagesPage({ agentId: 'reader', cursor: 'malformed' })).toThrow(/cursor/i);
      expect(() => agentSignal(db, { action: 'list', agentId: 'reader', workspacePath: root, cursor: 'malformed' })).toThrow(/cursor/i);
    } finally { db.close(); store.close(); }
  });

  it.each([false, true])('executes rebuilt CLI continuations without consuming hidden rows (compact=%s)', (compact) => {
    const { store, ids } = fixture(231);
    try {
      const cli = resolve(import.meta.dirname, '../out/octocode-awareness.js');
      let args = ['signal', 'list', '--db', store.dbPath, '--workspace', root, '--agent-id', 'reader', '--limit', '50', '--mark-read', ...(compact ? ['--compact'] : ['--include-bodies'])];
      const seen: string[] = [];
      for (let page = 0; page < 10; page++) {
        const execution = spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8', timeout: 10000 });
        expect(execution.status, execution.stderr).toBe(0);
        const result = JSON.parse(execution.stdout);
        seen.push(...result.signals.map((signal: { signal_id: string }) => signal.signal_id));
        expect(readExternalAwarenessStatus({ workspace: root, agentId: 'reader' }).unreadInbox).toBe(ids.length - seen.length);
        if (!result.partial) break;
        expect(result.partialReasons).toEqual(['limit']);
        expect(result.next.list.command.name).toBe('signal list');
        args = ['signal', 'list', ...result.next.list.command.args];
      }
      expect(new Set(seen).size).toBe(seen.length);
      expect([...seen].sort()).toEqual([...ids].sort());
    } finally { store.close(); }
  });
});
