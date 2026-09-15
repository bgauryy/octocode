import { PLAN_STATUSES, TASK_STATUSES, AGENT_STATUSES, PLAN_MEMBER_ROLES, PLAN_DOC_KINDS, TASK_RUN_ORIGINS, TASK_RUN_STATUSES } from '@octocodeai/agent-contracts/entities';
import { sqlEnum } from '@octocodeai/agent-contracts/schema';
import { CONTINUITY_SCHEMA_DDL, EVENT_OUTBOX_TYPED_INDEX_DDL } from './db-continuity-schema.js';
import { LOCAL_HISTORY_INDEX_DDL, LOCAL_HISTORY_SCHEMA_DDL } from './db-history-schema.js';
import { AWARENESS_META_DDL, HOOK_RECEIPTS_DDL } from './db-meta-schema.js';
// ─── Schema ───────────────────────────────────────────────────────────────────

export const SCHEMA_DDL = `
    ${AWARENESS_META_DDL}

    ${CONTINUITY_SCHEMA_DDL}

    CREATE TABLE IF NOT EXISTS sessions (
      session_id     TEXT PRIMARY KEY,
      agent_id       TEXT NOT NULL,
      workspace_path TEXT,
      artifact       TEXT,
      repo           TEXT,
      ref            TEXT,
      started_at     TEXT NOT NULL,
      ended_at       TEXT,
      summary        TEXT
    );

    CREATE TABLE IF NOT EXISTS awareness_memories (
      memory_id             TEXT PRIMARY KEY,
      agent_id              TEXT NOT NULL,
      task_context          TEXT NOT NULL,
      observation           TEXT NOT NULL,
      importance            INTEGER NOT NULL CHECK(importance BETWEEN 1 AND 10),
      state                 TEXT NOT NULL DEFAULT 'ACTIVE' CHECK(state IN ('ACTIVE', 'SUPERSEDED')),
      label                 TEXT NOT NULL DEFAULT 'OTHER',
      superseded_by         TEXT,
      tags_json             TEXT NOT NULL DEFAULT '[]',
      workspace_path        TEXT,
      artifact              TEXT,
      repo                  TEXT,
      ref                   TEXT,
      file_tree_fingerprint TEXT,
      novelty_score         REAL,
      last_accessed_at      TEXT,
      access_count          INTEGER NOT NULL DEFAULT 0,
      decay_half_life_days  REAL,
      failure_signature     TEXT,
      valid_from            TEXT,
      valid_to              TEXT,
      expired_at            TEXT,
      scope_kind            TEXT,
      source_digest         TEXT,
      verified_at           TEXT,
      secret_scan_status    TEXT,
      embedding             BLOB,
      embedding_model       TEXT,
      created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at            TEXT
    );

    CREATE TABLE IF NOT EXISTS awareness_plans (
      plan_id        TEXT PRIMARY KEY,
      name           TEXT NOT NULL,
      objective      TEXT NOT NULL,
      lead_agent_id  TEXT NOT NULL,
      status         TEXT NOT NULL DEFAULT 'DRAFT'
                     CHECK(status IN (${sqlEnum(PLAN_STATUSES)})),
      workspace_path TEXT NOT NULL,
      artifact       TEXT,
      doc_dir        TEXT NOT NULL,
      source_kind    TEXT,
      source_key     TEXT,
      rfc_path       TEXT,
      rfc_revision   TEXT,
      created_at     TEXT NOT NULL,
      updated_at     TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS plan_members (
      plan_id    TEXT NOT NULL REFERENCES awareness_plans(plan_id) ON DELETE CASCADE,
      agent_id   TEXT NOT NULL,
      role       TEXT NOT NULL DEFAULT 'CONTRIBUTOR' CHECK(role IN (${sqlEnum(PLAN_MEMBER_ROLES)})),
      joined_at  TEXT NOT NULL,
      PRIMARY KEY(plan_id, agent_id)
    );

    CREATE TABLE IF NOT EXISTS plan_docs (
      plan_id       TEXT NOT NULL REFERENCES awareness_plans(plan_id) ON DELETE CASCADE,
      relative_path TEXT NOT NULL,
      title         TEXT NOT NULL,
      kind          TEXT NOT NULL DEFAULT 'SUPPORTING' CHECK(kind IN (${sqlEnum(PLAN_DOC_KINDS)})),
      ordinal       INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY(plan_id, relative_path)
    );

    CREATE TABLE IF NOT EXISTS awareness_tasks (
      task_id      TEXT PRIMARY KEY,
      plan_id      TEXT NOT NULL REFERENCES awareness_plans(plan_id) ON DELETE CASCADE,
      title        TEXT NOT NULL,
      reasoning    TEXT NOT NULL,
      acceptance_criteria TEXT NOT NULL,
      source_step_key TEXT,
      check_command TEXT,
      status       TEXT NOT NULL DEFAULT 'OPEN'
                   CHECK(status IN (${sqlEnum(TASK_STATUSES)})),
      priority     INTEGER NOT NULL DEFAULT 0,
      created_by   TEXT NOT NULL,
      created_at   TEXT NOT NULL,
      updated_at   TEXT NOT NULL,
      completed_at TEXT
    );

    CREATE TABLE IF NOT EXISTS task_paths (
      task_id TEXT NOT NULL REFERENCES awareness_tasks(task_id) ON DELETE CASCADE,
      path    TEXT NOT NULL,
      ordinal INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY(task_id, path)
    );

    CREATE TABLE IF NOT EXISTS task_dependencies (
      task_id            TEXT NOT NULL REFERENCES awareness_tasks(task_id) ON DELETE CASCADE,
      depends_on_task_id TEXT NOT NULL REFERENCES awareness_tasks(task_id) ON DELETE CASCADE,
      created_by         TEXT NOT NULL,
      created_at         TEXT NOT NULL,
      PRIMARY KEY(task_id, depends_on_task_id),
      CHECK(task_id <> depends_on_task_id)
    );

    CREATE TABLE IF NOT EXISTS task_runs (
      run_id         TEXT PRIMARY KEY,
      task_id        TEXT REFERENCES awareness_tasks(task_id) ON DELETE SET NULL,
      origin         TEXT NOT NULL DEFAULT 'TASK' CHECK(origin IN (${sqlEnum(TASK_RUN_ORIGINS)})),
      agent_id       TEXT NOT NULL,
      session_id     TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
      rationale      TEXT NOT NULL,
      test_plan      TEXT NOT NULL,
      context_ref    TEXT,
      status         TEXT NOT NULL DEFAULT 'ACTIVE'
                     CHECK(status IN (${sqlEnum(TASK_RUN_STATUSES)})),
      workspace_path TEXT,
      artifact       TEXT,
      created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
      updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE TABLE IF NOT EXISTS run_files (
      run_id         TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
      file_path      TEXT NOT NULL,
      reason_override TEXT,
      source         TEXT NOT NULL CHECK(source IN ('EXPLICIT','HOOK')),
      started_at     TEXT NOT NULL,
      heartbeat_at   TEXT NOT NULL,
      expires_at     TEXT NOT NULL,
      ended_at       TEXT,
      PRIMARY KEY(run_id, file_path)
    );

    CREATE TABLE IF NOT EXISTS task_claims (
      task_id      TEXT PRIMARY KEY REFERENCES awareness_tasks(task_id) ON DELETE CASCADE,
      run_id       TEXT NOT NULL UNIQUE REFERENCES task_runs(run_id) ON DELETE CASCADE,
      agent_id     TEXT NOT NULL,
      claimed_at   TEXT NOT NULL,
      heartbeat_at TEXT NOT NULL,
      expires_at   TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS awareness_locks (
      lock_id     TEXT PRIMARY KEY,
      file_path   TEXT NOT NULL,
      run_id      TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
      acquired_at TEXT NOT NULL,
      expires_at  TEXT,
      UNIQUE(file_path, run_id)
    );

    CREATE TABLE IF NOT EXISTS delivery_state (
      consumer_id TEXT NOT NULL,
      channel     TEXT NOT NULL,
      scope_key   TEXT NOT NULL,
      fingerprint TEXT NOT NULL,
      delivered_at TEXT NOT NULL,
      PRIMARY KEY(consumer_id, channel, scope_key)
    );

    ${HOOK_RECEIPTS_DDL}

    CREATE TABLE IF NOT EXISTS signals (
      signal_id      TEXT PRIMARY KEY,
      workspace_path TEXT NOT NULL,
      artifact       TEXT,
      repo           TEXT,
      ref            TEXT,
      from_agent     TEXT NOT NULL,
      to_agent       TEXT,
      kind           TEXT NOT NULL,
      subject        TEXT NOT NULL,
      body           TEXT,
      files_json     TEXT NOT NULL DEFAULT '[]',
      refs_json      TEXT NOT NULL DEFAULT '[]',
      thread_id      TEXT NOT NULL,
      reply_to       TEXT,
      importance     INTEGER NOT NULL DEFAULT 5,
      status         TEXT NOT NULL DEFAULT 'open'
                     CHECK(status IN ('open','resolved')),
      resolved_at    TEXT,
      created_at     TEXT NOT NULL,
      expires_at     TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS signal_reads (
      signal_id TEXT NOT NULL,
      agent_id  TEXT NOT NULL,
      read_at   TEXT NOT NULL,
      PRIMARY KEY (signal_id, agent_id),
      FOREIGN KEY(signal_id) REFERENCES signals(signal_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS memory_refs (
      memory_id TEXT    NOT NULL,
      reference TEXT    NOT NULL,
      kind      TEXT,
      ordinal   INTEGER NOT NULL DEFAULT 0,
      PRIMARY KEY (memory_id, reference),
      FOREIGN KEY(memory_id) REFERENCES awareness_memories(memory_id) ON DELETE CASCADE
    );

    -- ARCH-5: Agent identity registry — maps opaque agentIds to human-readable names.
    -- Separate from awareness_memories so the mapping persists even when memories are pruned.
    -- ON CONFLICT logic in agents.ts ensures a non-empty name is never overwritten by ''.
    CREATE TABLE IF NOT EXISTS awareness_agents (
      agent_id       TEXT NOT NULL,
      agent_name     TEXT NOT NULL DEFAULT '',
      workspace_path TEXT NOT NULL DEFAULT '',
      artifact       TEXT,
      context        TEXT,
      role           TEXT,
      status         TEXT NOT NULL DEFAULT 'ACTIVE' CHECK(status IN (${sqlEnum(AGENT_STATUSES)})),
      metadata_json  TEXT NOT NULL DEFAULT '{}',
      registered_at  TEXT NOT NULL,
      last_seen_at   TEXT NOT NULL,
      PRIMARY KEY(workspace_path, agent_id)
    );

    ${LOCAL_HISTORY_SCHEMA_DDL}
`;

