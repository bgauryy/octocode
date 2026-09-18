// ─── Edit log ─────────────────────────────────────────────────────────────────

export type EditOperation = 'create' | 'update' | 'delete' | 'move' | 'rename';

export interface EditLogRow {
  edit_id: string;
  session_id: string | null;
  run_id: string | null;
  agent_id: string;
  file_path: string;
  operation: EditOperation;
  old_file_path: string | null;
  lines_added: number | null;
  lines_removed: number | null;
  content_hash: string | null;
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
}

export interface InsertEditLogParams {
  agentId: string;
  sessionId?: string | null;
  runId?: string | null;
  filePath: string;
  operation: EditOperation;
  oldFilePath?: string | null;
  linesAdded?: number | null;
  linesRemoved?: number | null;
  contentHash?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
}

export interface QueryEditLogParams {
  sessionId?: string;
  runId?: string;
  agentId?: string;
  filePath?: string;
  workspacePath?: string;
  artifact?: string | null;
  operation?: EditOperation;
  since?: string;    // ISO timestamp
  limit?: number;
}

// ─── Harness log ──────────────────────────────────────────────────────────────

export type HarnessEventType = 'mine' | 'propose' | 'validate' | 'apply' | 'capture' | 'reflect';

export interface HarnessLogRow {
  harness_id: string;
  session_id: string | null;
  agent_id: string;
  workspace_path: string | null;
  artifact: string | null;
  event_type: HarnessEventType;
  payload_json: string | null;
  memory_id: string | null;
  run_id: string | null;
  created_at: string;
}

export interface InsertHarnessLogParams {
  agentId: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  eventType: HarnessEventType;
  payload?: Record<string, unknown>;
  memoryId?: string | null;
  runId?: string | null;
}

// ─── Session row / end session ────────────────────────────────────────────────

/** Raw DB row for the sessions table — mirrors the public Session shape. */
export interface SessionRow {
  session_id: string;
  agent_id: string;
  workspace_path: string | null;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  started_at: string;
  ended_at: string | null;
  summary: string | null;
}

export interface EndSessionParams {
  sessionId: string;
  agentId: string;
  workspacePath?: string | null;
  artifact?: string | null;
  summary?: string | null;
}
