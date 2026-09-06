import type { FileLock } from './types/identity-memory.js';

export interface DigestResult {
  ok: true;
  archived_memories: number;   // valid_to expired (or would_archive in dry_run)
  pruned_old: number;          // SUPERSEDED older than retention_days
  pruned_locks: number;        // expired file locks
  pruned_refinements: number;  // stale handoffs (any state) and done refinements
  resolved_handoff_signals: number; // open handoff signals auto-resolved past TTL
  failed_stale_active_runs: number; // explicit recovery for ACTIVE runs with expired presence
  pruned_runs: number;         // old terminal standalone WORK/HOOK rows
  fts_rebuilt: boolean;
  dry_run?: true;
  would_archive?: number;
  would_prune_old?: number;
  would_prune_locks?: number;
  would_prune_refinements?: number;
  would_resolve_handoff_signals?: number;
  would_fail_stale_active_runs?: number;
  would_prune_runs?: number;
  pressure_age_days?: number;
  stale_pending_runs?: number;
  stale_active_runs?: number;
  stale_open_signals?: number;
  stale_handoff_signals?: number;
  stale_missing_refs?: number;
  pressure_samples?: MaintenancePressure['samples'];
  candidate_limit?: number;
  candidate_ids?: {
    expire_memory_ids: string[];
    purge_memory_ids: string[];
    locks: FileLock[];
    refinement_ids: string[];
    run_ids: string[];
    stale_active_run_ids: string[];
  };
}

export interface MaintenancePressure {
  pressure_age_days: number;
  cutoff: string;
  stale_pending_runs: number;
  stale_active_runs: number;
  stale_open_signals: number;
  stale_handoff_signals: number;
  stale_missing_refs: number;
  samples: {
    run_ids: string[];
    active_run_ids: string[];
    signal_ids: string[];
    handoff_signal_ids: string[];
    memory_ids: string[];
  };
}
