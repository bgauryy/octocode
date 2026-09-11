/**
 * Agent type contracts: all public agent types, interfaces, and spawn-policy constants
 * shared across the agents sub-modules and the agent-tools orchestration layer.
 */
import type { SpawnPolicy } from '../../types.js';
import type { WorktreeIsolation } from '../worktree.js';
import type { WorkerAwarenessInspection } from '../awareness-worker-audit.js';
import type { InternalWorktreeState } from '../worktree.js';
import type { WorkerLedgerEvent, WorkerMessageActivity } from '../../types.js';
import type { WorkerCapabilityGrant, WorkerCapabilitySelection } from '@octocodeai/agent-contracts/capabilities';

export type AgentStatus = 'starting' | 'running' | 'idle' | 'exited' | 'failed' | 'killed';
export type ResourceMode = 'lean' | 'octocode' | 'default';
export type NormalizedWorkerStatus = 'done' | 'blocked' | 'failed' | 'unknown';
export type NormalizedWorkerConfidence = 'confirmed' | 'likely' | 'uncertain';

export type WorktreeDecision = 'shared' | 'create';

export interface NormalizedWorkerResult {
  status: NormalizedWorkerStatus;
  result?: string;
  evidence: string[];
  verification?: string;
  confidence: NormalizedWorkerConfidence;
  next?: string;
  artifact?: string;
  rawPrefixes: Record<string, string[]>;
}

export interface WorkerRecoveryRisk {
  warnings: string[];
  statusOrActionCount: number;
  evidenceCount: number;
  hasVerification: boolean;
}

export const REQUIRED_PACKET_SECTIONS = ['goal', 'context', 'scope', 'ownership', 'acceptance', 'return'];

export interface SpawnAgentParams {
  task?: string;
  context?: string;
  name?: string;
  cwd?: string;
  model?: string;
  /** Optional current plan step, shown in the parent agent ledger/footer. */
  planStep?: string;
  /** Internal canonical assignment identity, scoped to the parent session. */
  planId?: string;
  planScope?: string;
  /** Optional parent-assigned grouping for bounded fleet summaries. */
  cohortId?: string;
  provider?: string;
  thinking?: string;
  tools?: string[];
  /** Exact parent-approved identities; omitted fields use the focused role default. */
  capabilities?: WorkerCapabilitySelection;
  capabilitySnapshotRevision?: string;
  /** Internal role selector for default resource grants. */
  capabilityProfile?: string;
  systemPrompt?: string;
  resourceMode?: ResourceMode;
  noSession?: boolean;
  isolation?: WorktreeIsolation;
  includeUncommitted?: boolean;
  /** Internal: set only after the user-decision gate approves worktree creation or explicitly chooses shared cwd. */
  worktreeDecision?: WorktreeDecision;
  /**
   * Absolute paths to skill directories to load via --skill (additive, works with --no-skills).
   * Typed agent profiles resolve these directories internally from the skill registry.
   */
  skills?: string[];
}

/** Maximum number of simultaneously active (non-droppable) agent records. Hard limit enforced on spawn. */
export const MAX_AGENT_RECORDS = 50;
/** Cross-host root fan-out ceiling. Children never receive the agent facade. */
export const MAX_ACTIVE_AGENTS = 4;
export const DEFAULT_SPAWN_POLICY: SpawnPolicy = {
  maxActiveAgents: MAX_ACTIVE_AGENTS,
  warningActiveAgents: MAX_ACTIVE_AGENTS - 1,
  requiredPacketSections: REQUIRED_PACKET_SECTIONS,
  maxStepsPerWorker: 60,
};

// ─── Process / Spawn types ────────────────────────────────────────────────────

export interface AgentProcess {
  stdin: { write(data: string): unknown; end?(): unknown };
  stdout: { on: (event: string, cb: (chunk: Buffer | string) => void) => void };
  stderr: { on: (event: string, cb: (chunk: Buffer | string) => void) => void };
  on: (event: string, cb: (...args: unknown[]) => void) => void;
  kill(signal?: NodeJS.Signals): boolean;
  killed?: boolean;
  /** null while running; a number once the process exited normally. */
  exitCode?: number | null;
  /** null while running; the signal name if the process was killed by a signal. */
  signalCode?: NodeJS.Signals | null;
}

export interface SpawnOptions {
  cwd?: string;
  shell?: boolean;
  stdio?: Array<'ignore' | 'pipe'>;
  env?: NodeJS.ProcessEnv;
}

export type AgentProcessFactory = (command: string, args: string[], options: SpawnOptions) => AgentProcess;

export interface AgentToolCall {
  toolCallId?: string;
  toolName: string;
  status: 'running' | 'done' | 'error';
  startedAt: number;
  finishedAt?: number;
  isError?: boolean;
}

