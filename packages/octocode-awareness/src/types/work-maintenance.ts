import type { RunOrigin, RunStatus, WorkSource } from './identity-memory.js';

// ─── Internal raw DB row shapes ───────────────────────────────────────────────

export interface MemoryRow {
  memory_id: string;
  agent_id: string;
  task_context: string;
  observation: string;
  importance: number;
  state: string;
  label: string;
  superseded_by: string | null;
  tags_json: string;
  workspace_path: string | null;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  file_tree_fingerprint: string | null;
  novelty_score: number | null;
  last_accessed_at: string | null;
  access_count: number;
  decay_half_life_days: number | null;
  failure_signature: string | null;
  valid_from: string | null;
  valid_to: string | null;
  expired_at: string | null;
  created_at: string;
  updated_at: string | null;
  _bm25?: number;
}

// ─── Advisory file work ──────────────────────────────────────────────────────

export interface WorkRunRecord {
  run_id: string;
  task_id: string | null;
  origin: RunOrigin;
  agent_id: string;
  session_id: string | null;
  rationale: string;
  test_plan: string;
  context_ref: string | null;
  status: RunStatus;
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
  updated_at: string;
}

export interface WorkFileRecord {
  run_id: string;
  file_path: string;
  reason_override: string | null;
  source: WorkSource;
  started_at: string;
  heartbeat_at: string;
  expires_at: string;
  ended_at: string | null;
}

export interface WorkPresence extends WorkFileRecord {
  task_id: string | null;
  origin: RunOrigin;
  agent_id: string;
  session_id: string | null;
  rationale: string;
  test_plan: string;
  status: RunStatus;
  workspace_path: string | null;
  artifact: string | null;
  exclusive: boolean;
}

export interface WorkPeer {
  run_id: string;
  task_id: string | null;
  origin: RunOrigin;
  agent_id: string;
  file_path: string;
  rationale: string;
  heartbeat_at: string;
  expires_at: string;
  exclusive: boolean;
}

export interface WorkConflict extends WorkPeer {
  conflict_type: 'ACTIVE_WORK' | 'EXCLUSIVE_LOCK';
}

export interface StartWorkParams {
  agentId: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  runId?: string | null;
  targetFiles: string[];
  rationale?: string;
  testPlan?: string;
  contextRef?: string | null;
  origin?: Exclude<RunOrigin, 'TASK'>;
  source?: WorkSource;
  ttlMs?: number | null;
  exclusive?: boolean;
  reasonOverride?: string | null;
}

export type StartWorkResult =
  | { ok: true; run: WorkRunRecord; files: WorkFileRecord[]; peers: WorkPeer[]; peer_count: number }
  | { ok: false; conflict: true; conflicts: WorkConflict[] };

export interface TouchWorkParams {
  runId: string;
  agentId: string;
  targetFiles?: string[];
  ttlMs?: number | null;
}

export interface WorkMutationResult {
  run: WorkRunRecord;
  files: WorkFileRecord[];
  peers: WorkPeer[];
  peer_count: number;
}

export interface EndWorkParams {
  runId: string;
  agentId: string;
  targetFiles?: string[];
}

export interface ListWorkParams {
  workspacePath?: string | null;
  artifact?: string | null;
  agentId?: string | null;
  runId?: string | null;
  filePath?: string | null;
  activeOnly?: boolean;
  limit?: number | null;
  offset?: number;
}

export interface ListWorkResult {
  count: number;
  total_count: number;
  omitted_count: number;
  files: WorkPresence[];
  partial: boolean;
  partialReasons: Array<'limit'>;
  next?: { list: { method: 'listWork'; params: ListWorkParams } };
}

export interface TableInfoRow {
  name: string;
}

// ─── Forget ──────────────────────────────────────────────────────────────────

export interface ForgetMemoryParams {
  agentId?: string;
  memoryIds?: string[];
  tags?: string[];
  before?: string;               // ISO — delete memories created before this
  maxImportance?: number;        // safety ceiling — only delete at or below this score
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  dryRun?: boolean;
  cwd?: string;
}

export interface ForgetMemoryResult {
  deleted: number;
  dry_run?: true;
  would_delete?: number;
  memory_ids: string[];
  /** Present when a broad selector was capped at the default importance ceiling. */
  salience_floor?: number;
}

// ─── Reversible memory archive ───────────────────────────────────────────────

export interface MemoryLifecycleParams {
  memoryIds: string[];
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  dryRun?: boolean;
  cwd?: string;
}

export interface ArchiveMemoryResult {
  archived: number;
  dry_run?: true;
  would_archive?: number;
  memory_ids: string[];
}

export interface RestoreMemoryResult {
  restored: number;
  dry_run?: true;
  would_restore?: number;
  memory_ids: string[];
}

// ─── Wait-for-lock ────────────────────────────────────────────────────────────

export interface WaitForLockParams {
  agentId?: string;
  targetFiles?: string[];
  waitMs?: number;               // max wait time ms (default 60000)
  retryIntervalMs?: number;      // poll interval ms (default 5000)
}

// ─── Prune-stale ──────────────────────────────────────────────────────────────

export interface PruneStaleParams {
  dryRun?: boolean;
  olderThanMinutes?: number;     // treat locks acquired >= N minutes ago as stale (default 20)
  expiredOnly?: boolean;         // only prune locks past expires_at (ignore age)
  agentId?: string;
  targetFiles?: string[];
}
