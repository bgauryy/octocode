import type { InlineSegment } from './components.js';
import { formatCompact, formatDurationShort } from '../ui-extras.js';
import type { UxPriority, UxSnapshotV1 } from '../tools/ux-snapshot.js';
import { truncateToWidth } from './width.js';
import { isRecentAgentOutcome } from '../tools/agents/display-state.js';

export type StatusDensity = 'automatic' | 'compact' | 'expanded';

export interface StatusDiagnosticV1 {
  id: string;
  priority: UxPriority;
  segments: readonly InlineSegment[];
}

export interface StatusPolicyOptions {
  width: number;
  height: number;
  density?: StatusDensity;
  diagnostics?: readonly StatusDiagnosticV1[];
}

export interface StatusSelectionV1 {
  rows: InlineSegment[][];
  rowIds: string[];
  maxRows: number;
  omitted: number;
  detailRoutes: string[];
}

interface Candidate {
  id: string;
  priority: UxPriority;
  order: number;
  segments: InlineSegment[];
  detailRoute?: string;
}

const PRIORITY_ORDER: Record<UxPriority, number> = {
  P0: 0,
  P1: 1,
  P2: 2,
  P3: 3,
  P4: 4,
};

function rowBudget(height: number, density: StatusDensity): number {
  const safeHeight = Math.max(
    1,
    Math.floor(Number.isFinite(height) ? height : 40)
  );
  if (density === 'compact') return Math.min(2, safeHeight);
  if (density === 'expanded') return Math.min(10, Math.max(1, safeHeight - 4));
  return Math.min(6, Math.max(1, Math.floor(safeHeight * 0.15)));
}

function plural(count: number, singular: string): string {
  return `${count} ${singular}${count === 1 ? '' : 's'}`;
}

function planSegments(snapshot: UxSnapshotV1): InlineSegment[] | undefined {
  const plan = snapshot.plan;
  if (!plan) return undefined;
  const current =
    snapshot.tasks.find(task => task.status === 'doing') ??
    snapshot.tasks.find(task => task.status === 'todo') ??
    snapshot.tasks.find(task => task.status === 'blocked');
  const taskState = current?.status === 'doing'
    ? plan.phase === 'verifying' ? 'verifying' : plan.phase === 'executing' ? 'running' : 'paused'
    : current?.status === 'todo' ? 'next' : current?.status;
  const currentTask: InlineSegment[] = current
    ? [
        {
          text: `task ${current.index} ${taskState}: ${current.status === 'doing' ? current.activeLabel ?? current.label : current.label}`,
          token: current.status === 'blocked' ? 'warning' : 'muted',
        },
      ]
    : [];
  if (plan.phase !== 'executing' && plan.phase !== 'verifying') {
    return [
      { text: `Plan ${plan.phase.replaceAll('_', ' ')}`, token: plan.phase === 'failed' ? 'error' : plan.phase === 'blocked' ? 'warning' : 'brand' },
      ...(plan.phase === 'complete'
        ? [{ text: `${plan.done}/${plan.displayTotal ?? plan.total} done`, token: 'success' as const }]
        : []),
      ...(plan.phase === 'blocked' || plan.phase === 'failed' ? currentTask : []),
      { text: 'plan', token: 'link' },
    ];
  }
  if (plan.phase === 'verifying' || plan.verifying > 0)
    return [
      { text: 'Plan verifying', token: 'brand' },
      ...currentTask,
      { text: plural(plan.done, 'passed'), token: 'success' },
      ...(plan.verifying > 0
        ? [{ text: plural(plan.verifying, 'running'), token: 'brand' as const }]
        : []),
      ...(plan.blocked > 0
        ? [
            {
              text: plural(plan.blocked, 'pending'),
              token: 'warning' as const,
              attention: true,
            },
          ]
        : []),
      { text: 'plan', token: 'link' },
    ];
  if (plan.progressMode === 'linear' && plan.displayTotal !== undefined) {
    return [
      {
        text: `Plan ${plan.done}/${plan.displayTotal}`,
        token: plan.blocked > 0 ? 'warning' : 'brand',
        attention: plan.blocked > 0,
      },
      ...currentTask,
      { text: 'plan', token: 'link' },
    ];
  }
  return [
    {
      text: 'Plan',
      token: plan.blocked > 0 || plan.failed > 0 ? 'warning' : 'brand',
      attention: plan.blocked > 0 || plan.failed > 0,
    },
    { text: `${plan.done} done`, token: 'success' },
    ...currentTask,
    ...(plan.active > 0
      ? [{ text: `${plan.active} active`, token: 'brand' as const }]
      : []),
    ...(plan.ready > 0
      ? [{ text: `${plan.ready} ready`, token: 'muted' as const }]
      : []),
    ...(plan.blocked > 0
      ? [
          {
            text: `${plan.blocked} waiting`,
            token: 'warning' as const,
          },
        ]
      : []),
    ...(plan.progressMode === 'dynamic'
      ? [{ text: 'scope changing', token: 'warning' as const }]
      : []),
    { text: 'plan', token: 'link' },
  ];
}

