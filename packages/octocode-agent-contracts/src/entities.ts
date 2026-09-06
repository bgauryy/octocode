/**
 * entities.ts — the canonical coordination-entity types.
 *
 * The domain shapes for the local coordination store (plans, tasks, locks, work
 * presence, handoffs, memory, agents, messages) live here so they are defined
 * ONCE and imported by every consumer (Awareness today; open to others).
 * Pure type declarations — no runtime, no dependencies.
 *
 * These are shared cross-host contracts for the Awareness-owned coordination
 * database. Sharing the TypeScript shapes does not imply shared physical
 * storage with the Agent control or runtime databases.
 */

export const PLAN_STATUSES = ['DRAFT', 'ACTIVE', 'PAUSED', 'COMPLETED', 'CANCELLED'] as const;
export const TASK_STATUSES = ['OPEN', 'IN_PROGRESS', 'BLOCKED', 'VERIFY', 'DONE', 'FAILED', 'CANCELLED'] as const;
export type PlanStatus = (typeof PLAN_STATUSES)[number];
export type TaskStatus = (typeof TASK_STATUSES)[number];
export type CheckStatus = 'SUCCESS' | 'FAILED';
export type AgentStatus = 'ACTIVE' | 'IDLE' | 'LEFT';

export type PlanMemberRole = 'LEAD' | 'CONTRIBUTOR';

export interface PlanRecord {
  plan_id: string;
  name: string;
  objective: string;
  lead_agent_id: string;
  status: PlanStatus;
  workspace_path: string;
  artifact: string | null;
  doc_dir: string;
  source_kind: string | null;
  source_key: string | null;
  rfc_path: string | null;
  rfc_revision: string | null;
  created_at: string;
  updated_at: string;
}

export interface PlanMemberRecord {
  agent_id: string;
  role: PlanMemberRole;
  joined_at: string;
}

export interface PlanDocRecord {
  relative_path: string;
  title: string;
  kind: 'PRIMARY' | 'SUPPORTING';
  ordinal: number;
}

export interface PlanDetail extends PlanRecord {
  members: PlanMemberRecord[];
  docs: PlanDocRecord[];
}

export interface CreatePlanParams {
  name: string;
  objective: string;
  leadAgentId: string;
  workspacePath: string;
  docsPath?: string | null;
  artifact?: string | null;
}

export interface JoinPlanParams {
  planId: string;
  agentId: string;
}

export interface RegisterPlanDocParams {
  planId: string;
  agentId: string;
  relativePath: string;
  title: string;
}

export interface TaskClaimRecord {
  task_id: string;
  run_id: string;
  agent_id: string;
  claimed_at: string;
  heartbeat_at: string;
  expires_at: string;
}

export interface PlanTaskRecord {
  task_id: string;
  plan_id: string;
  title: string;
  reasoning: string;
  acceptance_criteria: string;
  status: TaskStatus;
  priority: number;
  created_by: string;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
  run_id: string | null;
  run_status: TaskRunRecord['status'] | null;
  run_updated_at: string | null;
  verification: { agent_id: string; message: string; created_at: string } | null;
  source_step_key: string | null;
  check_command: string | null;
  paths: string[];
  dependencies: string[];
  claim: TaskClaimRecord | null;
}

export interface TaskRunRecord {
  run_id: string;
  task_id: string | null;
  origin: 'TASK' | 'WORK' | 'HOOK';
  agent_id: string;
  session_id: string | null;
  rationale: string;
  test_plan: string;
  context_ref: string | null;
  status: 'PENDING' | 'ACTIVE' | 'SUCCESS' | 'FAILED';
  workspace_path: string | null;
  artifact: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateTaskParams {
  planId: string;
  title: string;
  reasoning: string;
  acceptanceCriteria: string;
  paths: string[];
  createdBy: string;
  priority?: number;
  dependsOn?: string[];
}

export function isPlanStatus(value: unknown): value is PlanStatus {
  return typeof value === 'string' && (PLAN_STATUSES as readonly string[]).includes(value);
}

export function isTaskStatus(value: unknown): value is TaskStatus {
  return typeof value === 'string' && (TASK_STATUSES as readonly string[]).includes(value);
}

export interface PruneResult {
  dryRun: boolean;
  matched: number;
  deleted: number;
  olderThan: string;
}

export interface Plan {
  planId: string;
  title: string;
  goal: string | null;
  status: PlanStatus;
  createdAt: string;
  updatedAt: string;
  sourceKind: string | null;
  sourceKey: string | null;
  rfcPath: string | null;
  rfcRevision: string | null;
}

export interface SourceStep {
  sourceStepKey: string;
  title: string;
  paths?: string[];
  reasoning?: string | null;
  acceptance?: string | null;
  checkCommand?: string | null;
  dependsOnStepKeys?: string[];
  priority?: number;
}

export interface Task {
  taskId: string;
  planId: string;
  title: string;
  filePath: string | null;
  paths: string[];
  reasoning: string | null;
  acceptance: string | null;
  checkCommand: string | null;
  status: TaskStatus;
  priority: number;
  dependencies: string[];
  agentId: string | null;
  claimedAt: string | null;
  leaseExpiresAt: string | null;
  createdAt: string;
  updatedAt: string;
  doneAt: string | null;
  verifiedAt: string | null;
  verifiedBy: string | null;
  verificationMessage: string | null;
  sourceStepKey: string | null;
  /** The active or most recently submitted run that owns this task's completion fence. */
  runId: string | null;
}

export interface PlanGraphResult {
  plan: Plan;
  tasks: Map<string, Task>;
}

export interface Lock {
  filePath: string;
  runId: string;
  agentId: string;
  reason: string;
  acquiredAt: string;
  expiresAt: string | null;
}

export interface WorkPresence {
  filePath: string;
  runId: string;
  agentId: string;
  reason: string;
  startedAt: string;
  updatedAt: string;
  expiresAt: string;
}

export interface HandoffNote {
  handoffId: string;
  agentId: string;
  summary: string;
  files: string[];
  createdAt: string;
  clearedAt: string | null;
}

export interface PendingVerification {
  runId: string;
  taskId: string | null;
  agentId: string;
  testPlan: string;
  rationale: string;
  createdAt: string;
}

export interface CheckAudit {
  ok: boolean;
  pending: PendingVerification[];
  pendingCount: number;
  filters: {
    agentId: string | null;
    planId: string | null;
    minAgeMs: number | null;
  };
}

export interface LockWaitResult {
  ok: boolean;
  lockFree: boolean;
  filePath: string;
  waitedMs: number;
  conflict: Lock | null;
}

export interface MemoryItem {
  memoryId: string;
  label: string;
  text: string;
  tags: string[];
  createdAt: string;
  similarity?: number;
}

export interface AgentRecord {
  agentId: string;
  name: string | null;
  role: string | null;
  status: AgentStatus;
  metadata: Record<string, unknown>;
  createdAt: string;
  lastSeenAt: string;
}

export interface LiteMessage {
  messageId: string;
  fromAgentId: string;
  toAgentId: string | null;
  topic: string | null;
  text: string;
  files: string[];
  createdAt: string;
  readAt: string | null;
}
