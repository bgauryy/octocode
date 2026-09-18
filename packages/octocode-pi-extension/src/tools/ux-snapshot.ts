import type {
  PlanReadModelTaskV1,
  PlanReadModelV1,
} from './plan-read-model.js';
import type { ForegroundActivity, RuntimeState } from './runtime-store.js';
import { runtimeActivityPresentation } from './activity-presentation.js';
import {
  createExecutionState,
  type ExecutionState,
} from './execution-events.js';
import { classifyEvidenceAuthority } from './evidence-authority.js';
import { elapsedSince, formatDurationShort } from '../ui-extras.js';
import { projectUxAgents, type UxAgentV1 } from './ux-agent-projection.js';

export type { UxAgentV1 } from './ux-agent-projection.js';

export type UxPriority = 'P0' | 'P1' | 'P2' | 'P3' | 'P4';
export type UxSeverity = 'info' | 'warning' | 'error';
export type UxProgressMode = 'linear' | 'graph' | 'dynamic' | 'indeterminate';
export type UxVerificationState =
  'unverified' | 'pending' | 'running' | 'passed' | 'failed' | 'waived';

export interface UxSessionV1 {
  phase: RuntimeState['phase'];
  activity: {
    kind: ForegroundActivity['kind'];
    label: string;
    since?: number;
    detail?: string;
  };
  elapsedMs: number;
  contextPressure?: number;
  contextTokens?: number;
  contextWindow?: number;
  observationTime: number;
}

export interface UxPlanV1 {
  id: string;
  phase: PlanReadModelV1['phase'];
  revision?: string;
  total: number;
  displayTotal?: number;
  done: number;
  active: number;
  ready: number;
  blocked: number;
  verifying: number;
  failed: number;
  dynamic: boolean;
  progressMode: UxProgressMode;
  detailRoute: 'plan';
}

export interface UxTaskV1 {
  id: string;
  index: number;
  label: string;
  activeLabel?: string;
  status: PlanReadModelTaskV1['status'];
  dependencies: number[];
  owner?: string;
  verification: UxVerificationState;
  updatedAt: number;
}

export interface UxBackgroundJobV1 {
  id: string;
  title: string;
  status: RuntimeState['backgroundJobs'][number]['status'];
  elapsedMs: number;
  updatedAt: number;
  exitCode?: number | null;
}

export interface UxAttentionV1 {
  id: string;
  kind:
    | 'input'
    | 'authorization'
    | 'runtime_failed'
    | 'plan_blocked'
    | 'agent_blocked'
    | 'agent_failed'
    | 'background_failed'
    | 'messages'
    | 'context_pressure'
    | 'stale_source';
  priority: UxPriority;
  severity: UxSeverity;
  actor: string;
  reason: string;
  requiredAction?: string;
  detailRoute: string;
  createdAt: number;
}

export interface UxProvenanceV1 {
  owner: 'runtime' | 'plan' | 'workers' | 'awareness';
  sequence: number;
  observedAt: number;
  staleAfterMs?: number;
  stale: boolean;
}

export interface UxSnapshotV1 {
  version: 1;
  observedAt: number;
  session: UxSessionV1;
  goal: { text?: string; milestone?: string; nextAction?: string };
  plan?: UxPlanV1;
  tasks: UxTaskV1[];
  agents: UxAgentV1[];
  backgroundJobs: UxBackgroundJobV1[];
  attention: UxAttentionV1[];
  messages: {
    unread: number;
    queued: number;
    latestSender?: string;
    latestSubject?: string;
    detailRoute: '/octocode-inbox';
  };
  provenance: UxProvenanceV1[];
}

export interface UxSnapshotInput {
  now?: number;
  runtime: Pick<
    RuntimeState,
    'generation' | 'phase' | 'activity' | 'context' | 'footer'
  > & { execution?: ExecutionState; backgroundJobs?: RuntimeState['backgroundJobs'] };
  plan?: PlanReadModelV1;
  agents?: Parameters<typeof projectUxAgents>[0];
  goal?: UxSnapshotV1['goal'];
  dynamicPlan?: boolean;
  awareness?: {
    unread: number;
    observedAt: number;
    staleAfterMs: number;
    sequence?: number;
    latestSender?: string;
    latestSubject?: string;
  };
}