function attentionCandidates(snapshot: UxSnapshotV1): Candidate[] {
  return snapshot.attention
    .filter(
      item =>
        item.kind === 'input' ||
        item.kind === 'authorization' ||
        item.kind === 'runtime_failed' ||
        item.kind === 'plan_blocked' ||
        item.kind === 'stale_source'
    )
    .map(item => ({
      id: `attention:${item.id}`,
      priority: item.priority,
      order: -item.createdAt,
      segments: [
        {
          text:
            item.priority === 'P0'
              ? 'Needs you'
              : item.kind === 'runtime_failed'
                ? 'Failed'
                : item.kind === 'plan_blocked'
                  ? 'Blocked'
                  : item.actor,
          token: item.severity === 'error' ? 'error' : 'warning',
          attention: true,
        },
        {
          text: item.reason,
          token: item.severity === 'error' ? 'error' : 'muted',
        },
        ...(item.requiredAction
          ? [{ text: item.requiredAction, token: 'muted' as const }]
          : []),
        { text: item.detailRoute, token: 'link', attention: true },
      ],
      detailRoute: item.detailRoute,
    }));
}

function agentCandidates(snapshot: UxSnapshotV1, width: number): Candidate[] {
  if (snapshot.agents.length === 0) return [];
  const attention = snapshot.agents
    .filter(
      agent =>
        agent.state === 'blocked' ||
        agent.state === 'failed' ||
        agent.state === 'error'
    )
    .sort((a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id))
    .map((agent, index): Candidate => ({
      id: `agent:${agent.id}:${agent.state}`,
      priority: 'P1',
      order: -1_000 + index,
      segments: [
        {
          text: agent.state,
          token: agent.state === 'blocked' ? 'warning' : 'error',
          attention: true,
          keepWhole: true,
        },
        { text: agent.label, token: 'muted', attention: true },
        { text: '/octocode-inbox', token: 'link', attention: true },
        ...(agent.activeOperation ?? agent.assignment
          ? [{ text: agent.activeOperation ?? agent.assignment!, token: 'muted' as const }]
          : []),
      ],
      detailRoute: '/octocode-inbox',
    }));
  // Stable roster order prevents streaming updates from shuffling rows. Settled
  // workers remain in the inbox; ambient space belongs to live work and failures.
  const live = snapshot.agents
    .filter(agent => ['starting', 'queued', 'running', 'idle'].includes(agent.state))
    .sort((a, b) => a.id.localeCompare(b.id))
    .map((agent, index): Candidate => ({
      id: `worker:${agent.id}`,
      priority: 'P2',
      order: 1_000 + index,
      segments: [
        { text: agent.state, token: agent.state === 'idle' ? 'muted' : 'brand', keepWhole: true },
        { text: truncateToWidth(agent.label, Math.max(4, Math.min(24, Math.floor(width / 4)))), token: 'brand', attention: true },
        ...(agent.activeOperation ?? agent.assignment
          ? [{ text: agent.activeOperation ?? agent.assignment!, token: 'muted' as const }]
          : []),
        ...(agent.elapsedMs !== undefined ? [{ text: formatDurationShort(agent.elapsedMs), token: 'dim' as const }] : []),
        { text: '/octocode-inbox', token: 'link' },
      ],
      detailRoute: '/octocode-inbox',
    }));
  const recentOutcomes = snapshot.agents
    .filter(agent => isRecentAgentOutcome(agent, snapshot.observedAt))
    .sort((a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id))
    .map((agent, index): Candidate => ({
      id: `worker-outcome:${agent.id}`,
      priority: 'P3',
      order: 1_500 + index,
      segments: [
        {
          text: agent.state,
          token: agent.state === 'done' ? 'success' : 'muted',
          keepWhole: true,
        },
        {
          text: truncateToWidth(
            agent.label,
            Math.max(4, Math.min(24, Math.floor(width / 4))),
          ),
          token: 'brand',
        },
        ...(agent.activeOperation ?? agent.assignment
          ? [{ text: agent.activeOperation ?? agent.assignment!, token: 'muted' as const }]
          : []),
        { text: '/octocode-inbox', token: 'link' },
      ],
      detailRoute: '/octocode-inbox',
    }));
  return [...attention, ...live, ...recentOutcomes];
}

