import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it } from 'vitest';
import { connectDb } from '../src/db-runtime.js';
import { readAwarenessMeta } from '../src/db-introspection.js';
import { historyStoragePathsForIdentity } from '../src/history-store.js';
import { applyStoreRetirement, reportStoreRetirement } from '../src/store-retirement.js';

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture() {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-store-retirement-')));
  roots.push(root);
  const workspace = join(root, 'workspace');
  mkdirSync(workspace);
  const database = join(root, 'awareness.sqlite3');
  const db = connectDb(database);
  const identity = readAwarenessMeta(db);
  db.close();
  const storage = historyStoragePathsForIdentity({ workspace, dbPath: database }, { ...identity, persisted: true })!;
  mkdirSync(storage.history_root, { recursive: true });
  writeFileSync(join(storage.history_root, 'sentinel'), 'recoverable');
  const legacy = `${database}.history`;
  mkdirSync(legacy);
  writeFileSync(join(legacy, 'sentinel'), 'legacy-recoverable');
  return { root, workspace, database, storage, legacy };
}

describe('Awareness store retirement', () => {
  it('reports exact SQLite and derived LocalGit targets without moving them', () => {
    const { workspace, database, storage, legacy } = fixture();

    const report = reportStoreRetirement({ database, workspaces: [workspace] });

    expect(report).toMatchObject({
      action: 'report',
      dry_run: true,
      can_apply: true,
      confirmation: 'retire',
      blockers: [],
      database: { path: database },
    });
    expect(report.targets).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: 'sqlite', source: database, exists: true }),
      expect.objectContaining({ kind: 'local_git', source: storage.history_root, exists: true }),
      expect.objectContaining({ kind: 'legacy_local_git', source: legacy, exists: true }),
    ]));
    expect(existsSync(database)).toBe(true);
    expect(readFileSync(join(storage.history_root, 'sentinel'), 'utf8')).toBe('recoverable');
    expect(report.targets.every(target => !existsSync(target.quarantine))).toBe(true);
  });

  it('requires the exact confirmation and quarantines the complete recoverable store', () => {
    const { workspace, database, storage, legacy } = fixture();
    const report = reportStoreRetirement({ database, workspaces: [workspace] });

    expect(() => applyStoreRetirement({ report, confirm: undefined })).toThrow(/confirm.*retire/i);
    expect(existsSync(database)).toBe(true);
    expect(existsSync(storage.history_root)).toBe(true);

    const applied = applyStoreRetirement({ report, confirm: 'retire' });

    expect(applied).toMatchObject({ action: 'apply', status: 'quarantined', report_id: report.report_id });
    expect(existsSync(database)).toBe(false);
    expect(existsSync(storage.history_root)).toBe(false);
    expect(existsSync(legacy)).toBe(false);
    const databaseTarget = applied.quarantined.find(target => target.source === database)!;
    const historyTarget = applied.quarantined.find(target => target.source === storage.history_root)!;
    const legacyTarget = applied.quarantined.find(target => target.source === legacy)!;
    expect(existsSync(databaseTarget.quarantine)).toBe(true);
    expect(readFileSync(join(historyTarget.quarantine, 'sentinel'), 'utf8')).toBe('recoverable');
    expect(readFileSync(join(legacyTarget.quarantine, 'sentinel'), 'utf8')).toBe('legacy-recoverable');
  });

  it('refuses unfinished lifecycle state before any target moves', () => {
    const { workspace, database, storage } = fixture();
    const db = new DatabaseSync(database);
    db.prepare(`INSERT INTO sessions
      (session_id,agent_id,workspace_path,started_at)
      VALUES ('session-active','agent-active',?,?)`).run(workspace, new Date().toISOString());
    db.close();

    const report = reportStoreRetirement({ database, workspaces: [workspace] });

    expect(report.can_apply).toBe(false);
    expect(report.blockers).toContainEqual(expect.objectContaining({ code: 'open_sessions', count: 1 }));
    expect(() => applyStoreRetirement({ report, confirm: 'retire' })).toThrow(/active state/i);
    expect(existsSync(database)).toBe(true);
    expect(existsSync(storage.history_root)).toBe(true);
  });

  it('retains an absent recorded workspace in the report without blocking retirement', () => {
    const { workspace, database, legacy } = fixture();
    const db = new DatabaseSync(database);
    db.prepare(`INSERT INTO sessions(session_id,agent_id,workspace_path,started_at,ended_at)
      VALUES ('session-ended','agent-ended',?,?,?)`)
      .run(workspace, '2020-01-01T00:00:00Z', '2020-01-02T00:00:00Z');
    db.close();
    rmSync(workspace, { recursive: true });

    const report = reportStoreRetirement({ database });
    expect(report).toMatchObject({ can_apply: true, workspaces: [workspace] });
    expect(report.targets).toContainEqual(expect.objectContaining({
      kind: 'local_git', exists: false,
    }));
    const result = applyStoreRetirement({ report, confirm: 'retire' });
    expect(result.status).toBe('quarantined');
    expect(existsSync(database)).toBe(false);
    expect(existsSync(legacy)).toBe(false);
  });

  it('refuses an active SQLite writer and leaves every target in place', () => {
    const { workspace, database, storage } = fixture();
    const report = reportStoreRetirement({ database, workspaces: [workspace] });
    const writer = new DatabaseSync(database);
    writer.exec('BEGIN IMMEDIATE');
    try {
      expect(() => applyStoreRetirement({ report, confirm: 'retire' })).toThrow(/active SQLite writer/i);
      expect(existsSync(database)).toBe(true);
      expect(existsSync(storage.history_root)).toBe(true);
    } finally {
      writer.exec('ROLLBACK');
      writer.close();
    }
  });

  it('rejects broad workspace targets and stale dry-run plans', () => {
    const { root, workspace, database, storage } = fixture();
    expect(() => reportStoreRetirement({ database, workspaces: ['/'] })).toThrow(/broad.*workspace/i);
    expect(() => reportStoreRetirement({ database, workspaces: [homedir()] })).toThrow(/broad.*workspace/i);
    const linkedDatabase = join(root, 'linked.sqlite3');
    const linkedWorkspace = join(root, 'linked-workspace');
    symlinkSync(database, linkedDatabase);
    symlinkSync(workspace, linkedWorkspace);
    expect(() => reportStoreRetirement({ database: linkedDatabase, workspaces: [workspace] })).toThrow(/canonical.*database/i);
    expect(() => reportStoreRetirement({ database, workspaces: [linkedWorkspace] })).toThrow(/symlinked.*workspace/i);

    const report = reportStoreRetirement({ database, workspaces: [workspace] });
    renameSync(storage.history_root, `${storage.history_root}-old`);
    mkdirSync(storage.history_root);

    expect(() => applyStoreRetirement({ report, confirm: 'retire' })).toThrow(/changed since report/i);
    expect(existsSync(database)).toBe(true);
    expect(existsSync(storage.history_root)).toBe(true);
  });

  it.runIf(process.platform !== 'win32')('rejects dangling symlink sources and quarantine collisions', () => {
    const sourceFixture = fixture();
    rmSync(sourceFixture.legacy, { recursive: true });
    symlinkSync(join(sourceFixture.root, 'missing-legacy'), sourceFixture.legacy);
    expect(() => reportStoreRetirement({
      database: sourceFixture.database,
      workspaces: [sourceFixture.workspace],
    })).toThrow(/symlink/i);
    expect(existsSync(sourceFixture.database)).toBe(true);

    const collisionFixture = fixture();
    const report = reportStoreRetirement({
      database: collisionFixture.database,
      workspaces: [collisionFixture.workspace],
    });
    const quarantine = report.targets.find(target => target.source === collisionFixture.database)!.quarantine;
    symlinkSync(join(collisionFixture.root, 'missing-quarantine-target'), quarantine);
    expect(() => applyStoreRetirement({ report, confirm: 'retire' })).toThrow(/quarantine destination/i);
    expect(existsSync(collisionFixture.database)).toBe(true);
  });

  it.runIf(process.platform !== 'win32')('rolls back an earlier quarantine rename when a later target cannot move', () => {
    const { root, workspace, database, storage } = fixture();
    const secondWorkspace = join(root, 'workspace-z');
    mkdirSync(secondWorkspace);
    const db = new DatabaseSync(database, { readOnly: true });
    const identity = readAwarenessMeta(db);
    db.close();
    const secondStorage = historyStoragePathsForIdentity(
      { workspace: secondWorkspace, dbPath: database },
      { ...identity, persisted: true },
    )!;
    mkdirSync(secondStorage.history_root, { recursive: true });
    writeFileSync(join(secondStorage.history_root, 'sentinel'), 'second');
    const report = reportStoreRetirement({ database, workspaces: [workspace, secondWorkspace] });
    const protectedParent = join(secondWorkspace, '.octocode', '.localGit');
    chmodSync(protectedParent, 0o555);
    try {
      expect(() => applyStoreRetirement({ report, confirm: 'retire' })).toThrow();
      expect(existsSync(database)).toBe(true);
      expect(existsSync(storage.history_root)).toBe(true);
      expect(existsSync(secondStorage.history_root)).toBe(true);
      expect(report.targets.every(target => !existsSync(target.quarantine))).toBe(true);
    } finally {
      chmodSync(protectedParent, 0o755);
    }
  });
});
