import { describe, expect, it } from 'vitest';
import { spawn } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const DIST_INDEX_URL = pathToFileURL(resolve(PACKAGE_ROOT, 'dist/index.js')).href;

const OPEN_AT_ONCE = `
const [moduleUrl, dbPath, startAt] = process.argv.slice(1);
const { connectDb } = await import(moduleUrl);
const delay = Math.max(0, Number(startAt) - Date.now());
if (delay > 0) await new Promise((resolve) => setTimeout(resolve, delay));
const db = connectDb(dbPath);
const result = db.prepare('SELECT COUNT(*) AS count FROM task_runs').get();
if (result.count !== 10000) throw new Error('unexpected run count: ' + result.count);
db.close();
`;

function createV2Store(dbPath: string): void {
  const db = new DatabaseSync(dbPath);
  db.exec(`
    PRAGMA user_version = 2;
    CREATE TABLE task_runs (
      run_id TEXT PRIMARY KEY, task_id TEXT, agent_id TEXT NOT NULL, session_id TEXT,
      rationale TEXT NOT NULL, test_plan TEXT NOT NULL, context_ref TEXT,
      status TEXT NOT NULL, workspace_path TEXT, artifact TEXT,
      files_json TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, updated_at TEXT NOT NULL
    );
    CREATE TABLE locks (
      lock_id TEXT PRIMARY KEY, file_path TEXT NOT NULL, run_id TEXT NOT NULL,
      agent_id TEXT NOT NULL, session_id TEXT, lock_type TEXT NOT NULL,
      acquired_at TEXT NOT NULL, expires_at TEXT, UNIQUE(file_path, run_id)
    );
    BEGIN;
  `);
  const insert = db.prepare(`INSERT INTO task_runs
    (run_id, task_id, agent_id, rationale, test_plan, status, workspace_path,
     files_json, created_at, updated_at)
    VALUES (?, NULL, 'legacy-agent', 'legacy work', 'legacy test', 'PENDING',
      '/repo', ?, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')`);
  for (let index = 0; index < 10_000; index += 1) {
    insert.run(`run_${index}`, JSON.stringify([`/repo/file-${index}.ts`]));
  }
  db.exec('COMMIT');
  db.close();
}

function openConcurrently(dbPath: string, count: number): Promise<Array<{ code: number | null; stderr: string }>> {
  const startAt = Date.now() + 1_000;
  return Promise.all(Array.from({ length: count }, () => new Promise<{ code: number | null; stderr: string }>((resolveChild) => {
    const child = spawn(process.execPath, [
      '--input-type=module',
      '--eval', OPEN_AT_ONCE,
      DIST_INDEX_URL,
      dbPath,
      String(startAt),
    ], { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '';
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk: string) => { stderr += chunk; });
    child.on('close', (code) => resolveChild({ code, stderr }));
  })));
}

describe('concurrent schema migration', () => {
  it('serializes first open so every process observes one complete v3 migration', { timeout: 60_000 }, async () => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-awareness-concurrent-migration-'));
    const dbPath = join(root, 'awareness.sqlite3');
    try {
      createV2Store(dbPath);
      const results = await openConcurrently(dbPath, 8);
      expect(
        results.map((result) => result.code),
        results.map((result) => result.stderr).filter(Boolean).join('\n'),
      ).toEqual(results.map(() => 0));

      const migrated = new DatabaseSync(dbPath);
      expect(migrated.prepare('PRAGMA user_version').get()).toEqual({ user_version: 3 });
      expect(migrated.prepare('SELECT COUNT(*) AS count FROM task_runs').get()).toEqual({ count: 10_000 });
      expect(migrated.prepare('SELECT COUNT(*) AS count FROM run_files').get()).toEqual({ count: 10_000 });
      migrated.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