const PRIORITY_ORDER: Record<UxPriority, number> = {
  P0: 0,
  P1: 1,
  P2: 2,
  P3: 3,
  P4: 4,
};

function taskVerification(
  task: PlanReadModelTaskV1,
  phase: PlanReadModelV1['phase']
): UxVerificationState {
  // The plan projection contains task state, not check receipts.
  if (task.status === 'done') return 'unverified';
  if (task.status === 'doing' && phase === 'verifying') return 'running';
  return 'pending';
}

function freezeDeep<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const child of Object.values(value as Record<string, unknown>))
      freezeDeep(child);
    Object.freeze(value);
  }
  return value;
}

export function deriveUxSnapshot(input: UxSnapshotInput): UxSnapshotV1 {
  const now = input.now ?? Date.now();
  const activity = input.runtime.activity;
  const execution = input.runtime.execution ?? createExecutionState();
  const presentation = runtimeActivityPresentation({ activity, execution });
  const pending = Object.values(execution.interactions).filter(
    item => item.status === 'waiting'
  );
  const executing = execution.activeToolIds.length > 0 || execution.compacting;
  const activityKind =
    pending.length > 0
      ? 'awaiting_input'
      : executing
        ? 'working'
        : activity.kind;
  const plan = input.plan;
  const dynamic = input.dynamicPlan ?? false;
  const linear = plan?.shape === 'linear';
  const progressMode: UxProgressMode =
    !plan || plan.summary.total === 0
      ? 'indeterminate'
      : dynamic
        ? 'dynamic'
        : linear
          ? 'linear'
          : 'graph';
  const tasks: UxTaskV1[] = (plan?.tasks ?? []).map(task => ({
    id: task.id,
    index: task.index,
    label: task.text,
    ...(task.activeText ? { activeLabel: task.activeText } : {}),
    status: task.status,
    dependencies: [...task.dependsOn],
    verification: taskVerification(task, plan!.phase),
    updatedAt: now,
  }));
  const agents = projectUxAgents(input.agents, now);
  const backgroundJobs: UxBackgroundJobV1[] = (input.runtime.backgroundJobs ?? []).map((job) => ({
    id: job.id,
    title: job.title,
    status: job.status,
    elapsedMs: Math.max(0, (job.endedAt ?? now) - job.startedAt),
    updatedAt: job.updatedAt,
    ...(job.exitCode !== undefined ? { exitCode: job.exitCode } : {}),
  }));

  const contextUsage = input.runtime.footer.usage;
  const contextPressure =
    contextUsage?.tokens !== undefined && contextUsage.contextWindow > 0
      ? Math.max(
          0,
          Math.floor((contextUsage.tokens / contextUsage.contextWindow) * 100)
        )
      : undefined;
  const attention: UxAttentionV1[] = [];
  const addAttention = (item: UxAttentionV1): void => {
    attention.push(item);
  };
  for (const item of pending)
    addAttention({
      id: `execution:${item.id}`,
      kind: item.kind === 'permission' ? 'authorization' : 'input',
      priority: 'P0',
      severity: 'warning',
      actor: 'Octocode',
      reason: item.title,
      requiredAction:
        item.kind === 'permission'
          ? 'Allow or deny in the prompt'
          : 'Answer the pending question',
      detailRoute: 'interaction',
      createdAt: item.startedAt,
    });
  if (pending.length === 0 && activity.kind === 'awaiting_input')
    addAttention({
      id: `input:${activity.planScope}`,
      kind: 'input',
      priority: 'P0',
      severity: 'warning',
      actor: 'Octocode',
      reason: activity.question,
      requiredAction: 'Answer the pending question',
      detailRoute: 'interaction',
      createdAt: activity.since,
    });
  if (activity.kind === 'awaiting_start' || activity.kind === 'reviewing')
    addAttention({
      id: `authorization:${activity.planScope}`,
      kind: 'authorization',
      priority: 'P0',
      severity: 'warning',
      actor: 'Octocode',
      reason: `Review plan${activity.revision ? ` rev ${activity.revision}` : ''}`,
      requiredAction: 'Review plan → Start or Request changes',
      detailRoute: '/configuration',
      createdAt: activity.since,
    });
  for (const interactionId of plan?.pendingInteractionIds ?? [])
    addAttention({
      id: `interaction:${interactionId}`,
      kind: 'input',
      priority: 'P0',
      severity: 'warning',
      actor: 'Octocode',
      reason: `Input needed · ${interactionId}`,
      requiredAction: 'Answer the pending interaction',
      detailRoute: 'interaction',
      createdAt: now,
    });
  if (input.runtime.phase === 'failed' || activity.kind === 'failed')
    addAttention({
      id: 'runtime:failed',
      kind: 'runtime_failed',
      priority: 'P1',
      severity: 'error',
      actor: 'Octocode',
      reason: activity.kind === 'failed' ? activity.label : 'Runtime failed',
      requiredAction: 'Inspect transcript',
      detailRoute: 'transcript',
      createdAt: 'since' in activity ? activity.since : now,
    });
  if (activity.kind === 'blocked' || (plan?.summary.blocked ?? 0) > 0)
    addAttention({
      id: `plan:${plan?.planId ?? 'runtime'}:blocked`,
      kind: 'plan_blocked',
      priority: 'P1',
      severity: 'warning',
      actor: 'Plan',
      reason:
        activity.kind === 'blocked'
          ? activity.label
          : `${plan!.summary.blocked} blocked task${plan!.summary.blocked === 1 ? '' : 's'}`,
      requiredAction: 'Inspect plan',
      detailRoute: 'plan',
      createdAt: 'since' in activity ? activity.since : now,
    });
  for (const agent of agents) {
    if (
      agent.state !== 'blocked' &&
      agent.state !== 'failed' &&
      agent.state !== 'error'
    )
      continue;
    const failed = agent.state === 'failed' || agent.state === 'error';
    addAttention({
      id: `agent:${agent.id}:${agent.state}`,
      kind: failed ? 'agent_failed' : 'agent_blocked',
      priority: 'P2',
      severity: failed ? 'error' : 'warning',
      actor: agent.label,
      reason: agent.assignment ?? agent.activeOperation ?? agent.state,
      requiredAction: 'Open inbox',
      detailRoute: '/octocode-inbox',
      createdAt: agent.updatedAt,
    });
  }
  for (const job of backgroundJobs) {
    if (job.status !== 'failed' && job.status !== 'timed_out') continue;
    addAttention({
      id: `background:${job.id}:${job.status}`,
      kind: 'background_failed',
      priority: 'P1',
      severity: job.status === 'failed' ? 'error' : 'warning',
      actor: 'Background job',
      reason: `${job.title} ${job.status === 'failed' ? `failed${job.exitCode === undefined ? '' : ` with exit ${job.exitCode}`}` : 'timed out'}`,
      requiredAction: 'Inspect background output',
      detailRoute: '/octocode-status',
      createdAt: job.updatedAt,
    });
  }
  const queued = agents.reduce((sum, agent) => sum + agent.pendingMessages, 0);
  const unread = Math.max(0, input.awareness?.unread ?? 0);
  if (queued + unread > 0) {
    // Use the earliest known arrival time so messages sort correctly against
    // other P2 items. Using 'createdAt: now' always stamped them as newest,
    // which displaced more urgent agent-blocked/failed alerts.
    const queuedSince =
      queued > 0
        ? agents
            .filter(a => a.pendingMessages > 0)
            .reduce<number>((min, a) => Math.min(min, a.updatedAt), now)
        : now;
    const unreadSince =
      unread > 0 && input.awareness ? input.awareness.observedAt : now;
    const messagesSince = Math.min(queuedSince, unreadSince);
    addAttention({
      id: 'messages:pending',
      kind: 'messages',
      priority: 'P2',
      severity: 'info',
      actor: 'Messages',
      reason: `${queued + unread} pending`,
      requiredAction: 'Open inbox',
      detailRoute: '/octocode-inbox',
      createdAt: messagesSince,
    });
  }
  if (contextPressure !== undefined && contextPressure >= 90)
    addAttention({
      id: 'context:pressure',
      kind: 'context_pressure',
      priority: 'P3',
      severity: contextPressure >= 97 ? 'error' : 'warning',
      actor: 'Context',
      reason: `${contextPressure}% used`,
      requiredAction: 'Compact or finish current work',
      detailRoute: '/configuration',
      createdAt: now,
    });

  const awarenessAuthority = input.awareness
    ? classifyEvidenceAuthority({
        observedAt: input.awareness.observedAt,
        now,
        staleAfterMs: input.awareness.staleAfterMs,
      })
    : undefined;
  const awarenessStale = awarenessAuthority !== undefined && awarenessAuthority.authority !== 'live';
  if (awarenessStale)
    addAttention({
      id: 'awareness:stale',
      kind: 'stale_source',
      priority: 'P3',
      severity: 'warning',
      actor: 'Awareness',
      reason: `Coordination status last updated ${formatDurationShort(elapsedSince(input.awareness!.observedAt, now))} ago`,
      requiredAction: 'Inspect session details',
      detailRoute: '/octocode-status',
      createdAt: input.awareness!.observedAt,
    });

  attention.sort(
    (a, b) =>
      PRIORITY_ORDER[a.priority] - PRIORITY_ORDER[b.priority] ||
      b.createdAt - a.createdAt ||
      a.id.localeCompare(b.id)
  );

  const snapshot: UxSnapshotV1 = {
    version: 1,
    observedAt: now,
    session: {
      phase: input.runtime.phase,
      activity: {
        kind: activityKind,
        label: presentation.status ?? 'Idle',
        since:
          presentation.since ??
          ('since' in activity ? activity.since : undefined),
      },
      elapsedMs: elapsedSince(input.runtime.footer.sessionStartedAt, now) ?? 0,
      ...(contextPressure !== undefined
        ? {
            contextPressure,
            contextTokens: contextUsage!.tokens,
            contextWindow: contextUsage!.contextWindow,
          }
        : {}),
      observationTime: now,
    },
    goal: { ...(input.goal ?? {}) },
    ...(plan && plan.summary.total > 0
      ? {
          plan: {
            id: plan.planId,
            phase: plan.phase,
            ...(plan.revision ? { revision: plan.revision } : {}),
            total: plan.summary.total,
            ...(progressMode === 'linear'
              ? { displayTotal: plan.summary.total }
              : {}),
            done: plan.summary.done,
            active: plan.summary.running,
            ready: plan.tasks.filter(task => task.status === 'todo').length,
            blocked: plan.summary.blocked,
            verifying: plan.phase === 'verifying' ? plan.summary.running : 0,
            failed:
              plan.phase === 'failed' ? Math.max(1, plan.summary.blocked) : 0,
            dynamic,
            progressMode,
            detailRoute: 'plan',
          },
        }
      : {}),
    tasks,
    agents,
    backgroundJobs,
    attention,
    messages: {
      unread,
      queued,
      ...(input.awareness?.latestSender
        ? { latestSender: input.awareness.latestSender }
        : {}),
      ...(input.awareness?.latestSubject
        ? { latestSubject: input.awareness.latestSubject }
        : {}),
      detailRoute: '/octocode-inbox',
    },
    provenance: [
      {
        owner: 'runtime',
        sequence: input.runtime.generation,
        observedAt: now,
        stale: false,
      },
      ...(plan
        ? [
            {
              owner: 'plan' as const,
              sequence: plan.review.generation,
              observedAt: now,
              stale: false,
            },
          ]
        : []),
      {
        owner: 'workers',
        sequence: agents.reduce(
          (max, agent) => Math.max(max, agent.updatedAt),
          0
        ),
        observedAt: now,
        stale: false,
      },
      ...(input.awareness
        ? [
            {
              owner: 'awareness' as const,
              sequence: input.awareness.sequence ?? input.awareness.observedAt,
              observedAt: input.awareness.observedAt,
              staleAfterMs: input.awareness.staleAfterMs,
              stale: awarenessStale,
            },
          ]
        : []),
    ],
  };
  return freezeDeep(snapshot);
}
