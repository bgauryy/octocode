/** Exact pre-convergence log relations. Migration evidence only; never run during ordinary init. */
export const PREDECESSOR_EVENT_RELATIONS_DDL = `
  CREATE TABLE task_events (
    event_id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES awareness_tasks(task_id) ON DELETE CASCADE,
    run_id TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
    agent_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK(event_type IN ('CREATED','DEPENDENCY_ADDED','CLAIMED','SUBMITTED','BLOCKED','RELEASED','CLAIM_EXPIRED','VERIFIED','VERIFICATION_FAILED')),
    message TEXT NOT NULL,
    created_at TEXT NOT NULL
  );
  CREATE TABLE run_log (
    event_id TEXT PRIMARY KEY,
    run_id TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
    agent_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL
  );
  CREATE TABLE edit_log (
    edit_id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
    run_id TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
    agent_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('create','update','delete','move','rename')),
    old_file_path TEXT,
    lines_added INTEGER,
    lines_removed INTEGER,
    content_hash TEXT,
    workspace_path TEXT,
    artifact TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
  );
  CREATE TABLE harness_log (
    harness_id TEXT PRIMARY KEY,
    session_id TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
    agent_id TEXT NOT NULL,
    workspace_path TEXT,
    artifact TEXT,
    event_type TEXT NOT NULL CHECK(event_type IN ('mine','propose','validate','apply','capture','reflect')),
    payload_json TEXT,
    memory_id TEXT REFERENCES awareness_memories(memory_id) ON DELETE SET NULL,
    run_id TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
  );
  CREATE TABLE handoffs (
    handoff_id TEXT PRIMARY KEY,
    workspace_path TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    summary TEXT NOT NULL,
    files_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    cleared_at TEXT
  );
  CREATE INDEX idx_handoffs_open ON handoffs(workspace_path, cleared_at, created_at);
  CREATE INDEX idx_task_events_task ON task_events(task_id, created_at);
  CREATE INDEX idx_edit_log_session ON edit_log(session_id);
  CREATE INDEX idx_edit_log_run ON edit_log(run_id);
  CREATE INDEX idx_edit_log_agent ON edit_log(agent_id);
  CREATE INDEX idx_edit_log_file ON edit_log(file_path);
  CREATE INDEX idx_edit_log_workspace ON edit_log(workspace_path);
  CREATE INDEX idx_edit_log_scope ON edit_log(workspace_path, artifact);
  CREATE INDEX idx_edit_log_created_at ON edit_log(created_at);
  CREATE INDEX idx_harness_log_session ON harness_log(session_id);
  CREATE INDEX idx_harness_log_agent ON harness_log(agent_id);
  CREATE INDEX idx_harness_log_scope ON harness_log(workspace_path, artifact);
  CREATE INDEX idx_harness_log_event_type ON harness_log(event_type);
  CREATE INDEX idx_harness_log_memory ON harness_log(memory_id);
  CREATE INDEX idx_harness_log_run ON harness_log(run_id);
`;

export const PREDECESSOR_EVENT_RELATIONS = Object.freeze([
  'task_events', 'run_log', 'edit_log', 'harness_log', 'handoffs',
] as const);

/** Exact table removed by the one-surface cutover; migration input only. */
export const PREDECESSOR_REFINEMENTS_DDL = `
  CREATE TABLE refinements (
    refinement_id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    workspace_path TEXT NOT NULL,
    artifact TEXT,
    repo TEXT,
    ref TEXT,
    files_json TEXT NOT NULL DEFAULT '[]',
    reasoning TEXT NOT NULL,
    remember TEXT NOT NULL,
    quality TEXT NOT NULL CHECK(quality IN ('good','bad','handoff','instructions')) DEFAULT 'good',
    state TEXT NOT NULL CHECK(state IN ('open','ongoing','done')) DEFAULT 'open',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
  );
  CREATE INDEX idx_refinements_state ON refinements(state);
  CREATE INDEX idx_refinements_scope ON refinements(workspace_path, artifact);
  CREATE INDEX idx_refinements_repo ON refinements(repo);
  CREATE INDEX idx_refinements_state_updated ON refinements(state, updated_at DESC);
`;

/**
 * Frozen schema emitted by the repository-scoped Awareness store before
 * canonical relation names and the typed event stream. This is executable
 * migration evidence only. Runtime initialization must never execute it.
 */
