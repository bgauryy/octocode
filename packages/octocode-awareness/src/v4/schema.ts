import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';

/** ASCII "OCT4", stored in SQLite's application_id header field. */
export const V4_APPLICATION_ID = 0x4f435434;
export const V4_SCHEMA_VERSION = 4;

/**
 * The public storage contract: 15 ordinary tables plus one FTS5 virtual table.
 * SQLite-owned FTS shadow tables and indexes are deliberately not relations in
 * this contract.
 */
export const V4_RELATION_NAMES = [
  'agents',
  'sessions',
  'plans',
  'plan_docs',
  'tasks',
  'task_paths',
  'task_dependencies',
  'runs',
  'run_files',
  'activity_events',
  'signals',
  'signal_recipients',
  'deliveries',
  'memories',
  'memory_refs',
  'memories_fts',
] as const;

export type V4RelationName = typeof V4_RELATION_NAMES[number];
export type V4SchemaInitialization = 'created' | 'ready';

export const V4_SCHEMA_DDL = `
  CREATE TABLE agents (
    agent_id      TEXT PRIMARY KEY,
    agent_name    TEXT NOT NULL DEFAULT '',
    registered_at TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL
  );

  CREATE TABLE sessions (
    session_id      TEXT PRIMARY KEY,
    host            TEXT NOT NULL,
    host_session_id TEXT,
    agent_id        TEXT NOT NULL REFERENCES agents(agent_id),
    workspace_path  TEXT NOT NULL,
    artifact        TEXT,
    goal            TEXT,
    goal_source     TEXT,
    started_at      TEXT NOT NULL,
    heartbeat_at    TEXT NOT NULL,
    ended_at        TEXT,
    summary         TEXT,
    CHECK (ended_at IS NULL OR ended_at >= started_at)
  );

  CREATE TABLE plans (
    plan_id        TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    objective      TEXT NOT NULL,
    lead_agent_id  TEXT NOT NULL REFERENCES agents(agent_id),
    status         TEXT NOT NULL CHECK (status IN ('ACTIVE', 'PAUSED', 'COMPLETED', 'CANCELLED')),
    workspace_path TEXT NOT NULL,
    artifact       TEXT,
    doc_dir        TEXT NOT NULL UNIQUE,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
  );

  CREATE TABLE plan_docs (
    plan_id         TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
    relative_path   TEXT NOT NULL,
    title           TEXT NOT NULL,
    author_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    ordinal         INTEGER NOT NULL DEFAULT 0 CHECK (ordinal >= 0),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    PRIMARY KEY (plan_id, relative_path),
    CHECK (relative_path GLOB 'docs/*' AND relative_path NOT LIKE '%..%')
  );

  CREATE TABLE tasks (
    task_id             TEXT PRIMARY KEY,
    plan_id             TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
    title               TEXT NOT NULL,
    reasoning           TEXT NOT NULL,
    acceptance_criteria TEXT NOT NULL,
    status              TEXT NOT NULL CHECK (status IN ('OPEN', 'BLOCKED', 'DONE', 'FAILED', 'CANCELLED')),
    priority            INTEGER NOT NULL DEFAULT 5 CHECK (priority BETWEEN 0 AND 10),
    created_by_agent_id TEXT NOT NULL REFERENCES agents(agent_id),
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    completed_at        TEXT,
    CHECK (
      (status IN ('DONE', 'FAILED', 'CANCELLED') AND completed_at IS NOT NULL)
      OR (status IN ('OPEN', 'BLOCKED') AND completed_at IS NULL)
    )
  );

  CREATE TABLE task_paths (
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    path    TEXT NOT NULL,
    ordinal INTEGER NOT NULL DEFAULT 0 CHECK (ordinal >= 0),
    PRIMARY KEY (task_id, path)
  );

  CREATE TABLE task_dependencies (
    task_id            TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    depends_on_task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, depends_on_task_id),
    CHECK (task_id <> depends_on_task_id)
  );

  CREATE TABLE runs (
    run_id          TEXT PRIMARY KEY,
    task_id         TEXT REFERENCES tasks(task_id) ON DELETE RESTRICT,
    kind            TEXT NOT NULL CHECK (kind IN ('TASK', 'WORK')),
    source          TEXT NOT NULL CHECK (source IN ('EXPLICIT', 'SESSION')),
    agent_id        TEXT NOT NULL REFERENCES agents(agent_id),
    session_id      TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
    rationale       TEXT NOT NULL,
    test_plan       TEXT NOT NULL,
    context_ref     TEXT,
    state           TEXT NOT NULL CHECK (state IN ('ACTIVE', 'SUCCESS', 'FAILED', 'ABANDONED')),
    heartbeat_at    TEXT NOT NULL,
    lease_expires_at TEXT NOT NULL,
    workspace_path  TEXT NOT NULL,
    artifact        TEXT,
    result_evidence TEXT,
    evidence_kind   TEXT CHECK (
      evidence_kind IS NULL
      OR evidence_kind IN ('AUTOMATED', 'INDEPENDENT', 'USER', 'SELF_REPORT')
    ),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    completed_at    TEXT,
    CHECK (
      (kind = 'TASK' AND task_id IS NOT NULL)
      OR (kind = 'WORK' AND task_id IS NULL)
    ),
    CHECK (source <> 'SESSION' OR (kind = 'WORK' AND session_id IS NOT NULL)),
    CHECK (
      (state = 'ACTIVE' AND completed_at IS NULL AND result_evidence IS NULL AND evidence_kind IS NULL)
      OR (state = 'ABANDONED' AND completed_at IS NOT NULL)
      OR (
        state IN ('SUCCESS', 'FAILED')
        AND completed_at IS NOT NULL
        AND result_evidence IS NOT NULL
        AND evidence_kind IS NOT NULL
      )
    )
  );

  CREATE TABLE run_files (
    run_id               TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
    file_path            TEXT NOT NULL,
    source               TEXT NOT NULL CHECK (source IN ('DECLARED', 'OBSERVED')),
    reason_override      TEXT,
    started_at           TEXT NOT NULL,
    heartbeat_at         TEXT NOT NULL,
    expires_at           TEXT NOT NULL,
    ended_at             TEXT,
    exclusive_expires_at TEXT,
    PRIMARY KEY (run_id, file_path),
    CHECK (ended_at IS NULL OR exclusive_expires_at IS NULL)
  );

  CREATE TABLE activity_events (
    event_id        TEXT PRIMARY KEY,
    event_type      TEXT NOT NULL,
    workspace_path  TEXT NOT NULL,
    artifact        TEXT,
    agent_id        TEXT NOT NULL REFERENCES agents(agent_id),
    session_id      TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
    plan_id         TEXT REFERENCES plans(plan_id) ON DELETE SET NULL,
    task_id         TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
    run_id          TEXT REFERENCES runs(run_id) ON DELETE SET NULL,
    file_path       TEXT,
    old_file_path   TEXT,
    message         TEXT NOT NULL,
    payload_json    TEXT NOT NULL DEFAULT '{}',
    host_event_id   TEXT,
    created_at      TEXT NOT NULL
  );

  CREATE TABLE signals (
    signal_id       TEXT PRIMARY KEY,
    workspace_path  TEXT NOT NULL,
    artifact        TEXT,
    from_agent_id   TEXT NOT NULL REFERENCES agents(agent_id),
    kind            TEXT NOT NULL CHECK (
      kind IN ('HANDOFF', 'QUESTION', 'BLOCKER', 'REQUEST', 'DECISION', 'FYI', 'REVIEW')
    ),
    subject         TEXT NOT NULL,
    body            TEXT,
    files_json      TEXT NOT NULL DEFAULT '[]',
    refs_json       TEXT NOT NULL DEFAULT '[]',
    thread_id       TEXT NOT NULL,
    reply_to        TEXT REFERENCES signals(signal_id) ON DELETE SET NULL,
    importance      INTEGER NOT NULL DEFAULT 5 CHECK (importance BETWEEN 1 AND 10),
    status          TEXT NOT NULL DEFAULT 'OPEN' CHECK (status IN ('OPEN', 'RESOLVED')),
    created_at      TEXT NOT NULL,
    resolved_at     TEXT,
    CHECK (
      (status = 'OPEN' AND resolved_at IS NULL)
      OR (status = 'RESOLVED' AND resolved_at IS NOT NULL)
    )
  );

  CREATE TABLE signal_recipients (
    signal_id TEXT NOT NULL REFERENCES signals(signal_id) ON DELETE CASCADE,
    agent_id  TEXT NOT NULL REFERENCES agents(agent_id),
    read_at   TEXT,
    PRIMARY KEY (signal_id, agent_id)
  );

  CREATE TABLE deliveries (
    consumer_id TEXT NOT NULL,
    channel     TEXT NOT NULL,
    scope_key   TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    delivered_at TEXT NOT NULL,
    PRIMARY KEY (consumer_id, channel, scope_key)
  );

  CREATE TABLE memories (
    memory_id           TEXT PRIMARY KEY,
    created_by_agent_id TEXT REFERENCES agents(agent_id) ON DELETE SET NULL,
    task_context        TEXT NOT NULL,
    observation         TEXT NOT NULL,
    importance          INTEGER NOT NULL CHECK (importance BETWEEN 1 AND 10),
    tags_json           TEXT NOT NULL DEFAULT '[]',
    label               TEXT,
    workspace_path      TEXT,
    artifact            TEXT,
    repo                TEXT,
    ref                 TEXT,
    state               TEXT NOT NULL CHECK (state IN ('CANDIDATE', 'ACTIVE', 'QUARANTINED', 'RETIRED')),
    confidence          REAL NOT NULL DEFAULT 0 CHECK (confidence BETWEEN 0 AND 1),
    evidence_kind       TEXT CHECK (
      evidence_kind IS NULL
      OR evidence_kind IN ('AUTOMATED', 'INDEPENDENT', 'USER', 'SELF_REPORT')
    ),
    helpful_count       INTEGER NOT NULL DEFAULT 0 CHECK (helpful_count >= 0),
    harmful_count       INTEGER NOT NULL DEFAULT 0 CHECK (harmful_count >= 0),
    last_validated_at   TEXT,
    superseded_by       TEXT REFERENCES memories(memory_id) ON DELETE SET NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    CHECK (
      state <> 'ACTIVE'
      OR COALESCE(evidence_kind, '') IN ('AUTOMATED', 'INDEPENDENT', 'USER')
    ),
    CHECK (superseded_by IS NULL OR superseded_by <> memory_id)
  );

  CREATE TABLE memory_refs (
    memory_id TEXT NOT NULL REFERENCES memories(memory_id) ON DELETE CASCADE,
    ref_type  TEXT NOT NULL CHECK (
      ref_type IN ('EVENT', 'RUN', 'TASK', 'PLAN', 'SIGNAL', 'FILE', 'SOURCE')
    ),
    ref_id    TEXT NOT NULL,
    ordinal   INTEGER NOT NULL DEFAULT 0 CHECK (ordinal >= 0),
    PRIMARY KEY (memory_id, ref_type, ref_id)
  );

  CREATE VIRTUAL TABLE memories_fts
  USING fts5(memory_id UNINDEXED, task_context, observation, tags);

  CREATE UNIQUE INDEX sessions_host_identity
    ON sessions(host, host_session_id, workspace_path, ifnull(artifact, ''))
    WHERE host_session_id IS NOT NULL;
  CREATE INDEX sessions_agent_active
    ON sessions(agent_id, ended_at, heartbeat_at);

  CREATE INDEX plans_scope_status
    ON plans(workspace_path, artifact, status, updated_at);
  CREATE INDEX tasks_plan_status
    ON tasks(plan_id, status, priority DESC, created_at);
  CREATE INDEX task_dependencies_dependency
    ON task_dependencies(depends_on_task_id);

  CREATE UNIQUE INDEX runs_one_active_per_session
    ON runs(session_id)
    WHERE state = 'ACTIVE' AND session_id IS NOT NULL;
  CREATE UNIQUE INDEX runs_one_active_per_task
    ON runs(task_id)
    WHERE state = 'ACTIVE' AND task_id IS NOT NULL;
  CREATE INDEX runs_scope_state
    ON runs(workspace_path, artifact, state, lease_expires_at);
  CREATE INDEX runs_agent_state
    ON runs(agent_id, state, updated_at);
  CREATE INDEX run_files_active_path
    ON run_files(file_path, ended_at, expires_at, exclusive_expires_at);

  CREATE UNIQUE INDEX activity_events_host_dedupe
    ON activity_events(session_id, host_event_id)
    WHERE session_id IS NOT NULL AND host_event_id IS NOT NULL;
  CREATE INDEX activity_events_scope_time
    ON activity_events(workspace_path, artifact, created_at);
  CREATE INDEX activity_events_type_time
    ON activity_events(event_type, created_at);
  CREATE INDEX activity_events_task_time
    ON activity_events(task_id, created_at);
  CREATE INDEX activity_events_run_time
    ON activity_events(run_id, created_at);
  CREATE INDEX activity_events_file_time
    ON activity_events(file_path, created_at);

  CREATE INDEX signals_scope_status
    ON signals(workspace_path, artifact, status, created_at);
  CREATE INDEX signals_thread
    ON signals(thread_id, created_at);
  CREATE INDEX signal_recipients_agent_unread
    ON signal_recipients(agent_id, read_at);

  CREATE INDEX memories_scope_state
    ON memories(workspace_path, artifact, state, importance DESC, updated_at);
  CREATE INDEX memories_validation
    ON memories(state, last_validated_at);
  CREATE INDEX memory_refs_target
    ON memory_refs(ref_type, ref_id);
`;