export const SCHEMA_INDEX_DDL = `
  CREATE INDEX IF NOT EXISTS idx_event_outbox_workspace_sequence ON event_outbox(workspace_path, sequence);
  CREATE INDEX IF NOT EXISTS idx_event_outbox_aggregate ON event_outbox(workspace_path, aggregate_kind, aggregate_id, sequence);
  ${EVENT_OUTBOX_TYPED_INDEX_DDL}
  CREATE INDEX IF NOT EXISTS idx_interactions_session_status ON pending_interactions(workspace_path, session_id, status, created_at);
  CREATE INDEX IF NOT EXISTS idx_authorization_plan_revision ON authorization_receipts(workspace_path, plan_id, revision, consumed_at);
  CREATE UNIQUE INDEX IF NOT EXISTS idx_awareness_plans_source ON awareness_plans(workspace_path, source_kind, source_key) WHERE source_kind IS NOT NULL AND source_key IS NOT NULL;
  CREATE UNIQUE INDEX IF NOT EXISTS idx_awareness_tasks_source_step ON awareness_tasks(plan_id, source_step_key) WHERE source_step_key IS NOT NULL;
  CREATE INDEX IF NOT EXISTS idx_sessions_agent     ON sessions(agent_id);
  CREATE INDEX IF NOT EXISTS idx_sessions_workspace ON sessions(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_sessions_scope     ON sessions(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_importance      ON awareness_memories(importance);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_created_at      ON awareness_memories(created_at);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_state           ON awareness_memories(state);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_label           ON awareness_memories(label);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_failure_sig     ON awareness_memories(failure_signature);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_workspace_path  ON awareness_memories(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_scope           ON awareness_memories(workspace_path, repo, ref);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_artifact_scope  ON awareness_memories(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_repo_ref        ON awareness_memories(repo, ref);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_valid           ON awareness_memories(valid_from, valid_to);
  CREATE INDEX IF NOT EXISTS idx_awareness_memories_embedding_model ON awareness_memories(embedding_model);
  CREATE INDEX IF NOT EXISTS idx_awareness_plans_scope          ON awareness_plans(workspace_path, artifact, status);
  CREATE INDEX IF NOT EXISTS idx_awareness_plans_lead           ON awareness_plans(lead_agent_id, status);
  CREATE INDEX IF NOT EXISTS idx_plan_members_agent   ON plan_members(agent_id, plan_id);
  CREATE INDEX IF NOT EXISTS idx_awareness_tasks_plan_status    ON awareness_tasks(plan_id, status, priority DESC, created_at);
  CREATE INDEX IF NOT EXISTS idx_task_deps_dependency ON task_dependencies(depends_on_task_id);
  CREATE INDEX IF NOT EXISTS idx_task_claims_agent    ON task_claims(agent_id, expires_at);
  CREATE INDEX IF NOT EXISTS idx_task_claims_expiry   ON task_claims(expires_at);
  CREATE INDEX IF NOT EXISTS idx_task_runs_status     ON task_runs(status);
  CREATE INDEX IF NOT EXISTS idx_task_runs_agent      ON task_runs(agent_id, status);
  CREATE INDEX IF NOT EXISTS idx_task_runs_task       ON task_runs(task_id, created_at DESC);
  CREATE INDEX IF NOT EXISTS idx_task_runs_scope      ON task_runs(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_run_files_path_active ON run_files(file_path, ended_at, expires_at);
  CREATE INDEX IF NOT EXISTS idx_run_files_heartbeat   ON run_files(heartbeat_at);
  CREATE INDEX IF NOT EXISTS idx_awareness_locks_file_path   ON awareness_locks(file_path);
  CREATE INDEX IF NOT EXISTS idx_awareness_locks_acquired_at ON awareness_locks(acquired_at);
  CREATE INDEX IF NOT EXISTS idx_awareness_locks_expires_at  ON awareness_locks(expires_at);
  CREATE INDEX IF NOT EXISTS idx_delivery_state_delivered ON delivery_state(delivered_at);
  CREATE INDEX IF NOT EXISTS idx_signals_status         ON signals(status);
  CREATE INDEX IF NOT EXISTS idx_signals_to_agent       ON signals(to_agent);
  CREATE INDEX IF NOT EXISTS idx_signals_workspace_path ON signals(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_signals_scope          ON signals(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_signals_created_at     ON signals(created_at);
  CREATE INDEX IF NOT EXISTS idx_signals_expires_at     ON signals(expires_at);
  CREATE INDEX IF NOT EXISTS idx_signals_thread         ON signals(thread_id);
  CREATE INDEX IF NOT EXISTS idx_memory_refs_ref  ON memory_refs(reference);
  CREATE INDEX IF NOT EXISTS idx_memory_refs_kind ON memory_refs(kind);
  CREATE INDEX IF NOT EXISTS idx_awareness_agents_workspace ON awareness_agents(workspace_path);
  CREATE INDEX IF NOT EXISTS idx_awareness_agents_scope     ON awareness_agents(workspace_path, artifact);
  CREATE INDEX IF NOT EXISTS idx_awareness_agents_last_seen ON awareness_agents(last_seen_at DESC);
  ${LOCAL_HISTORY_INDEX_DDL}
`;
/** In-place upgrade: convert signals.expires_at from nullable TEXT to TEXT NOT NULL.
 * Any NULL rows are backfilled with the current UTC time before enforcing the constraint.
 */
