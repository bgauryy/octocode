import type { PiInstance } from '../types.js';
import { EXTENSION_COMMANDS } from '../commands.js';
import { runtimeStoreFor } from '../tools/runtime-renderer.js';
import {
  expireExecutionInteractions,
  readExecutionEvents,
} from '../tools/execution-runtime.js';
import { serializeExecutionEvents } from '../tools/execution-event-io.js';
import type {
  ExecutionEvent,
  ExecutionState,
} from '../tools/execution-events.js';
import { executionLabel } from '../tools/execution-presentation.js';
import { elapsedSince, formatCompact, formatDurationShort } from '../ui-extras.js';
import { githubAuthLabel } from '../tools/github-auth-status.js';
import { formatAwarenessPanel, getAwarenessStatusHealth, getCachedAwarenessStatus } from '../tools/awareness-status.js';
import { openScrollInspector } from './scroll-inspector.js';

export function executionStatusLines(
  state: ExecutionState,
  now = Date.now()
): string[] {
  const usage = Object.entries(state.usage).map(
    ([key, value]) =>
      `  ${key} ${value === undefined ? 'unknown' : key === 'cost' ? `$${value.toFixed(6)}` : formatCompact(value)}`
  );
  const section = (title: string, lines: string[]): string[] =>
    lines.length ? ['', title, ...lines] : [];
  return [
    'Session',
    `  ${state.sessionId ?? 'starting'}`,
    `  ${executionLabel(state.cwd)}`,
    `  model ${[state.provider, state.model].filter(Boolean).join('/') || 'unknown'}`,
    `  elapsed ${formatDurationShort(elapsedSince(state.startedAt, now))} · turns ${state.completedTurns} · tools ${state.toolCount}`,
    '',
    'Usage · session totals',
    ...(usage.length ? usage : ['  Provider usage unavailable']),
    ...section(
      'Needs you',
      Object.values(state.interactions)
        .filter(item => item.status === 'waiting')
        .map(item => `  ${item.kind} · ${executionLabel(item.title)}`)
    ),
    ...section(
      'Tools',
      Object.values(state.tools).map(
        tool =>
          `  ${tool.status} · ${executionLabel(tool.title)} · ${formatDurationShort(tool.durationMs ?? now - tool.startedAt)}${tool.summary ? ` · ${executionLabel(tool.summary)}` : ''}`
      )
    ),
    ...section(
      'Skills',
      Object.values(state.skills).map(
        skill =>
          `  ${skill.status} · ${executionLabel(skill.name)}${skill.source ? ` · ${executionLabel(skill.source)}` : ''}`
      )
    ),
    ...section(
      'Plan',
      state.plan
        ? [
            `  ${state.plan.phase}`,
            ...state.plan.tasks.map(
              task => `  ${task.status} · ${executionLabel(task.title)}`
            ),
          ]
        : []
    ),
    ...section(
      'Agents',
      Object.values(state.agents).map(
        agent =>
          `  ${executionLabel(agent.name)} · ${agent.status} · parent ${agent.parentRunId}${agent.task ? ` · ${executionLabel(agent.task)}` : ''}`
      )
    ),
    ...section(
      'File operations',
      Object.values(state.files).map(
        file => `  ${file.operation} · ${executionLabel(file.path)}`
      )
    ),
    '',
    'Messages and full tool output remain in the Pi transcript. Ctrl+O expands tool results.',
  ];
}

/** Explain cached coordination evidence without presenting a repaint as a read. */
export function awarenessDetailLines(cwd: string, now = Date.now()): string[] {
  const status = getCachedAwarenessStatus(cwd);
  const health = getAwarenessStatusHealth(cwd);
  return [
    '', 'Awareness · shared work, verification checks, and messages',
    `  Source: ${health.state}`,
    ...(status ? [
      `  Last updated ${formatDurationShort(elapsedSince(status.observedAt, now))} ago`,
      ...formatAwarenessPanel(status),
    ] : ['  No coordination snapshot available.']),
    '  /octocode-inbox opens agent messages; /configuration opens plan details.',
  ];
}

export function registerRuntimeInspectors(pi: PiInstance): void {
  pi.registerCommand?.(EXTENSION_COMMANDS.status.name, {
    description: EXTENSION_COMMANDS.status.description,
    getArgumentCompletions: prefix =>
      ['events', 'export']
        .filter(value => value.startsWith(prefix))
        .map(value => ({ value, label: value })),
    handler: async (args, ctx) => {
      expireExecutionInteractions(ctx);
      const action = args.trim();
      if (!action) {
        const runtime = runtimeStoreFor(ctx)?.getState();
        await openScrollInspector(
          ctx,
          'Octocode status',
          runtime ? [
            ...executionStatusLines(runtime.execution),
            '', githubAuthLabel(runtime.footer.githubAuth.status),
            ...awarenessDetailLines(ctx.cwd ?? process.cwd()),
          ] : ['Session is initializing.']
        );
        return;
      }
      if (action !== 'events' && action !== 'export') {
        ctx.ui?.notify?.(
          'Use /octocode-status, /octocode-status events, or /octocode-status export.',
          'warning'
        );
        return;
      }
      let events: ExecutionEvent[];
      try {
        events = readExecutionEvents(ctx);
      } catch (error) {
        ctx.ui?.notify?.(
          error instanceof Error ? error.message : String(error),
          'error'
        );
        return;
      }
      if (action === 'export') {
        try {
          const { createSessionArtifactContext } =
            await import('../tools/session-artifacts.js');
          const artifacts = createSessionArtifactContext(ctx);
          artifacts.writeText(
            'execution-events.jsonl',
            serializeExecutionEvents(events)
          );
          ctx.ui?.notify?.(
            `Event journal exported: ${artifacts.resolve('execution-events.jsonl')}`,
            'info'
          );
        } catch (error) {
          ctx.ui?.notify?.(
            `Could not export event journal: ${error instanceof Error ? error.message : String(error)}`,
            'error'
          );
        }
        return;
      }
      await openScrollInspector(
        ctx,
        'Execution events · JSONL',
        events.length
          ? serializeExecutionEvents(events).trimEnd().split('\n')
          : ['No recorded events in this branch.']
      );
    },
  });
}
