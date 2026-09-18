import { afterEach, expect, it } from 'vitest';
import { mkdtempSync, realpathSync, rmSync, writeFileSync, readFileSync, mkdirSync, utimesSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectDb } from '../src/db-runtime.js';
import { runAwarenessHistoryOperation } from '../src/history-api.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true })));
function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'history-maintenance-')));
  roots.push(workspace);
  const database = join(workspace, 'ledger.sqlite3');
  const db = connectDb(database);
  db.prepare(`INSERT INTO local_history_operations
    (operation_id,workspace_path,agent_id,kind,status,request_hash,created_at,updated_at)
    VALUES ('source',?,'maintenance-test','edit','failed','source-hash','2020-01-01','2020-01-01')`).run(workspace);
  db.close();
  function preview(id: string, status: string, expires: string, files = '["file.ts"]', result: string | null = null, scope = workspace) {
    const connection = connectDb(database);
    try {
      connection.prepare(`INSERT INTO local_history_restores
        (preview_id,workspace_path,agent_id,source_operation_id,side,files_json,expected_json,target_json,status,expires_at,result_json,created_at)
        VALUES (?,?,'maintenance-test','source','before',?,'[]','[]',?,?,?,'2020-01-01')`)
        .run(id, scope, files, status, expires, result);
    } finally { connection.close(); }
  }
  async function call(command: string, params: Record<string, unknown> = {}, expectedExit = 0) {
    const action = command.replace(/^history /, '');
    const connection = connectDb(database);
    try {
      const payload = await runAwarenessHistoryOperation(connection, action, {
        workspace,
        ...(['capture', 'checkpoint', 'restore-preview', 'restore-apply'].includes(action)
          ? { agent_id: 'maintenance-test' }
          : {}),
        ...params,
      });
      expect(expectedExit === 0 || (expectedExit === 2 && payload.ok === false), JSON.stringify(payload)).toBe(true);
      return payload as Record<string, any>;
    } catch (error) {
      expect(expectedExit, error instanceof Error ? error.message : String(error)).toBe(1);
      return { ok: false, error };
    } finally {
      connection.close();
    }
  }
  async function pages(command: string, params: Record<string, unknown>, field: string) {
    const rows: Record<string, unknown>[] = [];
    let request = { command, params };
    for (let index = 0; index < 20; index++) {
      const payload = await call(request.command, request.params);
      rows.push(...payload[field]);
      if (!payload.partial) return rows;
      expect(payload.next?.command, JSON.stringify(payload)).toBeTruthy();
      request = { command: String(payload.next.command), params: payload.next.args };
    }
    throw new Error('Maintenance continuation did not terminate');
  }
  return { workspace, database, call, preview, pages };
}

it('prunes only expired ready previews across executable pages, preserving journals and other scopes', async () => {
  const f = fixture();
  for (let i = 0; i < 3; i++) f.preview(`expired-${i}`, 'ready', '2020-01-01');
  f.preview('fresh', 'ready', '2999-01-01');
  f.preview('applying', 'applying', '2020-01-01');
  f.preview('applied', 'applied', '2020-01-01');
  f.preview('other', 'ready', '2020-01-01', '["file.ts"]', null, '/other');
  expect((await f.pages('history retention-preview', { limit: 1 }, 'previews')).map(row => row.preview_id).sort())
    .toEqual(['expired-0', 'expired-1', 'expired-2']);
  expect((await f.pages('history retention-prune', { confirm: 'prune', limit: 1 }, 'pruned')).map(row => row.preview_id).sort())
    .toEqual(['expired-0', 'expired-1', 'expired-2']);
  const db = connectDb(f.database);
  try { expect(db.prepare('SELECT preview_id FROM local_history_restores ORDER BY preview_id').all().map(row => row.preview_id))
    .toEqual(['applied', 'applying', 'fresh', 'other']); } finally { db.close(); }
});

it('executes recovery continuations across tied operation and restore rowids', async () => {
  const f = fixture();
  f.preview('restore', 'applying', '2020-01-01');
  const rows = await f.pages('history recovery', { action: 'report', limit: 1 }, 'uncertain');
  expect(rows.map(row => row.operation_id).sort()).toEqual(['restore', 'source']);
});

it('reconciles only a complete unique durable journal and never replays workspace edits', async () => {
  const f = fixture();
  writeFileSync(join(f.workspace, 'file.ts'), 'preserve current bytes');
  f.preview('complete', 'applying', '2020-01-01', '["file.ts"]', '{"results":[{"path":"file.ts","status":"restored"}]}');
  f.preview('duplicate', 'applying', '2020-01-01', '["file.ts"]', '{"results":[{"path":"file.ts","status":"restored"},{"path":"file.ts","status":"restored"}]}');
  f.preview('non-array', 'applying', '2020-01-01', '["file.ts"]', '{"results":{}}');
  f.preview('broken-files', 'applying', '2020-01-01', '{broken', '{"results":[]}');
  f.preview('missing', 'applying', '2020-01-01');
  const result = await f.call('history recovery', { action: 'reconcile', confirm: 'reconcile', limit: 20 });
  expect(result.reconciled).toEqual(['complete']);
  expect(result.uncertain.map((row: Record<string, unknown>) => row.operation_id).sort())
    .toEqual(['broken-files', 'duplicate', 'missing', 'non-array', 'source']);
  expect(readFileSync(join(f.workspace, 'file.ts'), 'utf8')).toBe('preserve current bytes');
});

it('reports evidence through the public API without mutating objects', async () => {
  const f = fixture();
  const report = await f.call('history evidence', { action: 'report' });
  expect(report).toMatchObject({ ok: true, dry_run: true, safety: 'observational', objects: [] });
});

it('continues past a full scan window without skipping the first unscanned object', async () => {
  const f = fixture();
  await f.call('history evidence');
  const status = await f.call('history status');
  const objects = join(status.storage.git_dir, 'objects', '00');
  mkdirSync(objects, { recursive: true });
  for (let i = 1; i <= 1003; i++) {
    const path = join(objects, i.toString(16).padStart(38, '0'));
    writeFileSync(path, 'disposable orphan metadata fixture');
    if (i > 1000) utimesSync(path, new Date('2020-01-01'), new Date('2020-01-01'));
  }
  const first = await f.call('history evidence', { limit: 1 });
  expect(first).toMatchObject({ objects: [], partial: true, quiescent: false,
    diagnostic: { code: 'HISTORY_EVIDENCE_SCAN_LIMIT' } });
  const all = await f.pages(first.next.command, first.next.args, 'objects');
  expect(all.map(row => row.oid)).toEqual([1001, 1002, 1003].map(i => i.toString(16).padStart(40, '0')));
});