export const SIGNALS_EXPIRES_NOT_NULL_UPGRADE_DDL = `
  UPDATE signals SET expires_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE expires_at IS NULL;
  CREATE TABLE signals_upgrade_v5 (
    signal_id      TEXT PRIMARY KEY,
    workspace_path TEXT NOT NULL,
    artifact       TEXT,
    repo           TEXT,
    ref            TEXT,
    from_agent     TEXT NOT NULL,
    to_agent       TEXT,
    kind           TEXT NOT NULL,
    subject        TEXT NOT NULL,
    body           TEXT,
    files_json     TEXT NOT NULL DEFAULT '[]',
    refs_json      TEXT NOT NULL DEFAULT '[]',
    thread_id      TEXT NOT NULL,
    reply_to       TEXT,
    importance     INTEGER NOT NULL DEFAULT 5,
    status         TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','resolved')),
    resolved_at    TEXT,
    created_at     TEXT NOT NULL,
    expires_at     TEXT NOT NULL
  );
  INSERT INTO signals_upgrade_v5 SELECT * FROM signals;
  DROP TABLE signals;
  ALTER TABLE signals_upgrade_v5 RENAME TO signals;
  CREATE INDEX idx_signals_status         ON signals(status);
  CREATE INDEX idx_signals_to_agent       ON signals(to_agent);
  CREATE INDEX idx_signals_workspace_path ON signals(workspace_path);
  CREATE INDEX idx_signals_scope          ON signals(workspace_path, artifact);
  CREATE INDEX idx_signals_created_at     ON signals(created_at);
  CREATE INDEX idx_signals_expires_at     ON signals(expires_at);
  CREATE INDEX idx_signals_thread         ON signals(thread_id);
`;

export const FTS_SCHEMA_DDL = `
  CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts
  USING fts5(memory_id UNINDEXED, task_context, observation, tags)
`;
