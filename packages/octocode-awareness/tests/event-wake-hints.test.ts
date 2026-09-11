import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFile } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, expect, it, vi } from 'vitest';
import { openAwarenessStore } from '../src/coordination/open.js';
import { watchAwarenessEventHints } from '../src/event-wake-hints.js';

const roots: string[] = [];
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

it('wakes for committed outbox changes, ignores read receipts, and closes without another callback', async () => {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-idle-hints-')));
  roots.push(root);
  const database = join(root, 'awareness.sqlite3');
  const store = openAwarenessStore({ workspace: root, dbPath: database });
  const onHint = vi.fn();
  const onError = vi.fn();
  const watcher = watchAwarenessEventHints({ database, onHint, onError });
  const prepare = vi.spyOn(DatabaseSync.prototype, 'prepare');
  try {
    await new Promise(resolve => setTimeout(resolve, 100));
    expect(onError.mock.calls.map(([error]) => String(error))).toEqual([]);
    expect(onHint).not.toHaveBeenCalled();
    store.sendMessage({ fromAgentId: 'sender', toAgentId: 'recipient', text: 'first' });
    await vi.waitFor(() => expect(onHint).toHaveBeenCalledTimes(1));
    const [event] = store.listEvents({ consumerId: 'recipient' });
    store.acknowledgeEvent({ consumerId: 'recipient', eventId: event!.eventId, decision: 'accept' });
    await new Promise(resolve => setTimeout(resolve, 200));
    expect(onHint).toHaveBeenCalledTimes(1);
    expect(onError).not.toHaveBeenCalled();
    const checks = () => prepare.mock.calls.filter(([sql]) => sql === 'SELECT MAX(sequence) AS sequence FROM event_outbox').length;
    const settledChecks = checks();
    await new Promise(resolve => setTimeout(resolve, 150));
    expect(checks()).toBe(settledChecks);
    // A different process commits while the recipient has no lifecycle activity.
    await new Promise<void>((done, reject) => execFile(process.execPath, ['--input-type=module', '-e',
      `import { DatabaseSync } from 'node:sqlite';
       const db = new DatabaseSync(process.argv[1]);
       db.prepare("INSERT INTO event_outbox(event_id,workspace_path,event_type,actor_json,provenance_json,payload_json,created_at) SELECT 'child-event',workspace_path,event_type,actor_json,provenance_json,payload_json,created_at FROM event_outbox LIMIT 1").run();
       db.close();`, database], error => error ? reject(error) : done()));
    await vi.waitFor(() => expect(onHint).toHaveBeenCalledTimes(2));
    watcher.close();
    store.sendMessage({ fromAgentId: 'sender', toAgentId: 'recipient', text: 'after close' });
    await new Promise(resolve => setTimeout(resolve, 100));
    expect(onHint).toHaveBeenCalledTimes(2);
  } finally { watcher.close(); store.close(); }
});

it('keeps sparse failed-watch recovery armed until the watcher is closed', async () => {
  vi.useFakeTimers();
  const root = mkdtempSync(join(tmpdir(), 'awareness-watch-failure-'));
  roots.push(root);
  const onError = vi.fn();
  const onHint = vi.fn();
  const watcher = watchAwarenessEventHints({ database: join(root, 'missing-directory', 'missing.sqlite3'), onError, onHint });
  await vi.advanceTimersByTimeAsync(2_600);
  expect(onError).toHaveBeenCalledTimes(4);
  expect(onHint).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(1);
  await vi.advanceTimersByTimeAsync(30_000);
  expect(onError).toHaveBeenCalledTimes(5);
  expect(vi.getTimerCount()).toBe(1);
  watcher.close();
  expect(vi.getTimerCount()).toBe(0);
});
