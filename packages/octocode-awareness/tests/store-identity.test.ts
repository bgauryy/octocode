import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it } from 'vitest';
import { previewDatabaseMigration } from '../src/db-consolidation.js';
import { inspectSchemaState, readAwarenessMeta, resolveAwarenessStoreIdentity } from '../src/db-introspection.js';
import { connectDb } from '../src/db-runtime.js';
import { AWARENESS_SCHEMA_VERSION, SCHEMA_DDL, SCHEMA_INDEX_DDL } from '../src/db-schema.js';
import { AWARENESS_META_DDL } from '../src/db-meta-schema.js';
import { LEGACY_RENAMED_V1_SCHEMA_DDL } from '../src/db-predecessor-schema.js';
import { createHistoryContext, historyHash, historyStoragePaths } from '../src/history-store.js';
import { AWARENESS_APPLICATION_ID } from '../src/storage-scope.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });

function fixture() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-store-identity-')));
  roots.push(root);
  const workspace = join(root, 'workspace');
  mkdirSync(workspace);
  return { root, workspace, database: join(root, 'awareness.sqlite3') };
}

function digest(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

describe('stable Awareness store identity', () => {
  it('gives a fresh store a persistent random identity and v2 LocalGit namespace', () => {
    const { workspace, database } = fixture();
    let db = connectDb(database);
    const first = readAwarenessMeta(db);
    const paths = historyStoragePaths(createHistoryContext(db, workspace));
    expect(first).toMatchObject({ applicationId: AWARENESS_APPLICATION_ID, schemaVersion: AWARENESS_SCHEMA_VERSION });
    expect(first.storeId).toMatch(/^[0-9a-f-]{36}$/);
    expect(paths).toMatchObject({
      store_id: first.storeId,
      layout: 'awareness-v2',
      root: join(workspace, '.octocode', '.localGit', first.storeId, 'awareness-v2', historyHash(workspace)),
      current_path_preserved: false,
    });
    db.close();
    db = connectDb(database);
    expect(readAwarenessMeta(db)).toEqual(first);
    db.close();
  });

  it('derives an existing canonical identity without mutation and preserves its v1 LocalGit namespace', () => {
    const { workspace, database } = fixture();
    const old = new DatabaseSync(database);
    old.exec(SCHEMA_DDL.replace(AWARENESS_META_DDL, ''));
    old.exec(SCHEMA_INDEX_DDL);
    old.exec('PRAGMA journal_mode=WAL');
    old.exec(`PRAGMA application_id=${AWARENESS_APPLICATION_ID}`);
    old.close();
    const before = digest(database);
    const expectedStoreId = historyHash(realpathSync(database));
    const existing = join(workspace, '.octocode', '.localGit', expectedStoreId, 'awareness-v1', historyHash(workspace));
    mkdirSync(existing, { recursive: true });
    writeFileSync(join(existing, 'sentinel'), 'preserve');

    const db = connectDb(database);
    expect(resolveAwarenessStoreIdentity(db)).toMatchObject({ storeId: expectedStoreId, schemaVersion: 1, persisted: false });
    expect(db.prepare("SELECT name FROM sqlite_schema WHERE name='awareness_meta'").get()).toBeUndefined();
    const paths = historyStoragePaths(createHistoryContext(db, workspace));
    expect(paths).toMatchObject({ root: existing, layout: 'awareness-v1', current_path_preserved: true });
    expect(readFileSync(join(existing, 'sentinel'), 'utf8')).toBe('preserve');
    db.close();
    expect(digest(database)).toBe(before);
  });

  it('keeps the same LocalGit namespace after the database file moves', async () => {
    const { root, workspace, database } = fixture();
    const moved = join(root, 'moved.sqlite3');
    let db = connectDb(database);
    const original = historyStoragePaths(createHistoryContext(db, workspace))!;
    await createHistoryContext(db, workspace).store();
    db.close();
    renameSync(database, moved);

    db = connectDb(moved);
    const relocated = historyStoragePaths(createHistoryContext(db, workspace))!;
    expect(relocated.store_id).toBe(original.store_id);
    expect(relocated.root).toBe(original.root);
    expect(relocated.git_dir).toBe(original.git_dir);
    db.close();
  });

  it('classifies and previews the observed renamed-relation predecessor without changing either path', () => {
    const { workspace, database } = fixture();
    const destination = join(workspace, 'migrated.sqlite3');
    const source = new DatabaseSync(database);
    source.exec(LEGACY_RENAMED_V1_SCHEMA_DDL);
    source.prepare(`INSERT INTO agents
      (agent_id, agent_name, workspace_path, artifact, context, registered_at, last_seen_at)
      VALUES ('agent', '', ?, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')`).run(workspace);
    source.exec(`PRAGMA application_id=${AWARENESS_APPLICATION_ID}`);
    source.close();
    const before = digest(database);

    const readonly = new DatabaseSync(database, { readOnly: true });
    expect(inspectSchemaState(readonly)).toBe('legacy-renamed-predecessor');
    readonly.close();
    const preview = previewDatabaseMigration(database, destination, { workspace });

    expect(preview).toMatchObject({
      dryRun: true,
      sourceVersion: 'legacy-renamed-v1',
      sourcePath: database,
      destinationPath: destination,
      sourceUnchanged: true,
      copiedTables: { agents: 1, run_files: 0 },
      localGit: {
        storeId: historyHash(realpathSync(database)),
        sourceLayout: 'awareness-v1',
        destinationLayout: 'awareness-v2',
        sourceRoot: join(workspace, '.octocode', '.localGit', historyHash(realpathSync(database)), 'awareness-v1', historyHash(workspace)),
      },
    });
    expect(preview.transformations).toContainEqual({
      source: 'agents', destination: 'awareness_agents', rows: 1,
      defaultedColumns: ['role', 'status', 'metadata_json'],
    });
    expect(preview.transformations).toContainEqual({
      source: 'tasks', destination: 'awareness_tasks', rows: 0,
      defaultedColumns: ['source_step_key', 'check_command'],
    });
    expect(preview.createdRelations).toContain('awareness_meta');
    expect(existsSync(destination)).toBe(false);
    expect(digest(database)).toBe(before);
  });
});