interface SchemaIdentity {
  applicationId: number;
  userVersion: number;
  userRelations: string[];
}

interface SchemaObject {
  type: string;
  name: string;
  tableName: string;
  sql: string;
}

function readSchemaObjects(db: DatabaseSync): SchemaObject[] {
  const rows = db.prepare(`
    SELECT type, name, tbl_name, sql
    FROM sqlite_schema
    WHERE type IN ('table', 'view', 'index', 'trigger')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
    ORDER BY type, name
  `).all() as Array<{ type: string; name: string; tbl_name: string; sql: string | null }>;
  return rows.map((row) => ({
    type: row.type,
    name: row.name,
    tableName: row.tbl_name,
    sql: (row.sql ?? '').replace(/\s+/g, ' ').trim(),
  }));
}

function schemaFingerprint(objects: SchemaObject[]): string {
  return createHash('sha256').update(JSON.stringify(objects)).digest('hex');
}

let canonicalFingerprint: string | undefined;

function v4CanonicalFingerprint(): string {
  if (canonicalFingerprint) return canonicalFingerprint;
  const canonical = new DatabaseSync(':memory:');
  try {
    canonical.exec(V4_SCHEMA_DDL);
    canonicalFingerprint = schemaFingerprint(readSchemaObjects(canonical));
    return canonicalFingerprint;
  } finally {
    canonical.close();
  }
}

