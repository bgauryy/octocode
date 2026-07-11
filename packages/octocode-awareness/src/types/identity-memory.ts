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
export type LockType = 'EXCLUSIVE';
export type RunOrigin = 'TASK' | 'WORK' | 'HOOK';
export type WorkSource = 'EXPLICIT' | 'HOOK';
/** Maps to the task_runs table status column. */
export type RunStatus = 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED';
export type RefinementQuality = 'good' | 'bad' | 'handoff' | 'instructions';
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

/** One standalone or task-linked execution/verification attempt. */
export interface RunRecord {
  run_id: string;
  task_id: string | null;
  origin: RunOrigin;
  agent_id: string;
  session_id?: string | null;
  workspace_path: string;
  artifact: string | null;
  context_ref: string | null;
  target_files: string[];
  locks: FileLock[];
  status: RunStatus;
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
  /** Optional preselected candidate ids, used by alternate rankers before final filtering. */
  candidateMemoryIds?: string[];
  /** Set false when a caller will record access only after applying a final alternate ranking. */
  recordAccess?: boolean;
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
  /** Present with explain=true so callers can audit the effective query after smart widening. */
  applied_filters?: {
    query: string;
    limit: number;
    min_importance: number;
    labels: string[];
    tags: string[];
    references: string[];
    files: string[];
    file_regex: string[];
    regex: string[];
    workspace_path: string | null;
    artifact: string | null;
    repo: string | null;
    ref: string | null;
    strict_scope: boolean;
    global_only: boolean;
    states: string[];
    as_of: string | null;
    sort: string;
    smart: boolean;
  };
  /** Set when recall confidence is low — verify results before relying on them. */
  judgment_required?: boolean;
  judgment_reason?: string;
  /** True when smart recall widened an underfilled query. */
  smart_expanded?: boolean;
  /** Exact caller filters omitted by the smart widening pass. */
  smart_dropped_filters?: string[];
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
  /** Present when handoffs are excluded by default — use --include-handoffs to list them. */
  handoff_count?: number;
  /** Present when instructions-feedback refinements are excluded by default — see `reflect developer-review`. */
  instructions_count?: number;
}
