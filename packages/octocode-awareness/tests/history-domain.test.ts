import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync, rmSync, existsSync, chmodSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectDb } from '../src/db-runtime.js';
import { runAwarenessHistoryOperation } from '../src/history.js';
import { version } from 'isomorphic-git';

describe('canonical local history domain', () => {
  let root: string;
  let workspace: string;
  let db: ReturnType<typeof connectDb>;
  const agent_id = 'history-test';
  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'awareness-history-'));
    workspace = join(root, 'repo');
    mkdirSync(workspace);
    workspace = realpathSync(workspace);
    db = connectDb(join(root, 'awareness.sqlite3'));
    writeFileSync(join(workspace, 'a.ts'), 'before');
  });
  afterEach(() => { db.close(); rmSync(root, { recursive: true, force: true }); });
  const call = (command: string, input: Record<string, unknown> = {}) =>
    runAwarenessHistoryOperation(db, command, { workspace, ...input });
  const before = (operation_id = 'edit-1', file = ['a.ts']) =>
    call('capture', { agent_id, phase: 'before', operation_id, file });

  it('reports capability without initializing private Git', async () => {
    expect(await call('status')).toMatchObject({
      ok: true, available: true, initialized: false,
      backend: { name: 'isomorphic-git', version: version(), bundled: true, system_git_required: false },
    });
    expect(existsSync(join(root, 'awareness.sqlite3.history'))).toBe(false);
  });
  it('records exact before/after content, preserves outcome, and leaves user Git alone', async () => {
    mkdirSync(join(workspace, '.git'));
    writeFileSync(join(workspace, '.git', 'index'), 'user-index');
    await before();
    writeFileSync(join(workspace, 'a.ts'), 'partial-write');
    expect(await call('capture', { agent_id, phase: 'after', operation_id: 'edit-1', outcome: 'failure' }))
      .toMatchObject({ ok: true, operation: { status: 'complete', outcome: 'failure' } });
    const result = await call('read', { operation_id: 'edit-1', file: 'a.ts', side: 'before' });
    expect(result).toMatchObject({ encoding: 'base64', content: Buffer.from('before').toString('base64') });
    expect(readFileSync(join(workspace, '.git', 'index'), 'utf8')).toBe('user-index');
    expect(db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name = 'edit_log'").get()).toBeUndefined();
  });
  it('replays a matching before capture, rejects altered inputs and a foreign actor', async () => {
    const initial = await before();
    writeFileSync(join(workspace, 'a.ts'), 'new');
    expect(await before()).toEqual(initial);
    await expect(before('edit-1', ['b.ts'])).rejects.toMatchObject({ code: 'HISTORY_OPERATION_CONFLICT' });
    await expect(call('capture', { agent_id: 'other', phase: 'after', operation_id: 'edit-1', outcome: 'success' }))
      .rejects.toMatchObject({ code: 'HISTORY_OPERATION_CONFLICT' });
  });
  it('does not capture secrets or turn omitted content into an empty file', async () => {
    writeFileSync(join(workspace, '.env'), 'secret');
    await before('secrets', ['.env']);
    expect(await call('read', { operation_id: 'secrets', file: '.env', side: 'before' }))
      .toMatchObject({ status: 'omitted', content: null });
    await expect(call('restore-preview', { agent_id, operation_id: 'secrets', side: 'before' }))
      .rejects.toMatchObject({ code: 'HISTORY_VERSION_UNAVAILABLE' });
  });
  it('paginates timeline and binary reads with executable continuations', async () => {
    for (let i = 0; i < 3; i++) await before(`edit-${i}`);
    const first = await call('timeline', { limit: 2 });
    expect(first.operations).toHaveLength(2);
    expect(first.next).toMatchObject({ operation: 'history.timeline', params: { limit: 2 } });
    const next = first.next as { params: Record<string, unknown> };
    const second = await call('timeline', next.params);
    expect(second.operations).toHaveLength(1);
    expect(second.next).toBeNull();
    const page = await call('read', { operation_id: 'edit-1', file: 'a.ts', side: 'before', limit: 2 });
    expect(page).toMatchObject({ content: Buffer.from('be').toString('base64'), next: { operation: 'history.read', params: { offset: 2 } } });
    await expect(call('timeline', { ...(next.params), file: 'b.ts' })).rejects.toMatchObject({ code: 'HISTORY_CURSOR_INVALID' });
  });
  it('requires exact post-capture paths and does not claim unknown checkpoint preimages', async () => {
    await before();
    await expect(call('capture', { agent_id, phase: 'after', operation_id: 'edit-1', outcome: 'success', file: ['other.ts'] }))
      .rejects.toMatchObject({ code: 'HISTORY_OPERATION_CONFLICT' });
    await call('checkpoint', { agent_id, operation_id: 'checkpoint', file: ['a.ts'] });
    expect(await call('read', { operation_id: 'checkpoint', file: 'a.ts', side: 'before' }))
      .toMatchObject({ status: 'unknown', content: null });
  });
  it('previews, applies only selected files, and records a durable undo checkpoint', async () => {
    await before();
    writeFileSync(join(workspace, 'a.ts'), 'after');
    writeFileSync(join(workspace, 'untouched.ts'), 'keep');
    const preview = await call('restore-preview', { agent_id, operation_id: 'edit-1', side: 'before' });
    expect(readFileSync(join(workspace, 'a.ts'), 'utf8')).toBe('after');
    const result = await call('restore-apply', { agent_id, preview_id: preview.preview_id });
    expect(result).toMatchObject({ ok: true, status: 'applied', undo_operation_id: expect.any(String), undo_preview: {
      command: 'history restore-preview',
      params: { workspace, agent_id, side: 'after' },
    } });
    expect((result as { undo_preview: { params: { operation_id: string } } }).undo_preview.params.operation_id)
      .toBe((result as { undo_operation_id: string }).undo_operation_id);
    expect(readFileSync(join(workspace, 'a.ts'), 'utf8')).toBe('before');
    expect(readFileSync(join(workspace, 'untouched.ts'), 'utf8')).toBe('keep');
    expect(await call('read', { operation_id: result.undo_operation_id, file: 'a.ts', side: 'after' }))
      .toMatchObject({ content: Buffer.from('after').toString('base64') });
    expect(await call('restore-apply', { agent_id, preview_id: preview.preview_id })).toEqual(result);
  });
  it.each(['bytes', 'mode'])('rejects stale restore preview when %s change', async (kind) => {
    await before();
    writeFileSync(join(workspace, 'a.ts'), 'after');
    const preview = await call('restore-preview', { agent_id, operation_id: 'edit-1', side: 'before' });
    if (kind === 'bytes') writeFileSync(join(workspace, 'a.ts'), 'external');
    else chmodSync(join(workspace, 'a.ts'), 0o755);
    expect(await call('restore-apply', { agent_id, preview_id: preview.preview_id })).toMatchObject({ ok: false, status: 'conflict' });
    expect(readFileSync(join(workspace, 'a.ts'), 'utf8')).toBe(kind === 'bytes' ? 'external' : 'after');
  });
  it('restores a proven missing preimage as deletion', async () => {
    await before('create', ['created.ts']);
    writeFileSync(join(workspace, 'created.ts'), 'created');
    const preview = await call('restore-preview', { agent_id, operation_id: 'create', side: 'before' });
    expect(await call('restore-apply', { agent_id, preview_id: preview.preview_id })).toMatchObject({ status: 'applied' });
    expect(existsSync(join(workspace, 'created.ts'))).toBe(false);
  });
  it('rejects foreign and mixed request fields at the domain boundary', async () => {
    await expect(call('status', { phase: 'before' })).rejects.toMatchObject({ code: 'HISTORY_INVALID_REQUEST' });
    await expect(call('capture', { agent_id, phase: 'before', file: ['a.ts'], before_commit_oid: 'fake' }))
      .rejects.toMatchObject({ code: 'HISTORY_INVALID_REQUEST' });
  });
});
