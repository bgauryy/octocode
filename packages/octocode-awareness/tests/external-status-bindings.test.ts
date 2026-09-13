import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, it } from 'vitest';
import { openAwarenessStore, readExternalAwarenessStatus } from '../src/host-api.js';

it('reads inbox counts from the exact host database and actor', () => {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'aw-status-bindings-')));
  const dbPath = join(workspace, 'host.sqlite3');
  const store = openAwarenessStore({ workspace, dbPath });
  try {
    store.sendMessage({ fromAgentId: 'sender', toAgentId: 'reader', text: 'Review ready' });
    const status = readExternalAwarenessStatus({ workspace, dbPath, agentId: 'reader' });
    expect(status.unreadInbox).toBe(1);
    expect(status.lastInbound?.preview).toBe('Review ready');
    expect(readExternalAwarenessStatus({ workspace, dbPath, agentId: 'other' }).unreadInbox).toBe(0);
  } finally {
    store.close();
    rmSync(workspace, { recursive: true, force: true });
  }
});