function pragmaNumber(db: DatabaseSync, name: 'application_id' | 'user_version'): number {
  const row = db.prepare(`PRAGMA ${name}`).get() as Record<string, number>;
  return row[name] ?? 0;
}

function readIdentity(db: DatabaseSync): SchemaIdentity {
  const rows = db.prepare(`
    SELECT name
    FROM sqlite_schema
    WHERE type IN ('table', 'view')
      AND name NOT LIKE 'sqlite_%'
      AND name NOT GLOB 'memories_fts_*'
    ORDER BY name
  `).all() as Array<{ name: string }>;
  return {
    applicationId: pragmaNumber(db, 'application_id'),
    userVersion: pragmaNumber(db, 'user_version'),
    userRelations: rows.map(({ name }) => name),
  };
}

function isFresh(identity: SchemaIdentity): boolean {
  return identity.applicationId === 0
    && identity.userVersion === 0
    && identity.userRelations.length === 0;
}

function isV4(identity: SchemaIdentity): boolean {
  return identity.applicationId === V4_APPLICATION_ID
    && identity.userVersion === V4_SCHEMA_VERSION;
}

function assertV4RelationContract(identity: SchemaIdentity): void {
  const actual = new Set(identity.userRelations);
  const expected = new Set<string>(V4_RELATION_NAMES);
  const missing = V4_RELATION_NAMES.filter((name) => !actual.has(name));
  const unexpected = identity.userRelations.filter((name) => !expected.has(name));
  if (missing.length === 0 && unexpected.length === 0) return;

  const details = [
    missing.length > 0 ? `missing: ${missing.join(', ')}` : null,
    unexpected.length > 0 ? `unexpected: ${unexpected.join(', ')}` : null,
  ].filter((value): value is string => value !== null).join('; ');
  throw new Error(`v4 relation contract mismatch (${details})`);
}

