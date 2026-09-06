// src/sql/audit.ts — SQL constants for edit_log and harness_log tables

// ─── edit_log ─────────────────────────────────────────────────────────────────

export const EDIT_LOG_INSERT = `
  INSERT INTO edit_log (
    edit_id, session_id, run_id, agent_id,
    file_path, operation, old_file_path,
    lines_added, lines_removed, content_hash,
    workspace_path, artifact, created_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
`;

// ─── harness_log ──────────────────────────────────────────────────────────────

export const HARNESS_LOG_INSERT = `
  INSERT INTO harness_log (
    harness_id, session_id, agent_id, workspace_path, artifact, event_type,
    payload_json, memory_id, run_id, created_at
  ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
`;
