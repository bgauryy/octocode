/**
 * types.ts — Shared interfaces and types for @octocodeai/octocode-memory.
 */

// ─── Domain types ─────────────────────────────────────────────────────────────

export type MemoryState = 'ACTIVE' | 'SUPERSEDED';
export type LockType = 'EXCLUSIVE' | 'SHARED';
export type IntentStatus = 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED';
export type RefinementQuality = 'good' | 'bad';
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
  /** Decay + salience score — present after lexicalSearch */
  score?: number;
}

export interface FileLock {
  lock_id: string;
  file_path: string;
  lock_type: LockType;
  agent_id: string;
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
    state: 'ACTIVE';
    created_at: string;
  };
  superseded: string[];
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
  asOf?: string | null;
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
  intentId?: string | null;
  targetFiles?: string[];
  status?: IntentStatus;
}

export interface ReleaseFileLockResult {
  agent_id: string;
  status: IntentStatus;
  released: boolean;
  locks_released: number;
  intent_ids: string[];
  updated_at: string;
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
