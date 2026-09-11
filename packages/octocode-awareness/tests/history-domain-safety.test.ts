import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { connectDb } from '../src/db-runtime.js';
import { runAwarenessHistoryOperation } from '../src/history.js';

const cleanup: string[] = [];
afterEach(() => { for (const root of cleanup.splice(0)) rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'history-safety-')); cleanup.push(root);
  const workspace = join(root, 'repo'); mkdirSync(workspace); const canonical = realpathSync(workspace);
  const dbPath = join(root, 'awareness.sqlite3'); const db = connectDb(dbPath);
  const call = (command: string, request: Record<string, unknown>) => runAwarenessHistoryOperation(db, command, { workspace: canonical, ...request });
  return { root, workspace: canonical, dbPath, db, call };
}

describe('history domain adversarial contracts', () => {
  it('leaves an in-memory database and filesystem untouched when capture is disabled', async () => {
    const value = fixture(); value.db.close();
    const memory = connectDb(':memory:');
    await expect(runAwarenessHistoryOperation(memory, 'capture', { workspace: value.workspace, agent_id: 'a', phase: 'before', file: ['a.ts'] }))
      .rejects.toMatchObject({ code: 'HISTORY_DISABLED' });
    expect(memory.prepare('SELECT COUNT(*) AS count FROM local_history_operations').get()).toEqual({ count: 0 });
    expect(existsSync(`${value.dbPath}.history`)).toBe(false);
    memory.close();
  });

  it('binds terminal capture to the original host and session correlation', async () => {
    const value = fixture(); writeFileSync(join(value.workspace, 'a.ts'), 'before');
    await value.call('capture', { workspace: value.workspace, agent_id: 'a', session_id: 'session-1', host: 'pi', phase: 'before', operation_id: 'op', file: ['a.ts'] });
    await expect(value.call('capture', { agent_id: 'a', phase: 'after', operation_id: 'op', outcome: 'success' }))
      .rejects.toMatchObject({ code: 'HISTORY_OPERATION_CONFLICT' });
    value.db.close();
  });

  it('serializes concurrent first capture across independent database handles', async () => {
    const value = fixture(); writeFileSync(join(value.workspace, 'a.ts'), 'before');
    const second = connectDb(value.dbPath);
    const request = { workspace: value.workspace, agent_id: 'a', phase: 'before', operation_id: 'same', file: ['a.ts'] };
    const outcomes = await Promise.allSettled([
      runAwarenessHistoryOperation(value.db, 'capture', request), runAwarenessHistoryOperation(second, 'capture', request),
    ]);
    expect(outcomes.filter(result => result.status === 'fulfilled')).toHaveLength(1);
    const rejected = outcomes.find(result => result.status === 'rejected') as PromiseRejectedResult;
    expect(rejected.reason).toMatchObject({ code: expect.stringMatching(/^HISTORY_/) });
    expect(value.db.prepare("SELECT COUNT(*) AS count FROM local_history_operations WHERE operation_id='same'").get()).toEqual({ count: 1 });
    second.close(); value.db.close();
  });

  it('binds cursors to database, workspace, and file and reads every binary byte exactly once', async () => {
    const first = fixture(); const bytes = Buffer.from([0, 1, 2, 3, 254, 255, 4]); writeFileSync(join(first.workspace, 'a.bin'), bytes);
    await first.call('capture', { agent_id: 'a', phase: 'before', operation_id: 'binary', file: ['a.bin'] });
    let page = await first.call('read', { operation_id: 'binary', file: 'a.bin', side: 'before', limit: 2 });
    const chunks: Buffer[] = [];
    while (true) {
      chunks.push(Buffer.from(page.content as string, 'base64'));
      if (!page.next) break;
      page = await first.call('read', (page.next as { args: Record<string, unknown> }).args);
    }
    expect(Buffer.concat(chunks)).toEqual(bytes);
    for (let i = 0; i < 3; i++) await first.call('checkpoint', { agent_id: 'a', operation_id: `page-${i}`, file: ['a.bin'] });
    const timeline = await first.call('timeline', { limit: 1 });
    const next = (timeline.next as { args: Record<string, unknown> }).args;
    await expect(first.call('timeline', { ...next, cursor: `${String(next.cursor).slice(0, -1)}x` })).rejects.toMatchObject({ code: 'HISTORY_CURSOR_INVALID' });
    const other = fixture();
    await expect(other.call('timeline', next)).rejects.toMatchObject({ code: 'HISTORY_CURSOR_INVALID' });
    other.db.close(); first.db.close();
  });

  it('filters the timeline by exact captured path without leaking other operations', async () => {
    const value = fixture();
    writeFileSync(join(value.workspace, 'a.ts'), 'a');
    writeFileSync(join(value.workspace, 'b.ts'), 'b');
    await value.call('checkpoint', { agent_id: 'a', operation_id: 'only-a', file: ['a.ts'] });
    await value.call('checkpoint', { agent_id: 'a', operation_id: 'only-b', file: ['b.ts'] });
    const filtered = await value.call('timeline', { file: 'b.ts', limit: 20 });
    expect(filtered).toMatchObject({ ok: true, partial: false, next: null });
    expect((filtered.operations as Array<{ operation_id: string }>).map(row => row.operation_id)).toEqual(['only-b']);
    value.db.close();
  });

  it('rejects missing restore blobs during preview and treats symlinks as unavailable', async () => {
    const value = fixture(); writeFileSync(join(value.workspace, 'a.ts'), 'before');
    await value.call('capture', { agent_id: 'a', phase: 'before', operation_id: 'corrupt', file: ['a.ts'] });
    value.db.prepare("UPDATE local_history_versions SET before_oid='0123456789abcdef0123456789abcdef01234567' WHERE operation_id='corrupt'").run();
    await expect(value.call('restore-preview', { agent_id: 'a', operation_id: 'corrupt', side: 'before' }))
      .rejects.toMatchObject({ code: 'HISTORY_VERSION_UNAVAILABLE' });
    symlinkSync('a.ts', join(value.workspace, 'alias.ts'));
    await value.call('capture', { agent_id: 'a', phase: 'before', operation_id: 'symlink', file: ['alias.ts'] });
    await expect(value.call('restore-preview', { agent_id: 'a', operation_id: 'symlink', side: 'before' }))
      .rejects.toMatchObject({ code: 'HISTORY_VERSION_UNAVAILABLE' });
    value.db.close();
  });

  it('preserves legal leading and trailing spaces in relative filenames', async () => {
    const value = fixture(); const name = ' spaced .txt ';
    writeFileSync(join(value.workspace, name), 'exact');
    await value.call('capture', { agent_id: 'a', phase: 'before', operation_id: 'spaces', file: [name] });
    const read = await value.call('read', { operation_id: 'spaces', file: name, side: 'before' });
    expect(Buffer.from(read.content as string, 'base64').toString()).toBe('exact');
    value.db.close();
  });

  it('keeps omitted/failed captures incomplete and never manufactures verification settlement', async () => {
    const value = fixture(); writeFileSync(join(value.workspace, '.env'), 'secret');
    const before = await value.call('capture', { agent_id: 'a', phase: 'before', operation_id: 'partial', file: ['.env'] });
    expect(before).toMatchObject({ operation: { status: 'partial', outcome: 'unknown' } });
    const after = await value.call('capture', { agent_id: 'a', phase: 'after', operation_id: 'partial', outcome: 'failure' });
    expect(after).toMatchObject({ operation: { status: 'partial', outcome: 'failure' } });
    expect(value.db.prepare("SELECT COUNT(*) AS count FROM event_outbox WHERE event_type IN ('task.verified','task.verification-failed')").get()).toEqual({ count: 0 });
    value.db.close();
  });

  it('does not resolve an operation id through another workspace in the same database', async () => {
    const value = fixture(); writeFileSync(join(value.workspace, 'a.ts'), 'a');
    await value.call('capture', { agent_id: 'a', phase: 'before', operation_id: 'scoped', file: ['a.ts'] });
    const other = join(value.root, 'other'); mkdirSync(other); const otherWorkspace = realpathSync(other);
    await expect(runAwarenessHistoryOperation(value.db, 'read', { workspace: otherWorkspace, operation_id: 'scoped', file: 'a.ts', side: 'before' }))
      .rejects.toMatchObject({ code: 'HISTORY_NOT_FOUND' });
    value.db.close();
  });

  it('reports a before-only edit operation as incomplete work', async () => {
    const value = fixture(); writeFileSync(join(value.workspace, 'a.ts'), 'a');
    await value.call('capture', { agent_id: 'a', phase: 'before', operation_id: 'open-edit', file: ['a.ts'] });
    expect(await value.call('status', {})).toMatchObject({ operations: 1, incomplete: 1 });
    value.db.close();
  });
});
