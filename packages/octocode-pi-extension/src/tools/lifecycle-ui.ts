import type { PiContext, PiInstance } from '../types.js';
import { runtimeStoreFor } from './runtime-renderer.js';
import { emitExecution } from './execution-runtime.js';
import {
  executionLabel,
  executionQueries,
  executionResultDetails,
  executionResultSummary,
  executionToolTitle,
  executionUsage,
  record,
} from './execution-presentation.js';

/** Observe host facts once. Pi owns messages/results; semantic events reference them. */
export function registerLifecycleUi(
  pi: PiInstance,
  repaint: (ctx?: PiContext) => void
): void {
  const calls = new WeakMap<
    object,
    Map<string, { tool: string; queries: Record<string, unknown>[] }>
  >();
  const warnedTools = new WeakMap<object, Set<string>>();
  const refresh = (ctx: PiContext): void => {
    try {
      repaint(ctx);
    } catch {
      /* replaced UI cannot interrupt execution */
    }
  };
  pi.on('session_start', async (_event, ctx) => {
    const store = runtimeStoreFor(ctx);
    if (store) { calls.delete(store); warnedTools.delete(store); }
    refresh(ctx);
  });
  pi.on('turn_start', async (_event, ctx) => {
    emitExecution(ctx, 'turn.started', {});
    refresh(ctx);
  });
  pi.on('tool_execution_start', async (event, ctx) => {
    const store = runtimeStoreFor(ctx);
    if (!event.toolCallId || !store) return;
    const tool = executionLabel(event.toolName || 'tool', 64);
    const input = calls.get(store) ?? new Map();
    calls.set(store, input);
    input.set(event.toolCallId, {
      tool,
      queries: executionQueries(event.args),
    });
    emitExecution(ctx, 'tool.started', {
      toolCallId: event.toolCallId,
      tool,
      title: executionToolTitle(tool, event.args),
    });
    refresh(ctx);
  });
  pi.on('tool_execution_end', async (event, ctx) => {
    const store = runtimeStoreFor(ctx);
    if (!store) return;
    const stored = calls.get(store)?.get(event.toolCallId);
    const tool =
      runtimeStoreFor(ctx)?.getState().execution.tools[event.toolCallId];
    if (!tool || tool.status !== 'running') return;
    calls.get(store)?.delete(event.toolCallId);
    const failed = event.isError || record(event.result).isError === true;
    emitExecution(
      ctx,
      failed ? 'tool.failed' : 'tool.completed',
      {
        toolCallId: event.toolCallId,
        summary: executionResultSummary(event.result, failed),
        outputRef: { kind: 'tool-result', toolCallId: event.toolCallId },
      },
      'transcript'
    );
    for (const row of executionResultDetails(event.result)) {
      const q = stored?.queries[row.index];
      if (
        tool.tool === 'skill' &&
        q?.action !== 'list' &&
        q?.type !== 'call' &&
        typeof q?.name === 'string'
      ) {
        emitExecution(
          ctx,
          row.failed ||
            (failed &&
              !Array.isArray(record(record(event.result).details).results))
            ? 'skill.failed'
            : 'skill.activated',
          {
            name: executionLabel(row.details.name ?? q.name),
            source: executionLabel(row.details.dir) || undefined,
          },
          'transcript'
        );
      }
      if (
        tool.tool === 'file' &&
        !row.failed &&
        (!failed ||
          Array.isArray(record(record(event.result).details).results)) &&
        typeof row.details.path === 'string'
      ) {
        emitExecution(
          ctx,
          'file.changed',
          {
            path: row.details.path,
            operation:
              row.details.operation === 'delete'
                ? 'delete'
                : row.details.created === true
                  ? 'create'
                  : 'modify',
          },
          'transcript'
        );
      }
    }
    if (failed && ctx.hasUI) {
      const warned = warnedTools.get(store) ?? new Set<string>();
      warnedTools.set(store, warned);
      if (!warned.has(tool.tool)) {
        warned.add(tool.tool);
        try {
          ctx.ui?.notify?.(
            `${tool.title} failed. See its result for details.`,
            'warning'
          );
        } catch {
          /* UI only */
        }
      }
    }
    refresh(ctx);
  });
  pi.on('message_start', async (event, ctx) => {
    const message = record(event.message);
    const role = message.role;
    if (role !== 'user' && role !== 'assistant') return;
    const timestamp =
      typeof message.timestamp === 'number' ? message.timestamp : Date.now();
    const messageId = `${role}:${timestamp}`;
    if (role === 'user')
      emitExecution(
        ctx,
        'user.message',
        { messageId, outputRef: { kind: 'message', role, timestamp } },
        'transcript'
      );
    else emitExecution(ctx, 'assistant.started', { messageId });
  });
  pi.on('message_end', async (event, ctx) => {
    const message = record(event.message);
    if (message.role !== 'assistant') return;
    const timestamp =
      typeof message.timestamp === 'number' ? message.timestamp : Date.now();
    emitExecution(
      ctx,
      'assistant.completed',
      {
        messageId: `assistant:${timestamp}`,
        status:
          message.stopReason === 'aborted'
            ? 'interrupted'
            : message.stopReason === 'error'
              ? 'failed'
              : 'completed',
        outputRef: { kind: 'message', role: 'assistant', timestamp },
      },
      'transcript'
    );
  });
  pi.on('turn_end', async (event, ctx) => {
    emitExecution(
      ctx,
      'turn.completed',
      {
        status:
          ctx.signal?.aborted || event?.message?.stopReason === 'aborted'
            ? 'cancelled'
            : event?.message?.stopReason === 'error'
              ? 'failed'
              : 'completed',
        usage: executionUsage(event?.message?.usage),
      },
      'debug'
    );
    const store = runtimeStoreFor(ctx);
    if (store) { calls.delete(store); warnedTools.delete(store); }
    refresh(ctx);
  });
  pi.on('agent_settled', async (_event, ctx) => {
    const state = runtimeStoreFor(ctx)?.getState();
    if (
      state &&
      ['thinking', 'researching', 'planning', 'working', 'verifying'].includes(
        state.activity.kind
      )
    )
      state.setActivity({ kind: 'idle' });
    refresh(ctx);
  });
  pi.on('model_select', async (event, ctx) => {
    emitExecution(
      ctx,
      'session.updated',
      { model: event.model.id, provider: event.model.provider },
      'debug'
    );
    refresh(ctx);
  });
  pi.on('session_before_compact', async (_event, ctx) => {
    emitExecution(ctx, 'context.started', {});
    refresh(ctx);
  });
  pi.on('session_compact', async (event, ctx) => {
    const entry = record(event.compactionEntry);
    emitExecution(
      ctx,
      'context.compacted',
      {
        tokensBefore:
          typeof entry.tokensBefore === 'number'
            ? entry.tokensBefore
            : undefined,
      },
      'transcript'
    );
    refresh(ctx);
  });
  pi.on('session_compact_failed', async (event, ctx) => {
    emitExecution(ctx, 'context.failed', { cancelled: event.aborted });
    refresh(ctx);
  });
}
