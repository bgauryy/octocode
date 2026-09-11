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
