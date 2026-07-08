/**
 * types.ts — Shared interfaces and types for @octocodeai/octocode-awareness.
 */

// ─── Domain types ─────────────────────────────────────────────────────────────

// ─── Agent Identity ───────────────────────────────────────────────────────────

export interface AgentIdentity {
  agent_id: string;
  agent_name: string;            // human-readable display name; '' if unknown
  workspace_path: string | null; // primary workspace this agent was last seen in
  artifact: string | null;       // optional package/service slice in that workspace
  context: string | null;        // tool context: 'pi' | 'cursor' | 'claude-code'
  registered_at: string;
  last_seen_at: string;
}

export interface RegisterAgentParams {
  agentId: string;
  agentName?: string | null;     // '' or omit if unknown
  workspacePath?: string | null;
  artifact?: string | null;
  context?: string | null;       // 'pi' | 'cursor' | 'claude-code' | etc
}

export interface ListAgentsResult {
  count: number;
  agents: AgentIdentity[];
}

// ─── Session ──────────────────────────────────────────────────────────────────

export interface Session {
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

export interface InsertSessionParams {
  agentId: string;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
}

// ─── Embedding search ─────────────────────────────────────────────────────────

/** Cosine-similarity result from searchByEmbedding(). */
export interface EmbeddingSearchResult {
  memory_id: string;
  similarity: number; // 0–1
}

export type MemoryState = 'ACTIVE' | 'SUPERSEDED';
export type LockType = 'EXCLUSIVE' | 'SHARED';
/** Maps to the tasks table status column. */
export type TaskStatus = 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED';
export type RefinementQuality = 'good' | 'bad' | 'handoff';
export type RefinementState = 'open' | 'ongoing' | 'done';
export type ReflectionOutcome = 'worked' | 'partial' | 'failed';

// ─── Public shapes ────────────────────────────────────────────────────────────

export interface MemoryRecord {
  memory_id: string;
  agent_id: string;
  task_context: string;
  observation: string;
  importance: number;
  state: MemoryState;
  label: string;
  superseded_by: string | null;
  tags: string[];
  references: string[];
  workspace_path: string | null;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  failure_signature: string | null;
  access_count: number;
  last_accessed_at: string | null;
  decay_half_life_days: number | null;
  valid_from: string | null;
  valid_to: string | null;
  expired_at: string | null;
  file_tree_fingerprint: string | null;
  created_at: string;
  updated_at: string | null;
  novelty_score: number | null;
  /** Decay + salience score — present after lexicalSearch */
  score?: number;
  /** Normalized lexical relevance (0..1) — present after lexicalSearch */
  lexical?: number;
  /** Per-component score breakdown — present when getMemory({ explain: true }) */
  score_components?: {
    importance: number;
    recency: number;
    access: number;
    relevance: number;
    weights: { importance: number; recency: number; access: number; lexical: number };
    final: number;
  };
}

export interface FileLock {
  lock_id: string;
  file_path: string;
  lock_type: LockType;
  agent_id: string;
  session_id?: string | null;
  acquired_at: string;
  expires_at: string | null;
}

export interface RefinementRecord {
  refinement_id: string;
  agent_id: string;
  workspace_path: string;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  files: string[];
  reasoning: string;
  remember: string;
  quality: RefinementQuality;
  state: RefinementState;
  created_at: string;
  updated_at: string;
}

/** Maps to the tasks table. */
export interface TaskRecord {
  task_id: string;
  agent_id: string;
  session_id?: string | null;
  lock_type: LockType;
  workspace_path: string;
  artifact: string | null;
  plan_doc_ref: string | null;
  target_files: string[];
  locks: FileLock[];
  status: TaskStatus;
  created_at: string;
}

// ─── Input params ─────────────────────────────────────────────────────────────

export interface InsertMemoryParams {
  agentId?: string;
  taskContext: string;
  observation: string;
  importance: number;
  label?: string;
  tags?: string[];
  tagsCsv?: string;
  references?: string[];
  supersedes?: string[];
  failureSignature?: string | null;
  validFrom?: string | null;
  validTo?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  fileTreeFingerprint?: string | null;
  cwd?: string;
  /**
   * TOOL-2: Pre-computed similar memories from a prior findSimilarMemories call.
   * When provided, insertMemory skips its own internal findSimilarMemories query.
   */
  preComputedSimilar?: Array<{ memory_id: string; similarity: number }>;
}

export interface InsertMemoryResult {
  memoryId: string;
  memory: {
    memory_id: string;
    agent_id: string;
    task_context: string;
    observation: string;
    importance: number;
    label: string;
    tags: string[];
    references: string[];
    workspace_path: string | null;
    artifact: string | null;
    repo: string | null;
    ref: string | null;
    failure_signature: string | null;
    novelty_score: number | null;
    state: 'ACTIVE';
    created_at: string;
  };
  superseded: string[];
  noveltyScore: number;
  similarMemoryIds: string[];
}

/**
 * GetMemoryParams — query parameters for memory recall.
 *
 * Scope resolution order (first non-null wins):
 *   1. workspacePath (explicit absolute path)
 *   2. cwd (resolved to absolute path at call time)
 * When both are absent, the query is global.
 */
export interface GetMemoryParams {
  query?: string;
  limit?: number;
  minImportance?: number;
  label?: string | string[];
  tags?: string[];
  smart?: boolean | string;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  states?: string[];
  sort?: string;
  globalOnly?: boolean;
  strictScope?: boolean;
  asOf?: string | null;
  references?: string[];       // exact provenance filter
  regex?: string[];             // regex matched against all text fields
  fileRegex?: string[];         // regex matched against file path
  files?: string[];             // exact file path filter
  explain?: boolean;            // attach score_components per result for tuning
  /** Base directory for resolving relative file paths; falls back to workspacePath when absent. */
  cwd?: string;
}

export interface GetMemoryResult {
  count: number;
  memories: MemoryRecord[];
  mode: 'lexical' | 'fallback' | 'semantic';
  sort: string;
  as_of: string | null;
  global_only: boolean;
  states: string[];
  /** Set when recall confidence is low — verify results before relying on them. */
  judgment_required?: boolean;
  judgment_reason?: string;
}

export interface InsertRefinementParams {
  agentId?: string;
  reasoning: string;
  remember: string;
  quality?: RefinementQuality;
  state?: RefinementState;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  files?: string[];
  cwd?: string;
}

export interface InsertRefinementResult {
  refinementId: string;
  refinement: RefinementRecord;
}

export interface GetRefinementsParams {
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  quality?: RefinementQuality;
  includeHandoffs?: boolean;
  states?: string[];
  limit?: number;
  cwd?: string;
}

export interface GetRefinementsResult {
  count: number;
  refinements: RefinementRecord[];
}

/** Task pre-flight — checks for lock conflicts before acquiring. */
export interface PreFlightTaskParams {
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  rationale?: string;
  testPlan?: string;
  planDocRef?: string | null;
  targetFiles?: string[];
  lockType?: LockType;
  ttlMs?: number | null;
}

export interface PreFlightTaskSuccess {
  ok: true;
  task: TaskRecord;
}

export interface PreFlightTaskConflict {
  ok: false;
  conflict: true;
  conflicts: Array<{
    file_path: string;
    lock_type: LockType;
    agent_id: string;
    acquired_at: string;
    expires_at: string | null;
  }>;
}

export type PreFlightTaskResult = PreFlightTaskSuccess | PreFlightTaskConflict;

export interface ReleaseFileLockParams {
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  taskId?: string | null;
  targetFiles?: string[];
  status?: TaskStatus;
  verified?: boolean;            // record that test_plan was actually run
  verifiedNote?: string;         // what was verified (e.g. 'yarn test: 273 passed')
}

export interface TaskParams {
  type: 'lock' | 'release' | 'status' | 'renew';
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  taskId?: string | null;
  targetFiles?: string[];
  lockType?: LockType;
  ttlMs?: number | null;
  reasoning?: string | null;
  status?: TaskStatus;
  verified?: boolean;
  verifiedNote?: string;
}

/** @deprecated Use TaskParams. */
export type FileLockParams = TaskParams;

export interface ReleaseFileLockResult {
  agent_id: string;
  status: TaskStatus;
  released: boolean;
  locks_released: number;
  task_ids: string[];
  updated_at: string;
  unverifiedConclusion?: string;
}

export interface FileLockStatusEntry {
  lock_id: string;
  task_id: string;
  file_path: string;
  agent_id: string;
  session_id: string | null;
  workspace_path: string | null;
  artifact: string | null;
  reasoning: string;
  lock_type: LockType;
  acquired_at: string;
  expires_at: string | null;
}

export interface AcquireFileLockResult {
  ok: true;
  type: 'lock';
  taskId: string;
  files: string[];
  reasoning: string;
  acquiredAt: string | null;
  expiresAt: string | null;
  locks: FileLockStatusEntry[];
}

export type FileLockResult =
  | AcquireFileLockResult
  | { ok: false; type: 'lock'; conflict: true; conflicts: PreFlightTaskConflict['conflicts'] }
  | ({ ok: boolean; type: 'release' } & ReleaseFileLockResult)
  | { ok: true; type: 'status'; locks: FileLockStatusEntry[] }
  | { ok: true; type: 'renew'; taskId: string; renewed: boolean; locks_renewed: number; expiresAt: string | null };

/** One failed binary-eval question, treated as a diagnostic packet. */
export interface EvalFailure {
  id: string;
  dimension?: string;
  failure_signature?: string;
  suggested_lesson?: string;
}

/** Advisory reviewer prompts emitted by reflect({ duo: true }). Never stored. */
export interface ReflectionDuo {
  advisory: true;
  roles: [
    { role: 'supporter'; prompt: string },
    { role: 'skeptic'; prompt: string },
  ];
}

export interface ReflectParams {
  agentId?: string;
  task: string;
  outcome?: ReflectionOutcome | string;
  lesson?: string | null;
  worked?: string | null;
  didntWork?: string | null;
  fixRepo?: string | null;
  fixHarness?: string | null;
  failureSignature?: string | null;
  importance?: number | null;
  /** Judgment nuance: evidence checked, remaining uncertainty. Folded into the narrative. */
  judgmentNote?: string | null;
  /** Emit an advisory reflection_duo packet (two reviewer roles) in the result. Never stored. */
  duo?: boolean;
  /** Structured eval failures; each becomes an `eval`-tagged memory. */
  evalFailures?: EvalFailure[];
  references?: string[];
  file?: string | null;
  files?: string[];
  folders?: string[];
  validFrom?: string | null;
  validTo?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  cwd?: string;
}

export interface ReflectResult {
  outcome: ReflectionOutcome;
  learning_memory_id: string;
  repo_fix_refinement_id: string | null;
  harness_fix: boolean;
  eval_failure_count: number;
  eval_failure_ids: string[];
  next: string;
  novelty_score?: number;
  similar_memory_ids?: string[];
  reflection_duo?: ReflectionDuo;
}

export interface ScopePartial {
  workspace_path?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
}

export interface Scope {
  workspace_path: string | null;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
}

// ─── Weakness clustering ──────────────────────────────────────────────────────

export interface WeaknessCluster {
  failure_signature: string;  // raw (may include |surface:Z suffix)
  base_signature: string;     // without |surface:Z — use this for display/grouping
  surfaces: string[];         // extracted surface values across all merged signatures
  count: number;
  avg_importance: number;
  score: number;
  memory_ids: string[];
  representative: string;
  labels: string[];
}

export interface MineWeaknessResult {
  ok: true;
  clusters: WeaknessCluster[];
  total_signatures: number;
  total_memories: number;
}

export interface MineWeaknessParams {
  agentId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  minCount?: number;
  limit?: number;
  cwd?: string;
}

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

export interface RefinementRow {
  refinement_id: string;
  agent_id: string;
  workspace_path: string;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  files_json: string;
  reasoning: string;
  remember: string;
  quality: string;
  state: string;
  created_at: string;
  updated_at: string;
}

export interface FileLockRow {
  lock_id: string;
  file_path: string;
  task_id: string;
  agent_id: string;
  lock_type: string;
  acquired_at: string;
  expires_at: string | null;
  task_agent_id?: string;
}

export interface TableInfoRow {
  name: string;
}

export interface CountRow {
  count: number;
}

export interface StateCountRow {
  state: string;
  count: number;
}

export interface LabelCountRow {
  label: string;
  count: number;
}

export interface MetaRow {
  value: string;
}

export interface FtsRow {
  memory_id: string;
}

export interface TaskIdRow {
  task_id: string;
}

export interface RunResult {
  changes: number;
  lastInsertRowid: number | bigint;
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

// ─── Wait-for-lock ────────────────────────────────────────────────────────────

export interface WaitForLockParams {
  agentId?: string;
  targetFiles?: string[];
  lockType?: LockType;
  waitMs?: number;               // max wait time ms (default 60000)
  retryIntervalMs?: number;      // poll interval ms (default 5000)
}

export interface WaitForLockResult {
  ok: true;
  waited_ms: number;
  lock_free: boolean;
  conflicts?: Array<{ file_path: string; agent_id: string; expires_at: string | null }>;
}

// ─── Prune-stale ──────────────────────────────────────────────────────────────

export interface PruneStaleParams {
  dryRun?: boolean;
  olderThanMinutes?: number;     // treat locks acquired >= N minutes ago as stale (default 20)
  expiredOnly?: boolean;         // only prune locks past expires_at (ignore age)
  agentId?: string;
  targetFiles?: string[];
}

export interface PruneStaleResult {
  pruned_locks: number;
  updated_tasks: number;
  dry_run?: true;
  would_prune?: number;
}

// ─── Verify ───────────────────────────────────────────────────────────────────

export interface MarkVerifiedParams {
  taskId?: string;               // verify one task by id
  agentId?: string;
  allPending?: boolean;          // verify all pending tasks for this agent/workspace
  workspacePath?: string;        // scope for allPending
  artifact?: string | null;
  message?: string;              // what was verified
  status?: 'SUCCESS' | 'FAILED';
}

export interface MarkVerifiedResult {
  ok: boolean;
  task_id?: string;
  task_ids?: string[];           // when allPending=true
  status?: string;
  count?: number;
  error?: string;
}

// ─── Audit ────────────────────────────────────────────────────────────────────

export interface AuditUnverifiedParams {
  agentId?: string | null;
  workspacePath?: string;
  artifact?: string | null;
  abandon?: boolean;             // dismiss all found PENDING tasks as orphaned
}

// ─── Delete refinement ───────────────────────────────────────────────────────

export interface DeleteRefinementParams {
  refinementIds: string[];
  workspacePath?: string;
  artifact?: string | null;
  dryRun?: boolean;
}

export interface DeleteRefinementResult {
  deleted: number;
  dry_run?: true;
  would_delete?: number;
  refinement_ids: string[];
}

// ─── Notifications ───────────────────────────────────────────────────────────

export type NotificationKind =
  | 'claim' | 'handoff' | 'question' | 'reply'
  | 'blocker' | 'request' | 'decision' | 'fyi';

export type NotificationStatus = 'open' | 'resolved';

export interface NotificationRecord {
  signal_id: string;
  workspace_path: string;
  artifact: string | null;
  repo: string | null;
  ref: string | null;
  from_agent: string;
  to_agent: string | null;
  kind: NotificationKind;
  subject: string;
  body: string | null;
  files: string[];
  refs: string[];
  thread_id: string;
  reply_to: string | null;
  importance: number;
  status: NotificationStatus;
  created_at: string;
}

export interface InsertNotificationParams {
  agentId: string;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  toAgent?: string | null;
  kind: NotificationKind;
  subject: string;
  body?: string | null;
  files?: string[];
  refIds?: string[];             // related task/refinement/memory ids
  inReplyTo?: string | null;     // inherits thread from parent
  importance?: number;
  cwd?: string;
}

export interface InsertNotificationResult {
  signal_id: string;
  thread_id: string;
  workspace_path: string;
  artifact: string | null;
}

export interface GetNotificationsParams {
  agentId: string;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  kinds?: NotificationKind[];
  threadId?: string | null;
  unreadOnly?: boolean;          // default true
  markRead?: boolean;            // advance read cursor
  limit?: number;
  cwd?: string;
}

export interface GetNotificationsResult {
  count: number;
  signals: NotificationRecord[];
  unread_only: boolean;
}

export interface ResolveNotificationParams {
  agentId?: string | null;
  notificationIds?: string[];
  threadId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  cwd?: string;
}

export interface ResolveNotificationResult {
  resolved: number;
  signal_ids: string[];
}

export interface PruneNotificationsParams {
  workspacePath?: string | null;
  artifact?: string | null;
  notificationIds?: string[];
  resolvedOnly?: boolean;
  olderThanDays?: number;
  dryRun?: boolean;
  cwd?: string;
}

export interface PruneNotificationsResult {
  deleted: number;
  dry_run?: true;
  would_delete?: number;
  signal_ids: string[];
}

export type AgentSignalAction = 'publish' | 'list' | 'reply' | 'resolve' | 'ack';

export interface AgentSignalParams {
  action: AgentSignalAction;
  agentId: string;
  workspacePath?: string | null;
  artifact?: string | null;
  repo?: string | null;
  ref?: string | null;
  kind?: NotificationKind;
  subject?: string;
  body?: string | null;
  toAgents?: string[];
  files?: string[];
  refs?: string[];
  importance?: number;
  inReplyTo?: string | null;
  threadId?: string | null;
  signalIds?: string[];
  unreadOnly?: boolean;
  markRead?: boolean;
  kinds?: NotificationKind[];
  limit?: number;
  cwd?: string;
}

export interface AgentSignalRecord extends NotificationRecord {
  to_agents: string[];
}

export type AgentSignalResult =
  | { action: 'publish' | 'reply'; signal_id: string; signal_ids: string[]; thread_id: string; workspace_path: string; artifact: string | null }
  | { action: 'list'; count: number; signals: AgentSignalRecord[]; unread_only: boolean }
  | { action: 'resolve'; resolved: number; signal_ids: string[] }
  | { action: 'ack'; acknowledged: number; signal_ids: string[] };

// ─── Export harness ──────────────────────────────────────────────────────────

export interface ExportHarnessParams {
  limit?: number;
  minImportance?: number;
  workspacePath?: string | null;
  artifact?: string | null;
  cwd?: string;
}

export interface ExportHarnessResult {
  count: number;
  markdown: string;
  memories: Array<{ memory_id: string; label: string; importance: number; observation: string }>;
}

// ─── Memory references ────────────────────────────────────────────────────────

export interface MemoryReferenceRow {
  memory_id: string;
  reference: string;
  kind: string;
  ordinal: number;
}

// ─── Edit log ─────────────────────────────────────────────────────────────────

export type EditOperation = 'create' | 'update' | 'delete' | 'move' | 'rename';

export interface EditLogRow {
  edit_id: string;
  session_id: string | null;
  task_id: string | null;
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
  taskId?: string | null;
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
  taskId?: string;
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
  task_id: string | null;
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
  taskId?: string | null;
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
  summary?: string | null;
}
