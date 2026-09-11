import { afterEach, expect, it } from 'vitest';
import { existsSync, mkdtempSync, mkdirSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectDb } from '../src/db-runtime.js';
import { createAwarenessClient } from '../src/client.js';
import { runAwarenessHistoryOperation } from '../src/history-api.js';
import { HistoryError } from '../src/history-store.js';
import { createHistoryContext } from '../src/history-store.js';
import { historyInspect } from '../src/history-query.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true })));
it('inspects and reads linked history through executable caller-bound pages without granting restore authority', async () => {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'history-source-'))); roots.push(root);
  const main = join(root, 'main'); const peer = join(root, 'peer'); const clone = join(root, 'clone');
  const git = (cwd: string, ...args: string[]) => execFileSync('git', ['-C', cwd, ...args], { stdio: 'pipe' });
  mkdirSync(main); git(main, 'init', '-q', '-b', 'main');
  git(main, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--allow-empty', '-qm', 'seed');
  git(main, 'worktree', 'add', '-qb', 'peer', peer); git(root, 'clone', '-q', main, clone);
  for (const name of ['a', 'b', 'unicode-λ']) writeFileSync(join(main, name), `bytes:${name}`);
  const database = join(root, 'ledger.sqlite3');
  const runHistory = async (callerWorkspace: string, command: string, params: Record<string, unknown>) => {
    const db = connectDb(database);
    try {
      return await runAwarenessHistoryOperation(db, command.replace(/^history /, ''), { workspace: callerWorkspace, ...params });
    } finally { db.close(); }
  };
  const captured = await runHistory(main, 'checkpoint', {
    agent_id: 'reader', operation_id: 'source', file: ['a', 'b', 'unicode-λ'],
  });
  expect(captured).toMatchObject({ ok: true });
  const status = await createAwarenessClient({ workspace: main, database, agentId: 'reader' })
    .execute({ operation: 'history.status' });
  expect(status.exitCode, JSON.stringify(status.payload)).toBe(0);
  const storage = (status.payload as { storage: { root: string; git_dir: string } }).storage;
  const archivePaths = [storage.root, join(storage.root, 'history-store.json'), join(storage.git_dir, 'config'), join(storage.git_dir, 'HEAD')];
  const archiveState = () => archivePaths.map(path => { const value = statSync(path); return [value.mode, value.mtimeMs, value.ctimeMs]; });
  const beforeRead = archiveState();
  type HistoryRequest = { command: string; args: Record<string, unknown> };
  type HistoryReadCall = { operation: 'history.read'; params: Record<string, unknown> };
  let request: HistoryRequest | undefined = {
    command: 'inspect', args: { source_workspace: main, operation_id: 'source', limit: 1 },
  };
  const files: string[] = [];
  let read: HistoryReadCall | undefined;
  while (request) {
    const payload = await runHistory(peer, request.command, request.args) as {
      rows: Array<{ file_path: string; next?: { after?: HistoryReadCall } }>;
      next?: HistoryRequest;
    };
    for (const row of payload.rows) { files.push(row.file_path); read ??= row.next?.after; }
    request = payload.next;
    if (request) expect(request.args).toMatchObject({ workspace: peer, source_workspace: main });
  }
  expect(files).toEqual(['a', 'b', 'unicode-λ']);
  expect(read?.operation).toBe('history.read');
  read = { operation: 'history.read', params: { ...read!.params, limit: 2 } };
  const bytes: Buffer[] = [];
  const reader = createAwarenessClient({ workspace: peer, database, agentId: 'reader' });
  while (read) {
    const execution = await reader.execute(read);
    expect(execution.exitCode, JSON.stringify(execution.payload)).toBe(0);
    const payload = execution.payload as { content: string; next?: typeof read };
    bytes.push(Buffer.from(payload.content, 'base64')); read = payload.next;
    if (read) expect(read.params).toMatchObject({ source_workspace: main });
  }
  expect(Buffer.concat(bytes).toString()).toBe('bytes:a');
  expect(archiveState()).toEqual(beforeRead);
  for (const command of ['history inspect', 'history read']) {
    await expect(runHistory(clone, command, {
      operation_id: 'source', source_workspace: main,
      ...(command === 'history read' ? { file: 'a', side: 'after' } : {}),
    })).rejects.toMatchObject({ code: 'HISTORY_SOURCE_WORKSPACE' } satisfies Partial<HistoryError>);
  }
  const restore = await createAwarenessClient({ workspace: peer, database, agentId: 'reader' }).execute({
    operation: 'history.restore', params: { source_workspace: main, operation_id: 'source', side: 'after' },
  });
  expect(restore.exitCode).toBe(1);
  renameSync(storage.root, `${storage.root}.retained`);
  await expect(runHistory(peer, 'read', {
    source_workspace: main, operation_id: 'source', file: 'a', side: 'after',
  })).rejects.toThrow('HISTORY_STORE_UNAVAILABLE');
  expect(existsSync(storage.root)).toBe(false);
});

it('rejects cross-caller and malformed inspect cursors and restarts changed operation snapshots', () => {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'history-inspect-cursor-'))); roots.push(root);
  const db = connectDb(join(root, 'ledger.sqlite3'));
  const ctx = createHistoryContext(db, root);
  const now = new Date().toISOString();
  try {
    db.prepare(`INSERT INTO local_history_operations
      (operation_id,workspace_path,agent_id,kind,status,outcome,request_hash,created_at,updated_at)
      VALUES (?,?,?,'checkpoint','complete','unknown',?,?,?)`).run('source', root, 'reader', 'request', now, now);
    for (const [ordinal, path] of ['a', 'b', 'c'].entries()) {
      db.prepare('INSERT INTO local_history_versions (operation_id,file_path,ordinal) VALUES (?,?,?)').run('source', path, ordinal);
    }
    const input = { workspace: root, operation_id: 'source', limit: 1 };
    const first = historyInspect(ctx, input);
    const cursor = (first.next?.args as Record<string, unknown> | undefined)?.cursor as string;
    expect(cursor).toBeTypeOf('string');
    expect(() => historyInspect({ ...ctx, requestWorkspace: '/different-caller' }, { ...input, cursor })).toThrow('Inspect cursor');
    expect(() => historyInspect(ctx, { ...input, cursor: 'invalid' })).toThrow('Inspect cursor');
    db.prepare("UPDATE local_history_operations SET status='partial' WHERE operation_id='source'").run();
    const changed = historyInspect(ctx, { ...input, cursor });
    expect(changed).toMatchObject({ rows: [], partial: true, partialReasons: ['snapshot_changed'] });
    expect(changed.next?.args).toEqual(input);
    expect(historyInspect(ctx, changed.next!.args as typeof input).rows.map(row => row.file_path)).toEqual(['a']);
  } finally { db.close(); }
});
