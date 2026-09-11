import type { FileLock, RunRecord } from './identity-memory.js';

export type FileLockReleaseStatus = 'ACTIVE' | 'PENDING' | 'FAILED';

/** Run pre-flight — checks for lock conflicts before acquiring. */
export interface PreFlightRunParams {
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  runId?: string | null;
  rationale?: string;
  testPlan?: string;
  /** Require a caller-supplied contract when this operation creates a run. */
  requireRunContract?: boolean;
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
  conflicts: FileLock[];
}

export type PreFlightRunResult = PreFlightRunSuccess | PreFlightRunConflict;

export interface ReleaseFileLockParams {
  agentId?: string;
  sessionId?: string | null;
  workspacePath?: string | null;
  artifact?: string | null;
  runId?: string | null;
  targetFiles?: string[];
  status?: FileLockReleaseStatus;
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
  testPlan?: string | null;
  status?: FileLockReleaseStatus;
}

export interface ReleaseFileLockResult {
  agent_id: string;
  status: FileLockReleaseStatus;
  released: boolean;
  locks_released: number;
  run_ids: string[];
  updated_at: string;
  ambiguousRelease?: string;
}

export interface AcquireFileLockResult {
  ok: true;
  type: 'lock';
  run_id: string;
  locks: FileLock[];
}

export type FileLockResult =
  | AcquireFileLockResult
  | { ok: false; type: 'lock'; conflict: true; conflicts: PreFlightRunConflict['conflicts'] }
  | ({ ok: boolean; type: 'release' } & ReleaseFileLockResult)
  | { ok: true; type: 'status'; locks: FileLock[] }
  | { ok: true; type: 'renew'; run_id: string; renewed: boolean; locks_renewed: number; expires_at: string | null };

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
