import { describe, expect, it } from 'vitest';
import { spawn } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const DIST_INDEX_URL = pathToFileURL(resolve(PACKAGE_ROOT, 'out/index.js')).href;

const OPEN_AT_ONCE = `
const [moduleUrl, dbPath, startAt] = process.argv.slice(1);
const { connectDb } = await import(moduleUrl);
const delay = Math.max(0, Number(startAt) - Date.now());
if (delay > 0) await new Promise((resolve) => setTimeout(resolve, delay));
const db = connectDb(dbPath);
const result = db.prepare("SELECT COUNT(*) AS count FROM sqlite_schema WHERE type = 'table'").get();
if (result.count < 10) throw new Error('incomplete database contract: ' + result.count);
db.close();
`;

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

describe('concurrent database initialization', () => {
  it('serializes first open so every process observes one complete contract', { timeout: 60_000 }, async () => {
    const root = mkdtempSync(join(tmpdir(), 'octocode-awareness-concurrent-init-'));
    const dbPath = join(root, 'awareness.sqlite3');
    try {
      const results = await openConcurrently(dbPath, 8);
      expect(
        results.map((result) => result.code),
        results.map((result) => result.stderr).filter(Boolean).join('\n'),
      ).toEqual(results.map(() => 0));

      const database = new DatabaseSync(dbPath);
      expect(database.prepare('PRAGMA application_id').get()).toEqual({ application_id: 0x4f435431 });
      expect(database.prepare("SELECT name FROM sqlite_schema WHERE name='memories'").get())
        .toEqual({ name: 'memories' });
      database.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
