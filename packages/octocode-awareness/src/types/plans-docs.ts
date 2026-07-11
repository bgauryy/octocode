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

// ─── Doc staleness ─────────────────────────────────────────────────────────────

/** One doc-to-source mapping to check for drift, e.g. a package's ARCHITECTURE.md vs its src/. */
export interface DocStalenessTarget {
  /** Path as recorded in edit_log (repo-relative or absolute — must match insertEditLog's filePath convention). */
  docFile: string;
  /** Path prefixes considered "source of truth" for this doc. */
  sourceDirs: string[];
}

export interface DocStalenessParams {
  targets: DocStalenessTarget[];
  workspacePath?: string | null;
  artifact?: string | null;
  /** Edits to sourceDirs since the doc's last recorded edit at/above this count flag it stale. Default 5. */
  minEditsSinceSync?: number;
  /** Cumulative lines added+removed since the doc's last recorded edit at/above this flag it stale. Default 50. */
  minLinesSinceSync?: number;
  cwd?: string;
}

export interface DocStalenessEntry {
  doc_file: string;
  source_dirs: string[];
  /** Most recent edit_log timestamp for doc_file itself, or null if never tracked. */
  doc_last_synced_at: string | null;
  edits_since_sync: number;
  lines_changed_since_sync: number;
  files_touched: string[];
  latest_source_edit_at: string | null;
  stale: boolean;
}

export interface DocStalenessResult {
  ok: true;
  checked: number;
  stale_count: number;
  entries: DocStalenessEntry[];
}

export interface ProposeDocRefreshParams {
  agentId: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
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