export interface AgentRecord {
  id: string;
  name: string;
  cwd: string;
  command: string;
  args: string[];
  task: string;
  planStep?: string;
  planId?: string;
  planScope?: string;
  cohortId?: string;
  process: AgentProcess;
  status: AgentStatus;
  awarenessInspection?: WorkerAwarenessInspection;
  startedAt: number;
  updatedAt: number;
  exitCode?: number;
  signal?: string;
  error?: string;
  stderr: string;
  events: unknown[];
  messages: unknown[];
  responses: unknown[];
  toolCalls: AgentToolCall[];
  lastOutput: string;
  /** Rolling 1-line progress note (latest structured/progress line) shown live while the worker runs. */
  deltaSummary?: string;
  /** Durable markdown handback path assigned by the parent and safe to inspect after kill/remove. */
  handbackPath: string;
  /**
   * Count of messages queued to the worker (followUp / streaming send / idle steer)
   * that the worker has not yet begun a turn for. Cleared when the worker emits
   * agent_start. Keeps `wait` blocking and drives the 'queued' display state so the
   * ledger never shows 'running' before the turn actually starts.
   */
  pendingMessages: number;
  /** Latest directional parent↔worker communication for the footer ledger. */
  lastMessage?: WorkerMessageActivity;
  /** Consecutive inbound-message dedupe; cleared when the next worker turn starts. */
  lastInboundMessageFingerprint?: string;
  normalizedResult?: NormalizedWorkerResult;
  recoveryRisk: WorkerRecoveryRisk;
  ledgerEvents: WorkerLedgerEvent[];
  policyWarnings: string[];
  promptFiles: string[];
  waiters: Set<() => void>;
  /**
   * Heartbeat subscribers. Notified on every inbound RPC event (via touch) so a
   * blocking `wait` can reset its silence watchdog instead of enforcing a rigid
   * wall-clock deadline: as long as the worker streams events, the wait keeps going.
   */
  activityListeners: Set<() => void>;
  /**
   * In-flight liveness probes: request id → resolver. When the parent sends a
   * `get_state` probe during a silence gap, the child's correlated `response`
   * event resolves the matching entry, proving the worker is alive-but-quiet
   * rather than hung.
   */
  pendingProbes: Map<string, () => void>;
  nextRequestId: number;
  worktree?: InternalWorktreeState;
  /** Stable Awareness id used to register this worker in the shared agent list. */
  awarenessAgentId?: string;
  /** Physical checkout used for this worker's Awareness file/lock ownership. */
  awarenessWorkspace?: string;
  /** Explicit parent Awareness database when a worktree has repository-scoped storage. */
  awarenessDatabase?: string;
  /** Owning host lifecycle state; prevents duplicate leave receipts on kill + close. */
  awarenessPresence?: 'joined' | 'left';
  /** Public grant metadata only; broker authentication is never stored here. */
  capabilityGrant?: WorkerCapabilityGrant;
}

// ─── Display / UI types ───────────────────────────────────────────────────────

export type AgentDisplayState = 'starting' | 'queued' | 'running' | 'idle' | 'done' | 'blocked' | 'failed' | 'killed';

export type AgentDisplaySource = {
  status?: string;
  pendingMessages?: number;
  normalizedResult?: { status?: string; result?: string; next?: string; confidence?: string; verification?: string };
};

// ─── Wait types ───────────────────────────────────────────────────────────────

/** Why a `wait` returned. `terminal` = turn finished; `idle` = quiet gap the watchdog surfaced; `cap` = the optional absolute backstop fired. */
export type WaitReason = 'terminal' | 'idle' | 'cap';

export interface WaitOutcome {
  reason: WaitReason;
  /** True when the worker is not terminal — the turn is still in flight. */
  stillRunning: boolean;
  /** For reason:'idle' — whether a liveness probe got a reply (alive-but-quiet vs hung). */
  probedAlive: boolean;
}

export interface WaitOptions {
  /** Cancel this wait without interrupting or removing the worker. */
  signal?: AbortSignal;
  /**
   * Maximum silence before the wait resolves as 'idle'.
   * Defaults to DEFAULT_WAIT_MAX_SILENCE_MS. Set to 0 to poll once and return.
   */
  maxSilenceMs?: number;
  /**
   * Hard upper bound on total wait time across all silence windows.
   * Unset means no cap (the wait continues until terminal or silence).
   */
  absoluteCapMs?: number;
  /** Whether to probe the worker with a get_state ping on silence. */
  probe?: boolean;
  /** How long to wait for a probe reply before treating silence as a hang. */
  probeGraceMs?: number;
}

// ─── Constants ────────────────────────────────────────────────────────────────

export const MAX_STORED_EVENTS = 200;
export const MAX_LEDGER_EVENTS = 80;
export const MAX_AGENT_VIEW_CHARS = 12_000;
export const HANDBACK_ARTIFACT_FILENAME = 'handback.md';
export const SUBAGENT_ENV_VAR = 'OCTOCODE_PI_SUBAGENT';
export const AWARENESS_AGENT_ENV_VAR = 'OCTOCODE_AGENT_ID';
export const EXIT_SIGNALS: NodeJS.Signals[] = ['SIGTERM', 'SIGHUP', 'SIGINT'];
