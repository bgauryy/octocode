import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import {
  V4_APPLICATION_ID,
  V4_RELATION_NAMES,
  V4_SCHEMA_VERSION,
  initializeV4Schema,
} from '../src/v4/schema.js';

function freshV4(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  initializeV4Schema(db);
  return db;
}

function pragmaNumber(db: DatabaseSync, name: 'application_id' | 'user_version'): number {
  const row = db.prepare(`PRAGMA ${name}`).get() as Record<string, number>;
  return row[name] ?? -1;
}

function contractRelations(db: DatabaseSync): string[] {
  const rows = db.prepare(`
    SELECT name
    FROM sqlite_schema
    WHERE type = 'table'
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
    ORDER BY name
  `).all() as Array<{ name: string }>;
  return rows.map(({ name }) => name);
}

describe('v4 schema bootstrap', () => {
  it('creates the exact 15-table plus FTS relation contract on an empty store', () => {
    const db = freshV4();

    expect(pragmaNumber(db, 'application_id')).toBe(V4_APPLICATION_ID);
    expect(pragmaNumber(db, 'user_version')).toBe(V4_SCHEMA_VERSION);
    expect(contractRelations(db)).toEqual([...V4_RELATION_NAMES].sort());
    expect(V4_RELATION_NAMES).toHaveLength(16);
    expect(db.prepare("SELECT sql FROM sqlite_schema WHERE name = 'memories_fts'").get())
      .toMatchObject({ sql: expect.stringContaining('VIRTUAL TABLE') });
  });

  it('takes a read-only fast path for an already initialized store', () => {
    const db = freshV4();
    const beforeChanges = db.prepare('SELECT total_changes() AS value').get() as { value: number };

    expect(initializeV4Schema(db)).toBe('ready');

    const afterChanges = db.prepare('SELECT total_changes() AS value').get() as { value: number };
    expect(afterChanges.value).toBe(beforeChanges.value);
    expect(contractRelations(db)).toEqual([...V4_RELATION_NAMES].sort());
  });

  it('refuses a non-empty unversioned store without mutating it', () => {
    const db = new DatabaseSync(':memory:');
    db.exec('CREATE TABLE legacy_marker(value TEXT); INSERT INTO legacy_marker VALUES (\'keep\')');

    expect(() => initializeV4Schema(db)).toThrow(/refusing.*non-empty.*unversioned/i);
    expect(db.prepare('SELECT value FROM legacy_marker').get()).toEqual({ value: 'keep' });
    expect(pragmaNumber(db, 'application_id')).toBe(0);
    expect(pragmaNumber(db, 'user_version')).toBe(0);
  });

  it('refuses a wrong schema version before DDL or writes', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      PRAGMA application_id = ${V4_APPLICATION_ID};
      PRAGMA user_version = 3;
      CREATE TABLE legacy_marker(value TEXT);
      INSERT INTO legacy_marker VALUES ('keep');
    `);
    const beforeChanges = db.prepare('SELECT total_changes() AS value').get() as { value: number };

    expect(() => initializeV4Schema(db)).toThrow(/unsupported awareness schema.*version 3/i);

    expect(db.prepare('SELECT value FROM legacy_marker').get()).toEqual({ value: 'keep' });
    expect(pragmaNumber(db, 'user_version')).toBe(3);
    expect((db.prepare('SELECT total_changes() AS value').get() as { value: number }).value)
      .toBe(beforeChanges.value);
  });

  it('refuses a foreign application id even when user_version is 4', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      PRAGMA application_id = 1234;
      PRAGMA user_version = ${V4_SCHEMA_VERSION};
      CREATE TABLE foreign_marker(value TEXT);
    `);

    expect(() => initializeV4Schema(db)).toThrow(/foreign application_id 1234/i);
    expect(pragmaNumber(db, 'application_id')).toBe(1234);
    expect(pragmaNumber(db, 'user_version')).toBe(V4_SCHEMA_VERSION);
    expect(db.prepare("SELECT name FROM sqlite_schema WHERE name = 'foreign_marker'").get())
      .toEqual({ name: 'foreign_marker' });
  });

  it('refuses a v4 header when the relation contract is incomplete', () => {
    const db = new DatabaseSync(':memory:');
    db.exec(`
      PRAGMA application_id = ${V4_APPLICATION_ID};
      PRAGMA user_version = ${V4_SCHEMA_VERSION};
    `);

    expect(() => initializeV4Schema(db)).toThrow(/v4 relation contract mismatch.*missing/i);
    expect(pragmaNumber(db, 'application_id')).toBe(V4_APPLICATION_ID);
    expect(pragmaNumber(db, 'user_version')).toBe(V4_SCHEMA_VERSION);
  });

  it('refuses unexpected application relations on the v4 fast path', () => {
    const db = freshV4();
    db.exec('CREATE TABLE obsolete_runtime_state(value TEXT)');

    expect(() => initializeV4Schema(db)).toThrow(/v4 relation contract mismatch.*unexpected/i);
    expect(db.prepare("SELECT name FROM sqlite_schema WHERE name = 'obsolete_runtime_state'").get())
      .toEqual({ name: 'obsolete_runtime_state' });
  });

  it('refuses a v4 header whose relation names hide the wrong DDL', () => {
    const db = new DatabaseSync(':memory:');
    for (const relation of V4_RELATION_NAMES) {
      db.exec(`CREATE TABLE ${relation}(wrong_column TEXT)`);
    }
    db.exec(`
      PRAGMA application_id = ${V4_APPLICATION_ID};
      PRAGMA user_version = ${V4_SCHEMA_VERSION};
    `);

    expect(() => initializeV4Schema(db)).toThrow(/schema fingerprint mismatch/i);
  });

  it('rejects initialization inside a caller-owned transaction', () => {
    const db = new DatabaseSync(':memory:');
    db.exec('BEGIN');
    expect(() => initializeV4Schema(db)).toThrow(/active transaction/i);
    expect(db.isTransaction).toBe(true);
    db.exec('ROLLBACK');
  });
});

