import type { LockType, ReflectionOutcome, RunRecord, RunStatus } from '../types.js';

/** Run pre-flight — checks for lock conflicts before acquiring. */
export interface PreFlightRunParams {
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  runId?: string | null;
  rationale?: string;
  testPlan?: string;
  contextRef?: string | null;
  targetFiles?: string[];
  ttlMs?: number | null;
}

export interface PreFlightRunSuccess {
  ok: true;
  run: RunRecord;
}

export interface PreFlightRunConflict {
  ok: false;
  conflict: true;
  conflicts: Array<{
    file_path: string;
    lock_type: LockType;
    agent_id: string;
    acquired_at: string;
    expires_at: string | null;
    // Who/why context so a blocked agent can decide (wait / work elsewhere /
    // signal the holder) instead of only seeing who holds the lock and until when.
    run_id: string;
    reasoning: string;      // the holder's rationale — WHY the file is claimed
    test_plan: string;      // what the holder intends to verify before release
    session_id: string | null;
    holder_session_active: boolean;  // false = holder's session ended → likely abandoned
  }>;
}

export type PreFlightRunResult = PreFlightRunSuccess | PreFlightRunConflict;

export interface ReleaseFileLockParams {
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  runId?: string | null;
  targetFiles?: string[];
  status?: RunStatus;
  verified?: boolean;            // record that test_plan was actually run
  verifiedNote?: string;         // what was verified (e.g. 'yarn test: 273 passed')
}

export interface FileLockParams {
  type: 'lock' | 'release' | 'status' | 'renew';
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  runId?: string | null;
  targetFiles?: string[];
  ttlMs?: number | null;
  reasoning?: string | null;
  status?: RunStatus;
  verified?: boolean;
  verifiedNote?: string;
}

export interface ReleaseFileLockResult {
  agent_id: string;
  status: RunStatus;
  released: boolean;
  locks_released: number;
  run_ids: string[];
  updated_at: string;
  unverifiedConclusion?: string;
  ambiguousRelease?: string;
}

export interface FileLockStatusEntry {
  lock_id: string;
  run_id: string;
  file_path: string;
  agent_id: string;
  session_id: string | null;
  workspace_path: string | null;
  artifact: string | null;
  reasoning: string;
  test_plan?: string;
  lock_type: LockType;
  acquired_at: string;
  expires_at: string | null;
}

export interface AcquireFileLockResult {
  ok: true;
  type: 'lock';
  runId: string;
  files: string[];
  reasoning: string;
  acquiredAt: string | null;
  expiresAt: string | null;
  locks: FileLockStatusEntry[];
}

export type FileLockResult =
  | AcquireFileLockResult
  | { ok: false; type: 'lock'; conflict: true; conflicts: PreFlightRunConflict['conflicts'] }
  | ({ ok: boolean; type: 'release' } & ReleaseFileLockResult)
  | { ok: true; type: 'status'; locks: FileLockStatusEntry[] }
  | { ok: true; type: 'renew'; runId: string; renewed: boolean; locks_renewed: number; expiresAt: string | null };

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
  /**
   * Feedback to the human developer who authored this agent's operating instructions
   * (AGENTS.md, SKILL.md, system prompt, task brief). Use when the instructions —
   * not the code and not the harness — were ambiguous, wrong, over-constraining, or
   * missing context. Tags the learning memory `developer-review` and opens a tracked
   * `instructions`-quality refinement; both surface in the live developer-review view.
   */
  fixInstructions?: string | null;
  failureSignature?: string | null;
  importance?: number | null;
  /** Judgment nuance: evidence checked, remaining uncertainty. Folded into the narrative. */
  judgmentNote?: string | null;
  /** Emit an advisory reflection_duo packet (two reviewer roles) in the result. Never stored. */
  duo?: boolean;
  /** Structured eval failures; each becomes an `eval`-tagged memory. */
  evalFailures?: EvalFailure[];
  /** Explicitly retain a materially distinct recurrence that resembles existing memory. */
  allowSimilar?: boolean;
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
  learning_memory_skipped?: true;
  repo_fix_refinement_id: string | null;
  harness_fix: boolean;
  /** True when this reflection carried feedback to the instruction author (`--fix-instructions`). */
  instructions_feedback: boolean;
  /** Refinement id for the tracked instructions-feedback item, when one was opened. */
  developer_review_refinement_id: string | null;
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
