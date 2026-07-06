/**
 * types.ts — Shared interfaces and types for @octocodeai/octocode-awareness.
 */

// ─── Domain types ─────────────────────────────────────────────────────────────

export type MemoryState = 'ACTIVE' | 'SUPERSEDED';
export type LockType = 'EXCLUSIVE' | 'SHARED';
export type IntentStatus = 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED';
export type RefinementQuality = 'good' | 'bad' | 'handoff';
export type RefinementState = 'open' | 'ongoing' | 'done';
export type ReflectionOutcome = 'worked' | 'partial' | 'failed';

// ─── Public shapes ────────────────────────────────────────────────────────────

export interface MemoryRecord {
  memory_id: string;
  agent_id: string;
  task_context: string;
  observation: string;
  importance_score: number;
  state: MemoryState;
  label: string;
  superseded_by: string | null;
  tags: string[];
  references: string[];
  workspace_path: string | null;
  repo: string | null;
  ref: string | null;
  file: string | null;
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
  similar_memory_ids: string[];
  /** Decay + salience score — present after lexicalSearch */
  score?: number;
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

export interface IntentRecord {
  intent_id: string;
  agent_id: string;
  session_id?: string | null;
  lock_type: LockType;
  workspace_path: string;
  target_files: string[];
  locks: FileLock[];
  status: IntentStatus;
  created_at: string;
}

// ─── Input params ─────────────────────────────────────────────────────────────

export interface InsertMemoryParams {
  agentId?: string;
  taskContext: string;
  observation: string;
  importanceScore: number;
  label?: string;
  tags?: string[];
  tagsCsv?: string;
  references?: string[];
  supersedes?: string[];
  failureSignature?: string | null;
  validFrom?: string | null;
  validTo?: string | null;
  workspacePath?: string | null;
  repo?: string | null;
  ref?: string | null;
  file?: string | null;
  fileTreeFingerprint?: string | null;
  cwd?: string;
}

export interface InsertMemoryResult {
  memoryId: string;
  memory: {
    memory_id: string;
    agent_id: string;
    task_context: string;
    observation: string;
    importance_score: number;
    label: string;
    tags: string[];
    references: string[];
    workspace_path: string | null;
    repo: string | null;
    ref: string | null;
    file: string | null;
    failure_signature: string | null;
    novelty_score: number | null;
    similar_memory_ids: string[];
    state: 'ACTIVE';
    created_at: string;
  };
  superseded: string[];
  noveltyScore: number;
  similarMemoryIds: string[];
}

export interface GetMemoryParams {
  query?: string;
  limit?: number;
  minImportance?: number;
  label?: string | string[];
  tags?: string[];
  smart?: boolean | string;
  workspacePath?: string | null;
  states?: string[];
  sort?: string;
  globalOnly?: boolean;
  strictScope?: boolean;
  asOf?: string | null;
  references?: string[];     // exact provenance filter
  regex?: string[];           // regex matched against all text fields
  fileRegex?: string[];       // regex matched against file path
  files?: string[];           // exact file path filter
}

export interface GetMemoryResult {
  count: number;
  memories: MemoryRecord[];
  mode: 'lexical' | 'fallback';
  sort: string;
  as_of: string | null;
  global_only: boolean;
  states: string[];
}

export interface InsertRefinementParams {
  agentId?: string;
  reasoning: string;
  remember: string;
  quality?: RefinementQuality;
  state?: RefinementState;
  workspacePath?: string | null;
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
  repo?: string | null;
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

export interface PreFlightIntentParams {
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  rationale?: string;
  testPlan?: string;
  targetFiles?: string[];
  lockType?: LockType;
  ttlMs?: number | null;
}

export interface PreFlightIntentSuccess {
  ok: true;
  intent: IntentRecord;
}

export interface PreFlightIntentConflict {
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

export type PreFlightIntentResult = PreFlightIntentSuccess | PreFlightIntentConflict;

export interface ReleaseFileLockParams {
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  intentId?: string | null;
  targetFiles?: string[];
  status?: IntentStatus;
  verified?: boolean;          // record that test_plan was actually run
  verifiedNote?: string;       // what was verified (e.g. 'yarn test: 273 passed')
}

export interface FileLockParams {
  type: 'lock' | 'release' | 'status' | 'renew';
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  intentId?: string | null;
  targetFiles?: string[];
  lockType?: LockType;
  ttlMs?: number | null;
  status?: IntentStatus;
  verified?: boolean;
  verifiedNote?: string;
}

export interface ReleaseFileLockResult {
  agent_id: string;
  status: IntentStatus;
  released: boolean;
  locks_released: number;
  intent_ids: string[];
  updated_at: string;
  unverifiedConclusion?: string;
}

export interface FileLockStatusEntry {
  lock_id: string;
  intent_id: string;
  file_path: string;
  agent_id: string;
  session_id: string | null;
  workspace_path: string | null;
  lock_type: LockType;
  acquired_at: string;
  expires_at: string | null;
}

export type FileLockResult =
  | { ok: true; type: 'lock'; intentId: string; files: string[]; locks: FileLockStatusEntry[]; expiresAt: string | null }
  | { ok: false; type: 'lock'; conflict: true; conflicts: PreFlightIntentConflict['conflicts'] }
  | ({ ok: true; type: 'release' } & ReleaseFileLockResult)
  | { ok: true; type: 'status'; locks: FileLockStatusEntry[] }
  | { ok: true; type: 'renew'; intentId: string; renewed: boolean; locks_renewed: number; expiresAt: string | null };

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
  references?: string[];
  file?: string | null;
  files?: string[];
  folders?: string[];
  validFrom?: string | null;
  validTo?: string | null;
  workspacePath?: string | null;
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
  eval_failure_ids: never[];
  next: string;
  novelty_score?: number;
  similar_memory_ids?: string[];
}

export interface ScopePartial {
  workspace_path?: string | null;
  repo?: string | null;
  ref?: string | null;
}

export interface Scope {
  workspace_path: string | null;
  repo: string | null;
  ref: string | null;
}

// ─── Internal raw DB row shapes ───────────────────────────────────────────────

export interface MemoryRow {
  memory_id: string;
  agent_id: string;
  task_context: string;
  observation: string;
  importance_score: number;
  state: string;
  label: string;
  superseded_by: string | null;
  tags_json: string;
  tags_text: string;
  references_json: string;
  workspace_path: string | null;
  repo: string | null;
  ref: string | null;
  file_tree_fingerprint: string | null;
  file: string | null;
  novelty_score: number | null;
  similar_memory_ids_json: string;
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
  intent_id: string;
  agent_id: string;
  lock_type: string;
  acquired_at: string;
  expires_at: string | null;
  intent_agent_id?: string;
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

export interface IntentIdRow {
  intent_id: string;
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
  before?: string;           // ISO — delete memories created before this
  maxImportance?: number;    // safety ceiling — only delete at or below this score
  dryRun?: boolean;
  cwd?: string;
}

export interface ForgetMemoryResult {
  deleted: number;
  dry_run?: true;
  would_delete?: number;
  memory_ids: string[];
}

// ─── Real wait-for-lock ───────────────────────────────────────────────────────

export interface WaitForLockParams {
  agentId?: string;
  targetFiles?: string[];
  lockType?: LockType;
  waitMs?: number;           // max wait time ms (default 60000)
  retryIntervalMs?: number;  // poll interval ms (default 5000)
}

export interface WaitForLockResult {
  ok: true;
  waited_ms: number;
  lock_free: boolean;
  conflicts?: Array<{ file_path: string; agent_id: string; expires_at: string | null }>;
}

// ─── Enhanced prune-stale ─────────────────────────────────────────────────────

export interface PruneStaleParams {
  dryRun?: boolean;
  olderThanMinutes?: number; // treat locks acquired >= N minutes ago as stale (default 20)
  expiredOnly?: boolean;     // only prune locks past expires_at (ignore age)
  agentId?: string;
  targetFiles?: string[];
}

export interface PruneStaleResult {
  pruned_locks: number;
  updated_intents: number;
  dry_run?: true;
  would_prune?: number;
}

// ─── Verify enhancements ─────────────────────────────────────────────────────

export interface MarkVerifiedParams {
  intentId?: string;         // verify one intent by id
  agentId?: string;
  allPending?: boolean;      // verify all pending intents for this agent/workspace
  workspacePath?: string;    // scope for allPending
  message?: string;          // what was verified
  status?: 'SUCCESS' | 'FAILED';
}

export interface MarkVerifiedResult {
  ok: boolean;
  intent_id?: string;
  intent_ids?: string[];     // when allPending=true
  status?: string;
  count?: number;
  error?: string;
}

// ─── Audit enhancements ──────────────────────────────────────────────────────

export interface AuditUnverifiedParams {
  agentId?: string | null;
  workspacePath?: string;
  abandon?: boolean;         // dismiss all found PENDING intents as orphaned
}

// ─── Delete refinement ───────────────────────────────────────────────────────

export interface DeleteRefinementParams {
  refinementIds: string[];
  workspacePath?: string;
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
  notification_id: string;
  workspace_path: string;
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
  in_reply_to: string | null;
  importance: number;
  status: NotificationStatus;
  created_at: string;
}

export interface InsertNotificationParams {
  agentId: string;
  workspacePath?: string | null;
  repo?: string | null;
  ref?: string | null;
  toAgent?: string | null;
  kind: NotificationKind;
  subject: string;
  body?: string | null;
  files?: string[];
  refIds?: string[];         // related intent/refinement/memory ids
  inReplyTo?: string | null; // inherits thread from parent
  importance?: number;
  cwd?: string;
}

export interface InsertNotificationResult {
  notification_id: string;
  thread_id: string;
  workspace_path: string;
}

export interface GetNotificationsParams {
  agentId: string;
  workspacePath?: string | null;
  repo?: string | null;
  ref?: string | null;
  kinds?: NotificationKind[];
  threadId?: string | null;
  unreadOnly?: boolean;       // default true
  markRead?: boolean;         // advance read cursor
  limit?: number;
  cwd?: string;
}

export interface GetNotificationsResult {
  count: number;
  notifications: NotificationRecord[];
  unread_only: boolean;
}

export interface ResolveNotificationParams {
  notificationIds?: string[];
  threadId?: string | null;
  workspacePath?: string | null;
  cwd?: string;
}

export interface ResolveNotificationResult {
  resolved: number;
  notification_ids: string[];
}

export interface PruneNotificationsParams {
  workspacePath?: string | null;
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
  notification_ids: string[];
}

export type AgentSignalAction = 'publish' | 'list' | 'reply' | 'resolve' | 'ack';

export interface AgentSignalParams {
  action: AgentSignalAction;
  agentId: string;
  workspacePath?: string | null;
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
  notificationIds?: string[];
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
  | { action: 'publish' | 'reply'; notification_id: string; notification_ids: string[]; thread_id: string; workspace_path: string }
  | { action: 'list'; count: number; signals: AgentSignalRecord[]; unread_only: boolean }
  | { action: 'resolve'; resolved: number; notification_ids: string[] }
  | { action: 'ack'; acknowledged: number; notification_ids: string[] };

// ─── Export harness ──────────────────────────────────────────────────────────

export interface ExportHarnessParams {
  limit?: number;
  minImportance?: number;
  workspacePath?: string | null;
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
