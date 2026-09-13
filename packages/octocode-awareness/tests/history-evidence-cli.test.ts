import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, realpathSync, rmSync, utimesSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { afterEach, expect, it } from 'vitest';
import { connectDb } from '../src/db-runtime.js';
import { createHistoryContext } from '../src/history-store.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true })));

it('runs report and confirmation-gated reclaim through the built operator CLI', async () => {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'history-evidence-cli-')));
  roots.push(workspace);
  const database = join(workspace, 'awareness.sqlite3');
  const db = connectDb(database);
  const store = await createHistoryContext(db, workspace).store();
  const orphan = await store.writeBlob(Buffer.from('operator-only orphan'));
  await store.flush();
  const objectPath = join(store.gitdir, 'objects', orphan.oid.slice(0, 2), orphan.oid.slice(2));
  const old = new Date('2020-01-01T00:00:00Z');
  utimesSync(objectPath, old, old);
  db.close();

  const cli = resolve(import.meta.dirname, '../out/octocode-awareness.js');
  const help = spawnSync(process.execPath, [cli, 'history', 'evidence', '--help', '--compact'], {
    cwd: workspace, encoding: 'utf8', timeout: 15_000,
  });
  expect(help.status, help.stderr || help.stdout).toBe(0);
  expect(help.stdout).toContain('--action');
  expect(help.stdout).toContain('--confirm');

  const catalog = spawnSync(process.execPath, [cli, 'schema', 'commands', '--compact'], {
    cwd: workspace, encoding: 'utf8', timeout: 15_000,
  });
  expect(catalog.status, catalog.stderr || catalog.stdout).toBe(0);
  expect((JSON.parse(catalog.stdout) as { operations: string[] }).operations).not.toContain('history.evidence');

  const run = (...args: string[]) => spawnSync(process.execPath, [cli, 'history', 'evidence',
    '--db', database, '--workspace', workspace, '--grace-seconds', '3600', '--compact', ...args], {
    cwd: workspace, encoding: 'utf8', timeout: 15_000,
  });

  const report = run('--action', 'report');
  expect(report.status, report.stderr || report.stdout).toBe(0);
  expect(JSON.parse(report.stdout)).toMatchObject({
    ok: true, action: 'report', dry_run: true,
    objects: [{ oid: orphan.oid }],
  });
  expect(existsSync(objectPath)).toBe(true);

  const unconfirmed = run('--action', 'reclaim');
  expect(unconfirmed.status, unconfirmed.stderr || unconfirmed.stdout).toBe(1);
  expect(JSON.parse(unconfirmed.stdout)).toMatchObject({ ok: false, error: expect.stringMatching(/confirm/i) });
  expect(existsSync(objectPath)).toBe(true);

  const reclaimed = run('--action', 'reclaim', '--confirm', 'reclaim');
  expect(reclaimed.status, reclaimed.stderr || reclaimed.stdout).toBe(0);
  expect(JSON.parse(reclaimed.stdout)).toMatchObject({
    ok: true, action: 'reclaim', dry_run: false, reclaimed_objects: 1,
  });
  expect(existsSync(objectPath)).toBe(false);
});
