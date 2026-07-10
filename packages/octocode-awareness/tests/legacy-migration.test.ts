import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import {
  AWARENESS_APPLICATION_ID,
  AWARENESS_SCHEMA_VERSION,
  connectDb,
  initDb,
  tableColumns,
} from '../src/db.js';
import { insertMemory, getMemory } from '../src/memory.js';
import { insertRefinement } from '../src/refinements.js';
import { insertHarnessLog } from '../src/audit.js';

/**
 * Upgrade-path regression tests. This bug class has shipped twice:
 * index-created-before-migration, then migration that only backfilled
 * `artifact` so any newer column (failure_signature, valid_from, …) broke
 * every command on a pre-existing store — silently, because hooks fail open.
 * These tests open stores frozen at older schema generations and assert
 * initDb brings them fully up to the canonical schema.
 */

/** Store generation ~pre-bitemporal/pre-semantic: memories lacks state/label/
 *  failure_signature/valid_from/valid_to/embedding/etc; other tables lack
 *  artifact and session_id columns. */
function legacyDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec(`
    CREATE TABLE sessions (
      session_id TEXT PRIMARY KEY,
      agent_id   TEXT NOT NULL,
      workspace_path TEXT,
      started_at TEXT NOT NULL,
      ended_at   TEXT,
      summary    TEXT
    );
    CREATE TABLE memories (
      memory_id    TEXT PRIMARY KEY,
      agent_id     TEXT NOT NULL,
      task_context TEXT NOT NULL,
      observation  TEXT NOT NULL,
      importance   INTEGER NOT NULL CHECK(importance BETWEEN 1 AND 10),
      tags_json    TEXT NOT NULL DEFAULT '[]',
      workspace_path TEXT,
      created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );
    CREATE TABLE tasks (
      task_id    TEXT PRIMARY KEY,
      agent_id   TEXT NOT NULL,
      rationale  TEXT NOT NULL,
      test_plan  TEXT NOT NULL,
      status     TEXT NOT NULL DEFAULT 'ACTIVE',
      workspace_path TEXT,
      files_json TEXT NOT NULL DEFAULT '[]',
      created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );
    CREATE TABLE locks (
      lock_id     TEXT PRIMARY KEY,
      file_path   TEXT NOT NULL,
      task_id     TEXT NOT NULL,
      agent_id    TEXT NOT NULL,
      lock_type   TEXT NOT NULL,
      acquired_at TEXT NOT NULL,
      expires_at  TEXT,
      UNIQUE(file_path, task_id)
    );
    INSERT INTO memories (memory_id, agent_id, task_context, observation, importance)
      VALUES ('mem_legacy_1', 'agent-old', 'legacy work', 'a lesson recorded before the schema grew', 6);
  `);
  return db;
}