function candidates(
  snapshot: UxSnapshotV1,
  diagnostics: readonly StatusDiagnosticV1[],
  width: number,
): Candidate[] {
  const result: Candidate[] = attentionCandidates(snapshot);
  const activityAttention =
    snapshot.session.activity.kind === 'awaiting_input' ||
    snapshot.session.activity.kind === 'awaiting_start' ||
    snapshot.session.activity.kind === 'reviewing' ||
    snapshot.session.activity.kind === 'blocked' ||
    snapshot.session.activity.kind === 'failed';
  if (!activityAttention && snapshot.session.activity.kind !== 'idle') {
    const elapsed =
      snapshot.session.activity.since !== undefined &&
      snapshot.session.activity.kind !== 'complete'
        ? Math.max(0, snapshot.observedAt - snapshot.session.activity.since)
        : undefined;
    result.push({
      id: 'session:activity',
      priority: 'P1',
      order: 0,
      segments: [
        { text: snapshot.session.activity.label, token: 'brand' },
        ...(elapsed !== undefined
          ? [{ text: formatDurationShort(elapsed), token: 'dim' as const }]
          : []),
      ],
    });
  }
  const plan = planSegments(snapshot);
  if (plan)
    result.push({
      id: 'plan:progress',
      priority: 'P1',
      order: 10,
      segments: plan,
      detailRoute: 'plan',
    });
  result.push(...agentCandidates(snapshot, width));
  if (snapshot.messages.unread + snapshot.messages.queued > 0)
    result.push({
      id: 'messages:summary',
      priority: 'P2',
      order: 2_000,
      segments: [
        {
          text: `${snapshot.messages.unread + snapshot.messages.queued} messages pending`,
          token: 'link',
          attention: true,
        },
        ...(snapshot.messages.latestSender
          ? [
              {
                text: `latest from ${snapshot.messages.latestSender}`,
                token: 'muted' as const,
              },
            ]
          : []),
        { text: snapshot.messages.detailRoute, token: 'link', attention: true },
      ],
      detailRoute: snapshot.messages.detailRoute,
    });
  if (snapshot.session.contextPressure !== undefined)
    result.push({
      id: 'context:pressure',
      priority: 'P3',
      order: 0,
      segments: [
        {
          text: `ctx ${snapshot.session.contextTokens !== undefined && snapshot.session.contextWindow ? `${formatCompact(snapshot.session.contextTokens)}/${formatCompact(snapshot.session.contextWindow)} ` : ''}${snapshot.session.contextPressure}%`,
          token:
            snapshot.session.contextPressure >= 97
              ? 'error'
              : snapshot.session.contextPressure >= 90
                ? 'warning'
                : 'dim',
          attention: snapshot.session.contextPressure >= 90,
        },
      ],
      detailRoute: '/configuration',
    });
  for (const diagnostic of diagnostics)
    result.push({
      id: `diagnostic:${diagnostic.id}`,
      priority: diagnostic.priority,
      order: diagnostic.id === 'session' ? -1 : 0,
      segments: diagnostic.segments.map(segment => ({ ...segment })),
    });
  return result.sort(
    (a, b) =>
      PRIORITY_ORDER[a.priority] - PRIORITY_ORDER[b.priority] ||
      a.order - b.order ||
      a.id.localeCompare(b.id)
  );
}

