import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { initDb } from '../src/db-init.js';
import { connectDb } from '../src/db-runtime.js';
import { openAwarenessStore } from '../src/coordination/open.js';
import { awarenessEntityCatalog } from '../src/schema/entities.js';
import { appendWorkerLifecycleEvent } from '../src/worker-lifecycle-ledger.js';
import { tsxCli } from './helpers/tsx-cli.js';

const SOURCE_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../bin/awareness.ts');
const TSX_SCRIPT = tsxCli;
const NODE = process.execPath;

function runSource(args: string[]): { status: number; stdout: string; stderr: string; parsed: Record<string, unknown> | null } {
  const result = spawnSync(NODE, [TSX_SCRIPT, SOURCE_SCRIPT, ...args], {
    cwd: process.cwd(), env: process.env, encoding: 'utf8', timeout: 30000,
  });
  let parsed: Record<string, unknown> | null = null;
  try { parsed = JSON.parse(result.stdout) as Record<string, unknown>; } catch { /* non-JSON */ }
  return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, parsed };
}

function mktemp(): string {
  return mkdtempSync(join(tmpdir(), 'oc-schema-entities-'));
}

describe('schema entities', () => {
  it('lists the single canonical entity catalog without opening a database', () => {
    const result = runSource(['schema', 'entities', '--all', '--compact']);
    expect(result.status, result.stderr || result.stdout).toBe(0);
    const entities = result.parsed?.['entities'] as Array<Record<string, unknown>>;
    expect(result.parsed?.['kind']).toBe('awareness.entities');
    expect(entities).toHaveLength(32);
    expect(entities.find((entity) => entity['name'] === 'awareness_plans')).toMatchObject({ owner: 'awareness', family: 'planning' });
    expect(entities.find((entity) => entity['name'] === 'plans')).toBeUndefined();
    expect(entities.find((entity) => entity['name'] === 'worker_lifecycle_events')).toMatchObject({ owner: 'awareness', family: 'workers' });
    expect(entities.find((entity) => entity['name'] === 'memories_fts')).toMatchObject({ kind: 'virtual_table', family: 'search' });
    expect(result.parsed?.['storage']).toMatchObject({ default_scope: 'global', repo_override: '--db-scope repo', explicit_override: '--db <path>' });

    const compact = runSource(['schema', 'entities', '--compact']);
    expect(compact.status, compact.stderr || compact.stdout).toBe(0);
    expect(compact.parsed?.['families']).toEqual(expect.arrayContaining([
      expect.objectContaining({ family: 'planning', entities: expect.arrayContaining(['awareness_plans']) }),
      expect.objectContaining({ family: 'workers', entities: ['worker_lifecycle_events'] }),
    ]));
    expect(compact.parsed?.['entities']).toBeUndefined();
  });

  it('matches freshly initialized SQLite relations and does not create an isolated home', () => {
    const dir = mktemp();
    const dbPath = join(dir, 'entities.sqlite3');
    try {
      const store = openAwarenessStore({ dbPath, workspace: dir });
      store.close();
      const db = connectDb(dbPath);
      try {
        initDb(db);
        appendWorkerLifecycleEvent(db, {
          packetId: 'catalog-completeness-probe', workspace: dir, sessionId: 'catalog-session',
          workerId: 'catalog-worker', correlationId: 'catalog-correlation', type: 'schema.probe',
          redaction: 'public', createdAt: '2026-01-01T00:00:00.000Z', payload: {},
        });
      } finally { db.close(); }

      const actual = new DatabaseSync(dbPath);
      try {
        const actualNames = new Set(
          (actual.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT GLOB 'memories_fts_*'").all() as Array<{ name: string }>)
            .map((row) => row.name),
        );
        const catalogNames = new Set(awarenessEntityCatalog().entities.map((entity) => entity.name));
        expect([...actualNames].sort()).toEqual([...catalogNames].sort());
      } finally { actual.close(); }

      const isolatedHome = join(dir, 'missing-home');
      const isolatedCatalog = awarenessEntityCatalog({ ...process.env, OCTOCODE_HOME: isolatedHome });
      expect(isolatedCatalog.storage.default_path).toBe(join(isolatedHome, 'awareness', 'awareness.sqlite3'));
      expect(existsSync(isolatedHome)).toBe(false);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
});