function fileDigest(path: string): string {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function createIdentityFixture(
  path: string,
  applicationId: number,
  userVersion: number,
): void {
  const db = new DatabaseSync(path);
  db.exec(`
    PRAGMA application_id = ${applicationId};
    PRAGMA user_version = ${userVersion};
    CREATE TABLE identity_marker(value TEXT NOT NULL);
    INSERT INTO identity_marker VALUES ('preserve-me');
  `);
  db.close();
}

describe('canonical v1 schema identity guard', () => {
  it('rejects an unrelated non-empty unversioned store without mutating it', () => {
    const root = mkdtempSync(join(tmpdir(), 'oc-v1-unrelated-'));
    const path = join(root, 'awareness.sqlite3');
    try {
      createIdentityFixture(path, 0, 0);
      const before = fileDigest(path);

      expect(() => connectDb(path)).toThrow(/unrecognized|unversioned|unrelated/i);
      expect(fileDigest(path)).toBe(before);

      const check = new DatabaseSync(path);
      expect(check.prepare('PRAGMA application_id').get()).toEqual({ application_id: 0 });
      expect(check.prepare('PRAGMA user_version').get()).toEqual({ user_version: 0 });
      expect(check.prepare('PRAGMA journal_mode').get()).toEqual({ journal_mode: 'delete' });
      expect(check.prepare('SELECT value FROM identity_marker').get()).toEqual({ value: 'preserve-me' });
      expect(check.prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name").all())
        .toEqual([{ name: 'identity_marker' }]);
      check.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it.each([
    { name: 'staged v4', applicationId: 0x4f435434, userVersion: 4 },
    { name: 'foreign application', applicationId: 1234, userVersion: 3 },
    { name: 'future unbranded schema', applicationId: 0, userVersion: 4 },
  ])('connectDb refuses $name without mutating bytes, journal, schema, or headers', ({ applicationId, userVersion }) => {
    const root = mkdtempSync(join(tmpdir(), 'oc-v1-identity-'));
    const path = join(root, 'awareness.sqlite3');
    try {
      createIdentityFixture(path, applicationId, userVersion);
      const before = fileDigest(path);

      expect(() => connectDb(path)).toThrow(/refusing|unsupported|newer|foreign/i);
      expect(fileDigest(path)).toBe(before);

      const check = new DatabaseSync(path);
      expect(check.prepare('PRAGMA application_id').get()).toEqual({ application_id: applicationId });
      expect(check.prepare('PRAGMA user_version').get()).toEqual({ user_version: userVersion });
      expect(check.prepare('PRAGMA journal_mode').get()).toEqual({ journal_mode: 'delete' });
      expect(check.prepare('SELECT value FROM identity_marker').get()).toEqual({ value: 'preserve-me' });
      expect(check.prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name").all())
        .toEqual([{ name: 'identity_marker' }]);
      check.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it.each([1, 2, 3])('rejects an unrelated versioned store at user_version=%s without mutation', (userVersion) => {
    const root = mkdtempSync(join(tmpdir(), 'oc-v1-versioned-unrelated-'));
    const path = join(root, 'awareness.sqlite3');
    try {
      createIdentityFixture(path, 0, userVersion);
      const before = fileDigest(path);

      expect(() => connectDb(path)).toThrow(/unrecognized|unrelated|unsupported/i);
      expect(fileDigest(path)).toBe(before);

      const check = new DatabaseSync(path);
      expect(check.prepare('PRAGMA application_id').get()).toEqual({ application_id: 0 });
      expect(check.prepare('PRAGMA user_version').get()).toEqual({ user_version: userVersion });
      expect(check.prepare('SELECT value FROM identity_marker').get()).toEqual({ value: 'preserve-me' });
      check.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('rejects a one-table generic sessions database', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      PRAGMA user_version = 2;
      CREATE TABLE sessions(session_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL);
      INSERT INTO sessions VALUES ('s1','someone','2026');
    `);
    expect(() => initDb(db)).toThrow(/unrecognized|unrelated/i);
    expect(db.prepare('PRAGMA user_version').get()).toEqual({ user_version: 2 });
    expect(db.prepare("SELECT session_id FROM sessions").get()).toEqual({ session_id: 's1' });
  });

  it('requires connectDb backup orchestration for a file-backed legacy handle', () => {
    const root = mkdtempSync(join(tmpdir(), 'oc-v1-direct-init-'));
    const path = join(root, 'awareness.sqlite3');
    try {
      const db = new DatabaseSync(path);
      db.exec(`
        CREATE TABLE sessions(session_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL);
        CREATE TABLE memories(memory_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL,
          task_context TEXT NOT NULL, observation TEXT NOT NULL, importance INTEGER NOT NULL);
      `);
      expect(() => initDb(db)).toThrow(/file-backed legacy migration requires connectDb/i);
      expect(db.prepare('PRAGMA application_id').get()).toEqual({ application_id: 0 });
      expect(db.prepare('PRAGMA user_version').get()).toEqual({ user_version: 0 });
      db.close();
      expect(readdirSync(root).filter((name) => name.includes('.pre-v1-'))).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('refuses legacy migration inside a caller-owned transaction', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      CREATE TABLE sessions(session_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL);
      CREATE TABLE memories(memory_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL,
        task_context TEXT NOT NULL, observation TEXT NOT NULL, importance INTEGER NOT NULL);
      BEGIN;
    `);
    expect(() => initDb(db)).toThrow(/caller-owned transaction/i);
    expect(db.isTransaction).toBe(true);
    db.exec('ROLLBACK');
  });

  it.each([
    { applicationId: 1234, userVersion: 3 },
    { applicationId: 0, userVersion: 4 },
  ])('initDb rejects application_id=$applicationId version=$userVersion before adding canonical relations', ({ applicationId, userVersion }) => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      PRAGMA application_id = ${applicationId};
      PRAGMA user_version = ${userVersion};
      CREATE TABLE identity_marker(value TEXT NOT NULL);
      INSERT INTO identity_marker VALUES ('preserve-me');
    `);

    expect(() => initDb(db)).toThrow(/foreign|newer|unsupported/i);
    expect(db.prepare('PRAGMA application_id').get()).toEqual({ application_id: applicationId });
    expect(db.prepare('PRAGMA user_version').get()).toEqual({ user_version: userVersion });
    expect(db.prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name").all())
      .toEqual([{ name: 'identity_marker' }]);
  });

  it('opens an already canonical v1 store', () => {
    const root = mkdtempSync(join(tmpdir(), 'oc-v1-normal-open-'));
    const path = join(root, 'awareness.sqlite3');
    try {
      const seeded = new DatabaseSync(path);
      initDb(seeded);
      seeded.close();

      const reopened = connectDb(path);
      expect(reopened.prepare('PRAGMA application_id').get())
        .toEqual({ application_id: AWARENESS_APPLICATION_ID });
      expect(reopened.prepare('PRAGMA user_version').get())
        .toEqual({ user_version: AWARENESS_SCHEMA_VERSION });
      expect(reopened.prepare("SELECT name FROM sqlite_schema WHERE name='memories'").get())
        .toEqual({ name: 'memories' });
      reopened.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe('legacy store migration', () => {
  it('imports legacy intent history and removes every legacy relation', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      CREATE TABLE agent_intents (
        intent_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, plan_doc_ref TEXT,
        rationale TEXT NOT NULL, test_plan TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'ACTIVE', workspace_path TEXT,
        files_json TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, updated_at TEXT NOT NULL
      );
      CREATE TABLE intent_events (
        event_id TEXT PRIMARY KEY, intent_id TEXT, agent_id TEXT NOT NULL,
        event_type TEXT NOT NULL, message TEXT NOT NULL, created_at TEXT NOT NULL
      );
      CREATE TABLE agent_memories (memory_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL,
        task_context TEXT NOT NULL, observation TEXT NOT NULL, importance_score INTEGER NOT NULL,
        state TEXT NOT NULL DEFAULT 'ACTIVE', label TEXT NOT NULL DEFAULT 'OTHER',
        superseded_by TEXT, tags_json TEXT NOT NULL DEFAULT '[]', tags_text TEXT NOT NULL DEFAULT ',',
        file_tree_fingerprint TEXT, file TEXT, created_at TEXT NOT NULL, updated_at TEXT);
      CREATE TABLE file_locks (lock_id TEXT PRIMARY KEY, file_path TEXT NOT NULL,
        intent_id TEXT NOT NULL, agent_id TEXT NOT NULL, lock_type TEXT NOT NULL,
        acquired_at TEXT NOT NULL, expires_at TEXT);
      CREATE TABLE notifications (notification_id TEXT PRIMARY KEY, workspace_path TEXT NOT NULL,
        repo TEXT, ref TEXT, from_agent TEXT NOT NULL, to_agent TEXT, kind TEXT NOT NULL,
        subject TEXT NOT NULL, body TEXT, files_json TEXT NOT NULL DEFAULT '[]',
        refs_json TEXT NOT NULL DEFAULT '[]', thread_id TEXT NOT NULL, in_reply_to TEXT,
        importance INTEGER NOT NULL DEFAULT 5, status TEXT NOT NULL DEFAULT 'open', created_at TEXT NOT NULL);
      CREATE TABLE notification_reads (notification_id TEXT NOT NULL, agent_id TEXT NOT NULL,
        read_at TEXT NOT NULL, PRIMARY KEY(notification_id, agent_id));
      INSERT INTO agent_intents VALUES (
        'intent_legacy', 'agent-old', 'docs/PLAN.md', 'legacy work', 'legacy tests',
        'PENDING', '/repo', '["/repo/a.ts"]', '2026-01-01T00:00:00Z', '2026-01-01T00:01:00Z'
      );
      INSERT INTO intent_events VALUES (
        'event_legacy', 'intent_legacy', 'agent-old', 'RELEASE', 'awaiting verification',
        '2026-01-01T00:01:00Z'
      );
    `);

    initDb(db);

    expect(db.prepare(`SELECT run_id, origin, agent_id, context_ref, status
      FROM task_runs WHERE run_id = 'intent_legacy'`).get()).toEqual({
      run_id: 'intent_legacy', origin: 'WORK', agent_id: 'agent-old',
      context_ref: 'docs/PLAN.md', status: 'PENDING',
    });
    expect(db.prepare(`SELECT run_id, file_path, source, ended_at
      FROM run_files WHERE run_id = 'intent_legacy'`).get()).toEqual({
      run_id: 'intent_legacy', file_path: '/repo/a.ts', source: 'EXPLICIT',
      ended_at: '2026-01-01T00:01:00Z',
    });
    expect(db.prepare(`SELECT event_id, run_id, event_type
      FROM run_log WHERE event_id = 'event_legacy'`).get()).toEqual({
      event_id: 'event_legacy', run_id: 'intent_legacy', event_type: 'RELEASE',
    });
    const legacy = db.prepare(`SELECT name FROM sqlite_schema WHERE type='table' AND name IN (
      'agent_intents','intent_events','agent_memories','file_locks',
      'notifications','notification_reads','memory_fts'
    ) ORDER BY name`).all();
    expect(legacy).toEqual([]);
    expect(db.prepare('PRAGMA application_id').get())
      .toEqual({ application_id: AWARENESS_APPLICATION_ID });
    expect(db.prepare('PRAGMA user_version').get())
      .toEqual({ user_version: AWARENESS_SCHEMA_VERSION });
  });

  it('repairs missing primary, foreign-key, and unique constraints', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      PRAGMA user_version = 2;
      CREATE TABLE sessions(session_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL);
      CREATE TABLE task_paths(task_id TEXT NOT NULL, path TEXT NOT NULL, ordinal INTEGER NOT NULL DEFAULT 0);
    `);

    initDb(db);

    expect(() => db.exec("INSERT INTO task_paths VALUES ('missing-task','src/a.ts',0)"))
      .toThrow(/foreign key/i);
    const plan = db.prepare(`INSERT INTO plans
      (plan_id,name,objective,lead_agent_id,status,workspace_path,doc_dir,created_at,updated_at)
      VALUES ('plan-a','A','A','agent-a','ACTIVE','/repo','.octocode/plan/a','2026','2026')`);
    plan.run();
    db.exec(`INSERT INTO tasks
      (task_id,plan_id,title,reasoning,acceptance_criteria,status,priority,created_by,created_at,updated_at)
      VALUES ('task-a','plan-a','A','A','A','OPEN',0,'agent-a','2026','2026')`);
    db.exec("INSERT INTO task_paths VALUES ('task-a','src/a.ts',0)");
    expect(() => db.exec("INSERT INTO task_paths VALUES ('task-a','src/a.ts',0)"))
      .toThrow(/unique/i);
  });

  it('rolls back identity and rows when canonical rebuild encounters invalid legacy data', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      PRAGMA user_version = 2;
      CREATE TABLE sessions(session_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL);
      CREATE TABLE task_paths(task_id TEXT NOT NULL, path TEXT NOT NULL, ordinal INTEGER NOT NULL DEFAULT 0);
      INSERT INTO task_paths VALUES ('task-a','src/a.ts',0),('task-a','src/a.ts',0);
    `);

    expect(() => initDb(db)).toThrow(/unique/i);
    expect(db.prepare('PRAGMA application_id').get()).toEqual({ application_id: 0 });
    expect(db.prepare('PRAGMA user_version').get()).toEqual({ user_version: 2 });
    expect(db.prepare('SELECT COUNT(*) AS count FROM task_paths').get()).toEqual({ count: 2 });
  });

  it('creates a readable pre-v1 backup before migrating a file-backed legacy store', () => {
    const root = mkdtempSync(join(tmpdir(), 'oc-v1-backup-'));
    const path = join(root, 'awareness.sqlite3');
    try {
      const legacy = new DatabaseSync(path);
      legacy.exec(`
        CREATE TABLE sessions(session_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL);
        CREATE TABLE memories (
          memory_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, task_context TEXT NOT NULL,
          observation TEXT NOT NULL, importance INTEGER NOT NULL, tags_json TEXT NOT NULL DEFAULT '[]',
          workspace_path TEXT, created_at TEXT NOT NULL
        );
        INSERT INTO memories VALUES ('mem-backup','agent','ctx','obs',7,'[]','/repo','2026');
      `);
      legacy.close();

      const migrated = connectDb(path);
      migrated.close();

      const backups = readdirSync(root).filter((name) => name.includes('.pre-v1-') && name.endsWith('.sqlite3'));
      expect(backups).toHaveLength(1);
      const backup = new DatabaseSync(join(root, backups[0]!));
      expect(backup.prepare('PRAGMA application_id').get()).toEqual({ application_id: 0 });
      expect(backup.prepare('PRAGMA user_version').get()).toEqual({ user_version: 0 });
      expect(backup.prepare("SELECT memory_id FROM memories WHERE memory_id='mem-backup'").get())
        .toEqual({ memory_id: 'mem-backup' });
      backup.close();
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
  it('initDb succeeds on a pre-bitemporal store (indexes reference migrated columns)', () => {
    const db = legacyDb();
    // Before the fix this threw: "no such column: failure_signature"
    expect(() => initDb(db)).not.toThrow();
  });

  it('backfills every canonical column on every pre-existing table', () => {
    const db = legacyDb();
    initDb(db);
    const memories = tableColumns(db, 'memories');
    for (const col of [
      'state', 'label', 'superseded_by', 'artifact', 'repo', 'ref',
      'file_tree_fingerprint', 'novelty_score', 'last_accessed_at',
      'access_count', 'decay_half_life_days', 'failure_signature',
      'valid_from', 'valid_to', 'expired_at', 'embedding', 'embedding_model',
      'updated_at',
    ]) {
      expect(memories.has(col), `memories.${col}`).toBe(true);
    }
    expect(tableColumns(db, 'task_runs').has('session_id')).toBe(true);
    expect(tableColumns(db, 'task_runs').has('artifact')).toBe(true);
    expect(tableColumns(db, 'task_runs').has('context_ref')).toBe(true);
    expect(tableColumns(db, 'task_runs').has('origin')).toBe(true);
    expect(tableColumns(db, 'task_runs').has('files_json')).toBe(false);
    expect(tableColumns(db, 'tasks').has('plan_id')).toBe(true);
    expect(tableColumns(db, 'locks')).toEqual(new Set([
      'lock_id', 'file_path', 'run_id', 'acquired_at', 'expires_at',
    ]));
    expect(tableColumns(db, 'sessions').has('artifact')).toBe(true);
    expect(tableColumns(db, 'sessions').has('repo')).toBe(true);
  });

  it('constant defaults apply to pre-existing rows (NOT NULL columns stay usable)', () => {
    const db = legacyDb();
    initDb(db);
    const row = db.prepare(
      'SELECT state, label, access_count FROM memories WHERE memory_id = ?'
    ).get('mem_legacy_1') as { state: string; label: string; access_count: number };
    expect(row.state).toBe('ACTIVE');
    expect(row.label).toBe('OTHER');
    expect(row.access_count).toBe(0);
  });

  it('migrated store handles the full read/write loop', () => {
    const db = legacyDb();
    initDb(db);
    insertMemory(db, {
      taskContext: 'post-upgrade work',
      observation: 'new memory written after migration',
      importance: 7,
      label: 'GOTCHA',
    });
    const { memories } = getMemory(db, { query: 'post-upgrade work migration', limit: 5 });
    expect(memories.length).toBeGreaterThan(0);
    // The pre-existing row is also readable through the current query path.
    const all = db.prepare('SELECT COUNT(*) AS cnt FROM memories').get() as { cnt: number };
    expect(all.cnt).toBe(2);
  });

  it('initDb is idempotent on an already-migrated store', () => {
    const db = legacyDb();
    initDb(db);
    expect(() => initDb(db)).not.toThrow();
  });

  it('moves legacy edit tasks to task_runs without inventing plan tasks', () => {
    const db = legacyDb();
    db.prepare(`INSERT INTO tasks
      (task_id, agent_id, rationale, test_plan, status, workspace_path, files_json)
      VALUES ('task_legacy', 'agent-old', 'edit a file', 'run tests', 'PENDING', '/repo', '["/repo/a.ts"]')`)
      .run();

    initDb(db);

    expect(db.prepare('SELECT run_id, task_id, origin, status FROM task_runs WHERE run_id = ?')
      .get('task_legacy')).toEqual({ run_id: 'task_legacy', task_id: null, origin: 'HOOK', status: 'PENDING' });
    expect(db.prepare('SELECT run_id, file_path, source FROM run_files WHERE run_id = ?')
      .get('task_legacy')).toEqual({ run_id: 'task_legacy', file_path: '/repo/a.ts', source: 'HOOK' });
    expect(db.prepare('SELECT COUNT(*) AS count FROM tasks').get()).toEqual({ count: 0 });
    expect(db.prepare('PRAGMA application_id').get()).toEqual({ application_id: AWARENESS_APPLICATION_ID });
    expect(db.prepare('PRAGMA user_version').get()).toEqual({ user_version: AWARENESS_SCHEMA_VERSION });
  });

  it('migrates v2 run files and exclusive locks without duplicated identity columns', () => {
    const db = new DatabaseSync(':memory:');
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
      INSERT INTO task_runs VALUES (
        'run_v2', NULL, 'agent-v2', 'session-v2', 'legacy edit', 'legacy test', NULL,
        'ACTIVE', '/repo', NULL, '["/repo/a.ts","/repo/b.ts"]',
        '2026-01-01T00:00:00Z', '2026-01-01T00:01:00Z'
      );
      INSERT INTO locks VALUES (
        'lock_v2', '/repo/a.ts', 'run_v2', 'agent-v2', 'session-v2', 'SHARED',
        '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z'
      );
    `);

    initDb(db);

    expect(db.prepare('SELECT origin FROM task_runs WHERE run_id = ?').get('run_v2'))
      .toEqual({ origin: 'HOOK' });
    expect(db.prepare('SELECT file_path FROM run_files WHERE run_id = ? ORDER BY file_path').all('run_v2'))
      .toEqual([{ file_path: '/repo/a.ts' }, { file_path: '/repo/b.ts' }]);
    expect(db.prepare('SELECT * FROM locks WHERE lock_id = ?').get('lock_v2'))
      .toEqual({
        lock_id: 'lock_v2', file_path: '/repo/a.ts', run_id: 'run_v2',
        acquired_at: '2026-01-01T00:00:00Z', expires_at: '2099-01-01T00:00:00Z',
      });
    expect(tableColumns(db, 'task_runs').has('files_json')).toBe(false);
    expect(tableColumns(db, 'locks').has('lock_type')).toBe(false);
  });

  it('widens legacy refinement quality checks for instructions feedback', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      CREATE TABLE sessions(session_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL);
      CREATE TABLE refinements (
        refinement_id  TEXT PRIMARY KEY,
        agent_id       TEXT NOT NULL,
        workspace_path TEXT NOT NULL,
        artifact       TEXT,
        repo           TEXT,
        ref            TEXT,
        files_json     TEXT NOT NULL DEFAULT '[]',
        reasoning      TEXT NOT NULL,
        remember       TEXT NOT NULL,
        quality        TEXT NOT NULL CHECK(quality IN ('good','bad','handoff')) DEFAULT 'good',
        state          TEXT NOT NULL CHECK(state IN ('open','ongoing','done')) DEFAULT 'open',
        created_at     TEXT NOT NULL,
        updated_at     TEXT NOT NULL
      );
      INSERT INTO refinements (
        refinement_id, agent_id, workspace_path, files_json, reasoning, remember,
        quality, state, created_at, updated_at
      )
      VALUES (
        'ref_legacy', 'agent-old', '/repo', '[]', 'old handoff', 'keep it',
        'handoff', 'open', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
      );
    `);

    initDb(db);
    const schema = db.prepare(
      "SELECT sql FROM sqlite_master WHERE type='table' AND name='refinements'"
    ).get() as { sql: string };
    expect(schema.sql).toContain("'instructions'");

    const { refinement } = insertRefinement(db, {
      agentId: 'agent-new',
      workspacePath: '/repo',
      reasoning: 'instructions feedback',
      remember: 'clarify hook install flow',
      quality: 'instructions',
    });
    expect(refinement.quality).toBe('instructions');
    const count = db.prepare('SELECT COUNT(*) AS cnt FROM refinements')
      .get() as { cnt: number };
    expect(count.cnt).toBe(2);
  });

  // Generic CHECK-constraint drift repair (migrateCheckConstraints): the
  // column-only migration cannot widen a CHECK, so an old store whose enum is
  // narrower than the current DDL threw "CHECK constraint failed" on any insert
  // using a newer value. This must now be repaired for ANY table, not just the
  // hand-written refinements case.
  it('widens a legacy harness_log event_type CHECK and preserves its rows', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      CREATE TABLE sessions(session_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, started_at TEXT NOT NULL);
      CREATE TABLE harness_log (
        harness_id   TEXT PRIMARY KEY,
        session_id   TEXT,
        agent_id     TEXT NOT NULL,
        workspace_path TEXT,
        artifact     TEXT,
        event_type   TEXT NOT NULL CHECK(event_type IN ('mine','propose')),
        payload_json TEXT,
        memory_id    TEXT,
        task_id      TEXT,
        created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
      );
      INSERT INTO harness_log (harness_id, agent_id, event_type)
        VALUES ('hl_legacy', 'agent-old', 'mine');
    `);

    initDb(db);

    // The legacy row survives the table rebuild.
    const preserved = db.prepare(
      "SELECT COUNT(*) AS c FROM harness_log WHERE harness_id='hl_legacy'"
    ).get() as { c: number };
    expect(preserved.c).toBe(1);

    // Event types the narrow CHECK rejected now insert cleanly.
    for (const ev of ['reflect', 'validate', 'apply', 'capture'] as const) {
      expect(() => insertHarnessLog(db, { agentId: 'agent-new', eventType: ev }), ev).not.toThrow();
    }
  });

  it('does not rebuild a current-schema store (CHECK migration is a no-op)', () => {
    const db = new DatabaseSync(':memory:');
    initDb(db);
    insertHarnessLog(db, { agentId: 'a', eventType: 'reflect' });
    const before = db.prepare(
      "SELECT sql FROM sqlite_master WHERE type='table' AND name='harness_log'"
    ).get() as { sql: string };

    initDb(db); // second init must not touch the table

    const after = db.prepare(
      "SELECT sql FROM sqlite_master WHERE type='table' AND name='harness_log'"
    ).get() as { sql: string };
    expect(after.sql).toBe(before.sql);
    const rows = db.prepare('SELECT COUNT(*) AS c FROM harness_log').get() as { c: number };
    expect(rows.c).toBe(1);
  });
});