function assertV4SchemaFingerprint(db: DatabaseSync): void {
  const expected = v4CanonicalFingerprint();
  const actual = schemaFingerprint(readSchemaObjects(db));
  if (actual !== expected) {
    throw new Error(`v4 schema fingerprint mismatch (expected ${expected}, got ${actual})`);
  }
}

function unsupportedSchemaError(identity: SchemaIdentity): Error {
  if (identity.applicationId !== 0 && identity.applicationId !== V4_APPLICATION_ID) {
    return new Error(
      `refusing foreign application_id ${identity.applicationId}; expected ${V4_APPLICATION_ID}`,
    );
  }
  if (identity.applicationId === 0 && identity.userVersion === 0 && identity.userRelations.length > 0) {
    return new Error('refusing to initialize non-empty unversioned Awareness store');
  }
  return new Error(
    `unsupported Awareness schema: application_id ${identity.applicationId}, version ${identity.userVersion}; expected v4`,
  );
}

/**
 * Creates only a truly empty v4 store. Normal startup accepts exact v4 and
 * otherwise fails before DDL or data writes; legacy import is a separate,
 * explicit operation.
 */
export function initializeV4Schema(db: DatabaseSync): V4SchemaInitialization {
  if (db.isTransaction) {
    throw new Error('cannot initialize Awareness v4 inside an active transaction');
  }

  db.exec('PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000');

  const initial = readIdentity(db);
  if (isV4(initial)) {
    assertV4RelationContract(initial);
    assertV4SchemaFingerprint(db);
    return 'ready';
  }
  if (!isFresh(initial)) throw unsupportedSchemaError(initial);

  let transactionOpen = false;
  try {
    db.exec('BEGIN IMMEDIATE');
    transactionOpen = true;

    // A concurrent initializer may have completed while this connection was
    // waiting for the write lock. Re-read identity while holding that lock.
    const locked = readIdentity(db);
    if (isV4(locked)) {
      assertV4RelationContract(locked);
      assertV4SchemaFingerprint(db);
      db.exec('COMMIT');
      transactionOpen = false;
      return 'ready';
    }
    if (!isFresh(locked)) throw unsupportedSchemaError(locked);

    db.exec(V4_SCHEMA_DDL);
    db.exec(`PRAGMA application_id = ${V4_APPLICATION_ID}`);
    db.exec(`PRAGMA user_version = ${V4_SCHEMA_VERSION}`);
    assertV4RelationContract(readIdentity(db));
    assertV4SchemaFingerprint(db);
    db.exec('COMMIT');
    transactionOpen = false;
    return 'created';
  } catch (error) {
    if (transactionOpen) {
      try {
        db.exec('ROLLBACK');
      } catch {
        // Preserve the original initialization error.
      }
    }
    throw error;
  }
}