function compactCandidates(all: Candidate[]): Candidate[] {
  const activity = all.find(candidate => candidate.id === 'session:activity');
  const plan = all.find(candidate => candidate.id === 'plan:progress');
  const agentAttention = all.filter(candidate =>
    candidate.id.startsWith('agent:')
  );
  const removed = new Set<string>();
  const grouped: Candidate[] = [];
  if (activity && plan) {
    removed.add(activity.id);
    removed.add(plan.id);
    grouped.push({
      id: 'outcome:current',
      priority: 'P1',
      order: 0,
      detailRoute: 'plan',
      segments: [
        ...plan.segments,
        ...activity.segments.filter(segment => !segment.text.includes('task ')),
      ],
    });
  }
  if (agentAttention.length > 1) {
    for (const candidate of agentAttention) removed.add(candidate.id);
    const MAX_NAMED = 2;
    const visibleAttention = agentAttention.slice(0, MAX_NAMED);
    const overflow = agentAttention.length - MAX_NAMED;
    grouped.push({
      id: 'agents:attention',
      priority: 'P1',
      order: -1_000,
      detailRoute: '/octocode-inbox',
      segments: [
        ...visibleAttention.flatMap(candidate =>
          candidate.segments.slice(0, 2)
        ),
        ...(overflow > 0
          ? [
              {
                text: `+${overflow}`,
                token: 'warning' as const,
                attention: true,
              },
            ]
          : []),
        { text: '/octocode-inbox', token: 'link', attention: true },
      ],
    });
  }
  return [
    ...all.filter(candidate => !removed.has(candidate.id)),
    ...grouped,
  ].sort(
    (a, b) =>
      PRIORITY_ORDER[a.priority] - PRIORITY_ORDER[b.priority] ||
      a.order - b.order ||
      a.id.localeCompare(b.id)
  );
}

export function selectStatusRows(
  snapshot: UxSnapshotV1,
  options: StatusPolicyOptions
): StatusSelectionV1 {
  const density = options.density ?? 'automatic';
  const maxRows = rowBudget(options.height, density);
  const all = candidates(snapshot, options.diagnostics ?? [], options.width);
  const projected = density === 'compact' || (maxRows <= 3 && snapshot.agents.length > 0)
    ? compactCandidates(all)
    : all;
  // Diagnostics use spare space; reserving a metadata row can hide a blocker
  // even when the user explicitly chose a compact footer.
  const selected = projected.slice(0, maxRows);
  const omittedCandidates = projected.filter(
    candidate => !selected.includes(candidate)
  );
  const omitted = omittedCandidates.length;
  const detailRoutes = [
    ...new Set(
      omittedCandidates.flatMap(candidate =>
        candidate.detailRoute ? [candidate.detailRoute] : []
      )
    ),
  ];
  const rows = selected.map(candidate => candidate.segments);
  const omittedWorkers = omittedCandidates.filter(candidate => candidate.id.startsWith('worker:'));
  const rosterRow = selected.map(candidate => candidate.id.startsWith('worker:')
    || candidate.id.startsWith('agent:') || candidate.id === 'agents:attention').lastIndexOf(true);
  if (omittedWorkers.length > 0 && rosterRow >= 0) {
    rows[rosterRow] = [
      ...rows[rosterRow]!,
      { text: `+${plural(omittedWorkers.length, 'agent')}`, token: 'muted', attention: true },
    ];
  }
  const omittedDecisions = omittedCandidates.filter(
    candidate => candidate.priority === 'P0'
  );
  if (omittedDecisions.length > 0 && rows[0]) {
    rows[0] = [
      ...rows[0],
      {
        text: `+${omittedDecisions.length} needs you`,
        token: 'warning',
        attention: true,
      },
    ];
    if (!detailRoutes.includes('/octocode-status'))
      detailRoutes.push('/octocode-status');
  }
  if (omitted > 0 && selected[0]?.priority !== 'P0') {
    const urgentContext = omittedCandidates.find(
      candidate =>
        candidate.id === 'context:pressure' &&
        candidate.segments.some(segment => segment.attention)
    );
    const decisionRelevant = omittedCandidates.filter(
      candidate =>
        PRIORITY_ORDER[candidate.priority] <= PRIORITY_ORDER.P2 &&
        candidate !== urgentContext &&
        !(rosterRow >= 0 && candidate.id.startsWith('worker:'))
    );
    const route =
      urgentContext?.detailRoute ?? decisionRelevant[0]?.detailRoute;
    const overflow: InlineSegment[] = [
      ...(urgentContext ? urgentContext.segments : []),
      ...(route
        ? [{ text: route, token: 'link' as const, attention: true }]
        : []),
      ...(decisionRelevant.length > 0
        ? [{ text: decisionRelevant.every(candidate => candidate.id.startsWith('worker:'))
          ? `+${decisionRelevant.length} agents` : `${decisionRelevant.length} more`, token: 'muted' as const }]
        : []),
    ];
    // Never put an overflow route ahead of the state that earned this row.
    // Urgent context may lead ordinary activity, but attention keeps its place.
    rows[0] = urgentContext && !rows[0]?.[0]?.attention
      ? [...overflow, ...rows[0]!]
      : [...rows[0]!, ...overflow];
  }
  return {
    rows,
    rowIds: selected.map(candidate => candidate.id),
    maxRows,
    omitted,
    detailRoutes,
  };
}
