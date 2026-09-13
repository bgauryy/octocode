export type StoreRetirementBlockerCode =
  | 'open_sessions'
  | 'active_agents'
  | 'unfinished_plans'
  | 'unfinished_tasks'
  | 'active_runs'
  | 'live_task_claims'
  | 'live_run_files'
  | 'live_locks'
  | 'pending_interactions'
  | 'live_authorizations'
  | 'open_signals'
  | 'open_history_captures'
  | 'active_history_restores'
  | 'pending_experience_archives';

export interface StoreRetirementBlocker {
  code: StoreRetirementBlockerCode;
  count: number;
}

export interface StoreRetirementTarget {
  kind: 'sqlite' | 'local_git' | 'legacy_local_git';
  source: string;
  quarantine: string;
  exists: boolean;
  type: 'file' | 'directory' | 'missing';
  device: number | null;
  inode: number | null;
  size: number | null;
  modified_ms: number | null;
}

export interface StoreRetirementReport {
  action: 'report';
  dry_run: true;
  report_id: string;
  plan_digest: string;
  generated_at: string;
  confirmation: 'retire';
  writer_probe: 'deferred_to_apply';
  can_apply: boolean;
  database: { path: string; store_id: string };
  workspaces: string[];
  blockers: StoreRetirementBlocker[];
  targets: StoreRetirementTarget[];
}

export interface StoreRetirementResult {
  action: 'apply';
  status: 'quarantined';
  report_id: string;
  database: { path: string; store_id: string };
  writer_fence: 'exclusive_transaction_acquired';
  quarantined: Array<Pick<StoreRetirementTarget, 'kind' | 'source' | 'quarantine'>>;
  absent: Array<Pick<StoreRetirementTarget, 'kind' | 'source' | 'quarantine'>>;
  recovery: 'rename each quarantine path back to its exact source path before restarting Awareness writers';
}

export interface StoreRetirementInput {
  database: string;
  workspaces?: readonly string[];
}

export interface ApplyStoreRetirementInput {
  report: StoreRetirementReport;
  confirm?: 'retire';
}