describe('v4 schema invariants', () => {
  it('enforces run kind/source and one active run per session or task', () => {
    const db = freshV4();
    db.exec(`
      INSERT INTO agents(agent_id, agent_name, registered_at, last_seen_at)
      VALUES ('agent-a', 'A', '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z');
      INSERT INTO sessions(
        session_id, host, host_session_id, agent_id, workspace_path,
        goal, goal_source, started_at, heartbeat_at
      ) VALUES (
        'session-a', 'codex', 'host-a', 'agent-a', '/repo',
        'implement v4', 'HOST', '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      );
      INSERT INTO plans(
        plan_id, name, objective, lead_agent_id, status, workspace_path,
        doc_dir, created_at, updated_at
      ) VALUES (
        'plan-a', 'v4', 'implement v4', 'agent-a', 'ACTIVE', '/repo',
        '.octocode/plan/20260709-v4', '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      );
      INSERT INTO tasks(
        task_id, plan_id, title, reasoning, acceptance_criteria, status,
        priority, created_by_agent_id, created_at, updated_at
      ) VALUES (
        'task-a', 'plan-a', 'schema', 'need one source of truth', 'contract passes', 'OPEN',
        5, 'agent-a', '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      );
      INSERT INTO runs(
        run_id, task_id, kind, source, agent_id, session_id, rationale, test_plan,
        state, heartbeat_at, lease_expires_at, workspace_path, created_at, updated_at
      ) VALUES (
        'run-a', 'task-a', 'TASK', 'EXPLICIT', 'agent-a', 'session-a', 'build schema', 'focused tests',
        'ACTIVE', '2026-07-09T00:00:00Z', '2026-07-09T01:00:00Z', '/repo',
        '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      );
    `);

    expect(() => db.exec(`
      INSERT INTO runs(
        run_id, kind, source, agent_id, session_id, rationale, test_plan, state,
        heartbeat_at, lease_expires_at, workspace_path, created_at, updated_at
      ) VALUES (
        'run-b', 'WORK', 'EXPLICIT', 'agent-a', 'session-a', 'other work', 'tests', 'ACTIVE',
        '2026-07-09T00:00:00Z', '2026-07-09T01:00:00Z', '/repo',
        '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      )
    `)).toThrow(/unique/i);

    expect(() => db.exec(`
      INSERT INTO runs(
        run_id, task_id, kind, source, agent_id, rationale, test_plan, state,
        heartbeat_at, lease_expires_at, workspace_path, created_at, updated_at
      ) VALUES (
        'run-c', 'task-a', 'TASK', 'EXPLICIT', 'agent-a', 'duplicate task', 'tests', 'ACTIVE',
        '2026-07-09T00:00:00Z', '2026-07-09T01:00:00Z', '/repo',
        '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      )
    `)).toThrow(/unique/i);

    expect(() => db.exec(`
      INSERT INTO runs(
        run_id, task_id, kind, source, agent_id, session_id, rationale, test_plan, state,
        heartbeat_at, lease_expires_at, workspace_path, created_at, updated_at
      ) VALUES (
        'run-d', 'task-a', 'TASK', 'SESSION', 'agent-a', 'session-a', 'invalid source', 'tests', 'ACTIVE',
        '2026-07-09T00:00:00Z', '2026-07-09T01:00:00Z', '/repo',
        '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      )
    `)).toThrow(/check constraint/i);
  });

  it('keeps self-report-only learning as a candidate', () => {
    const db = freshV4();
    db.exec(`
      INSERT INTO agents(agent_id, agent_name, registered_at, last_seen_at)
      VALUES ('agent-a', 'A', '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z');
      INSERT INTO memories(
        memory_id, created_by_agent_id, task_context, observation, importance,
        state, confidence, evidence_kind, created_at, updated_at
      ) VALUES (
        'memory-candidate', 'agent-a', 'schema work', 'the schema seems correct', 5,
        'CANDIDATE', 0.4, 'SELF_REPORT', '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      );
    `);

    expect(() => db.exec(`
      INSERT INTO memories(
        memory_id, created_by_agent_id, task_context, observation, importance,
        state, confidence, evidence_kind, created_at, updated_at
      ) VALUES (
        'memory-active', 'agent-a', 'schema work', 'the schema is correct', 5,
        'ACTIVE', 0.4, 'SELF_REPORT', '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      )
    `)).toThrow(/check constraint/i);

    expect(() => db.exec(`
      INSERT INTO memories(
        memory_id, created_by_agent_id, task_context, observation, importance,
        state, confidence, created_at, updated_at
      ) VALUES (
        'memory-unsupported', 'agent-a', 'schema work', 'there is no evidence', 5,
        'ACTIVE', 0.4, '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      )
    `)).toThrow(/check constraint/i);

    expect(() => db.exec(`
      INSERT INTO memories(
        memory_id, created_by_agent_id, task_context, observation, importance,
        state, confidence, evidence_kind, created_at, updated_at
      ) VALUES (
        'memory-verified', 'agent-a', 'schema work', 'focused tests passed', 7,
        'ACTIVE', 0.9, 'AUTOMATED', '2026-07-09T00:00:00Z', '2026-07-09T00:00:00Z'
      )
    `)).not.toThrow();

    expect(db.prepare('SELECT state FROM memories WHERE memory_id = ?').get('memory-candidate'))
      .toEqual({ state: 'CANDIDATE' });
  });
});