export const LEGACY_RENAMED_V1_SCHEMA_DDL = `
  CREATE TABLE sessions (
    session_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, workspace_path TEXT,
    artifact TEXT, repo TEXT, ref TEXT, started_at TEXT NOT NULL, ended_at TEXT, summary TEXT
  );
  CREATE TABLE memories (
    memory_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, task_context TEXT NOT NULL,
    observation TEXT NOT NULL, importance INTEGER NOT NULL CHECK(importance BETWEEN 1 AND 10),
    state TEXT NOT NULL DEFAULT 'ACTIVE' CHECK(state IN ('ACTIVE', 'SUPERSEDED')),
    label TEXT NOT NULL DEFAULT 'OTHER', superseded_by TEXT, tags_json TEXT NOT NULL DEFAULT '[]',
    workspace_path TEXT, artifact TEXT, repo TEXT, ref TEXT, file_tree_fingerprint TEXT,
    novelty_score REAL, last_accessed_at TEXT, access_count INTEGER NOT NULL DEFAULT 0,
    decay_half_life_days REAL, failure_signature TEXT, valid_from TEXT, valid_to TEXT,
    expired_at TEXT, embedding BLOB, embedding_model TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')), updated_at TEXT
  );
  CREATE TABLE plans (
    plan_id TEXT PRIMARY KEY, name TEXT NOT NULL, objective TEXT NOT NULL,
    lead_agent_id TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'DRAFT'
      CHECK(status IN ('DRAFT','ACTIVE','PAUSED','COMPLETED','CANCELLED')),
    workspace_path TEXT NOT NULL, artifact TEXT, doc_dir TEXT NOT NULL,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL
  );
  CREATE TABLE plan_members (
    plan_id TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL, role TEXT NOT NULL DEFAULT 'CONTRIBUTOR'
      CHECK(role IN ('LEAD','CONTRIBUTOR')), joined_at TEXT NOT NULL,
    PRIMARY KEY(plan_id, agent_id)
  );
  CREATE TABLE plan_docs (
    plan_id TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
    relative_path TEXT NOT NULL, title TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'SUPPORTING' CHECK(kind IN ('PRIMARY','SUPPORTING')),
    ordinal INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(plan_id, relative_path)
  );
  CREATE TABLE tasks (
    task_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL REFERENCES plans(plan_id) ON DELETE CASCADE,
    title TEXT NOT NULL, reasoning TEXT NOT NULL, acceptance_criteria TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'OPEN'
      CHECK(status IN ('OPEN','IN_PROGRESS','BLOCKED','VERIFY','DONE','FAILED','CANCELLED')),
    priority INTEGER NOT NULL DEFAULT 0, created_by TEXT NOT NULL,
    created_at TEXT NOT NULL, updated_at TEXT NOT NULL, completed_at TEXT
  );
  CREATE TABLE task_paths (
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    path TEXT NOT NULL, ordinal INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(task_id, path)
  );
  CREATE TABLE task_dependencies (
    task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    depends_on_task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    created_by TEXT NOT NULL, created_at TEXT NOT NULL,
    PRIMARY KEY(task_id, depends_on_task_id), CHECK(task_id <> depends_on_task_id)
  );
  CREATE TABLE task_runs (
    run_id TEXT PRIMARY KEY, task_id TEXT REFERENCES tasks(task_id) ON DELETE SET NULL,
    origin TEXT NOT NULL DEFAULT 'TASK' CHECK(origin IN ('TASK','WORK','HOOK')),
    agent_id TEXT NOT NULL, session_id TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
    rationale TEXT NOT NULL, test_plan TEXT NOT NULL, context_ref TEXT,
    status TEXT NOT NULL DEFAULT 'ACTIVE' CHECK(status IN ('PENDING','ACTIVE','SUCCESS','FAILED')),
    workspace_path TEXT, artifact TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
  );
  CREATE TABLE run_files (
    run_id TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
    file_path TEXT NOT NULL, reason_override TEXT,
    source TEXT NOT NULL CHECK(source IN ('EXPLICIT','HOOK')),
    started_at TEXT NOT NULL, heartbeat_at TEXT NOT NULL, expires_at TEXT NOT NULL,
    ended_at TEXT, PRIMARY KEY(run_id, file_path)
  );
  CREATE TABLE task_claims (
    task_id TEXT PRIMARY KEY REFERENCES tasks(task_id) ON DELETE CASCADE,
    run_id TEXT NOT NULL UNIQUE REFERENCES task_runs(run_id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL, claimed_at TEXT NOT NULL, heartbeat_at TEXT NOT NULL, expires_at TEXT NOT NULL
  );
  CREATE TABLE task_events (
    event_id TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    run_id TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL, agent_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK(event_type IN
      ('CREATED','DEPENDENCY_ADDED','CLAIMED','SUBMITTED','BLOCKED','RELEASED','CLAIM_EXPIRED','VERIFIED','VERIFICATION_FAILED')),
    message TEXT NOT NULL, created_at TEXT NOT NULL
  );
  CREATE TABLE locks (
    lock_id TEXT PRIMARY KEY, file_path TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES task_runs(run_id) ON DELETE CASCADE,
    acquired_at TEXT NOT NULL, expires_at TEXT, UNIQUE(file_path, run_id)
  );
  CREATE TABLE delivery_state (
    consumer_id TEXT NOT NULL, channel TEXT NOT NULL, scope_key TEXT NOT NULL,
    fingerprint TEXT NOT NULL, delivered_at TEXT NOT NULL,
    PRIMARY KEY(consumer_id, channel, scope_key)
  );
  CREATE TABLE hook_receipts (
    workspace_path TEXT NOT NULL, host TEXT NOT NULL CHECK(host IN ('claude','codex','cursor')),
    event TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('success','failure')),
    last_seen_at TEXT NOT NULL, PRIMARY KEY(workspace_path, host, event)
  );
  CREATE TABLE run_log (
    event_id TEXT PRIMARY KEY, run_id TEXT, agent_id TEXT NOT NULL,
    event_type TEXT NOT NULL, message TEXT NOT NULL, created_at TEXT NOT NULL,
    FOREIGN KEY(run_id) REFERENCES task_runs(run_id) ON DELETE SET NULL
  );
  ${PREDECESSOR_REFINEMENTS_DDL}
  CREATE TABLE signals (
    signal_id TEXT PRIMARY KEY, workspace_path TEXT NOT NULL, artifact TEXT, repo TEXT, ref TEXT,
    from_agent TEXT NOT NULL, to_agent TEXT, kind TEXT NOT NULL, subject TEXT NOT NULL, body TEXT,
    files_json TEXT NOT NULL DEFAULT '[]', refs_json TEXT NOT NULL DEFAULT '[]',
    thread_id TEXT NOT NULL, reply_to TEXT, importance INTEGER NOT NULL DEFAULT 5,
    status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','resolved')),
    resolved_at TEXT, created_at TEXT NOT NULL
  );
  CREATE TABLE signal_reads (
    signal_id TEXT NOT NULL, agent_id TEXT NOT NULL, read_at TEXT NOT NULL,
    PRIMARY KEY(signal_id, agent_id),
    FOREIGN KEY(signal_id) REFERENCES signals(signal_id) ON DELETE CASCADE
  );
  CREATE TABLE memory_refs (
    memory_id TEXT NOT NULL, reference TEXT NOT NULL, kind TEXT,
    ordinal INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(memory_id, reference),
    FOREIGN KEY(memory_id) REFERENCES memories(memory_id) ON DELETE CASCADE
  );
  CREATE TABLE agents (
    agent_id TEXT PRIMARY KEY, agent_name TEXT NOT NULL DEFAULT '', workspace_path TEXT,
    artifact TEXT, context TEXT, registered_at TEXT NOT NULL, last_seen_at TEXT NOT NULL
  );
  CREATE TABLE edit_log (
    edit_id TEXT PRIMARY KEY, session_id TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
    run_id TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL, agent_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('create','update','delete','move','rename')),
    old_file_path TEXT, lines_added INTEGER, lines_removed INTEGER, content_hash TEXT,
    workspace_path TEXT, artifact TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
  );
  CREATE TABLE harness_log (
    harness_id TEXT PRIMARY KEY, session_id TEXT REFERENCES sessions(session_id) ON DELETE SET NULL,
    agent_id TEXT NOT NULL, workspace_path TEXT, artifact TEXT,
    event_type TEXT NOT NULL CHECK(event_type IN ('mine','propose','validate','apply','capture','reflect')),
    payload_json TEXT, memory_id TEXT REFERENCES memories(memory_id) ON DELETE SET NULL,
    run_id TEXT REFERENCES task_runs(run_id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
  );

  CREATE INDEX idx_sessions_agent ON sessions(agent_id);
  CREATE INDEX idx_sessions_workspace ON sessions(workspace_path);
  CREATE INDEX idx_sessions_scope ON sessions(workspace_path, artifact);
  CREATE INDEX idx_memories_importance ON memories(importance);
  CREATE INDEX idx_memories_created_at ON memories(created_at);
  CREATE INDEX idx_memories_state ON memories(state);
  CREATE INDEX idx_memories_label ON memories(label);
  CREATE INDEX idx_memories_failure_sig ON memories(failure_signature);
  CREATE INDEX idx_memories_workspace_path ON memories(workspace_path);
  CREATE INDEX idx_memories_scope ON memories(workspace_path, repo, ref);
  CREATE INDEX idx_memories_artifact_scope ON memories(workspace_path, artifact);
  CREATE INDEX idx_memories_repo_ref ON memories(repo, ref);
  CREATE INDEX idx_memories_valid ON memories(valid_from, valid_to);
  CREATE INDEX idx_memories_embedding_model ON memories(embedding_model);
  CREATE INDEX idx_plans_scope ON plans(workspace_path, artifact, status);
  CREATE INDEX idx_plans_lead ON plans(lead_agent_id, status);
  CREATE INDEX idx_plan_members_agent ON plan_members(agent_id, plan_id);
  CREATE INDEX idx_tasks_plan_status ON tasks(plan_id, status, priority DESC, created_at);
  CREATE INDEX idx_task_deps_dependency ON task_dependencies(depends_on_task_id);
  CREATE INDEX idx_task_claims_agent ON task_claims(agent_id, expires_at);
  CREATE INDEX idx_task_claims_expiry ON task_claims(expires_at);
  CREATE INDEX idx_task_runs_status ON task_runs(status);
  CREATE INDEX idx_task_runs_agent ON task_runs(agent_id, status);
  CREATE INDEX idx_task_runs_task ON task_runs(task_id, created_at DESC);
  CREATE INDEX idx_task_runs_scope ON task_runs(workspace_path, artifact);
  CREATE INDEX idx_task_events_task ON task_events(task_id, created_at);
  CREATE INDEX idx_run_files_path_active ON run_files(file_path, ended_at, expires_at);
  CREATE INDEX idx_run_files_heartbeat ON run_files(heartbeat_at);
  CREATE INDEX idx_locks_file_path ON locks(file_path);
  CREATE INDEX idx_locks_acquired_at ON locks(acquired_at);
  CREATE INDEX idx_locks_expires_at ON locks(expires_at);
  CREATE INDEX idx_delivery_state_delivered ON delivery_state(delivered_at);
  CREATE INDEX idx_signals_status ON signals(status);
  CREATE INDEX idx_signals_to_agent ON signals(to_agent);
  CREATE INDEX idx_signals_workspace_path ON signals(workspace_path);
  CREATE INDEX idx_signals_scope ON signals(workspace_path, artifact);
  CREATE INDEX idx_signals_created_at ON signals(created_at);
  CREATE INDEX idx_signals_thread ON signals(thread_id);
  CREATE INDEX idx_memory_refs_ref ON memory_refs(reference);
  CREATE INDEX idx_memory_refs_kind ON memory_refs(kind);
  CREATE INDEX idx_agents_workspace ON agents(workspace_path);
  CREATE INDEX idx_agents_scope ON agents(workspace_path, artifact);
  CREATE INDEX idx_agents_last_seen ON agents(last_seen_at DESC);
  CREATE INDEX idx_edit_log_session ON edit_log(session_id);
  CREATE INDEX idx_edit_log_run ON edit_log(run_id);
  CREATE INDEX idx_edit_log_agent ON edit_log(agent_id);
  CREATE INDEX idx_edit_log_file ON edit_log(file_path);
  CREATE INDEX idx_edit_log_workspace ON edit_log(workspace_path);
  CREATE INDEX idx_edit_log_scope ON edit_log(workspace_path, artifact);
  CREATE INDEX idx_edit_log_created_at ON edit_log(created_at);
  CREATE INDEX idx_harness_log_session ON harness_log(session_id);
  CREATE INDEX idx_harness_log_agent ON harness_log(agent_id);
  CREATE INDEX idx_harness_log_scope ON harness_log(workspace_path, artifact);
  CREATE INDEX idx_harness_log_event_type ON harness_log(event_type);
  CREATE INDEX idx_harness_log_memory ON harness_log(memory_id);
  CREATE INDEX idx_harness_log_run ON harness_log(run_id);
`;
