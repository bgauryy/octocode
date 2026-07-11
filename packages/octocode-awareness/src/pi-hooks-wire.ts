import { auditUnverified } from './verify.js';
import { artifactFrom, defaultGetDb, getPiAwarenessAgentId, getPiAwarenessSessionId, notify, PiAwarenessBridgeOptions, PiLikeApi, PiLikeContext, PiToolEvent } from './pi-hooks-inputs.js';
import { createPiAwarenessBridge } from './pi-hooks-bridge.js';
import { finalizeActivePiFallbackRuns } from './pi-hooks-guard.js';

export function wirePiAwarenessHooks(pi: PiLikeApi, options: PiAwarenessBridgeOptions = {}) {
  if (!pi?.on) return null;
  const bridge = createPiAwarenessBridge(options);
  const verifyReminderKeys = new Set<string>();

  pi.on('tool_call', async (event, ctx) => bridge.handleToolCall(event as PiToolEvent, ctx));
  pi.on('tool_result', async (event, ctx) => bridge.handleToolResult(event as PiToolEvent, ctx));
  pi.on('tool_execution_start', async (event, ctx) => bridge.handleToolCall({
    toolCallId: String(event?.toolCallId ?? ''),
    toolName: String(event?.toolName ?? ''),
    input: event?.args,
  }, ctx));
  pi.on('tool_execution_end', async (event, ctx) => bridge.handleToolResult({
    toolCallId: String(event?.toolCallId ?? ''),
    toolName: String(event?.toolName ?? ''),
    isError: event?.isError === true,
  }, ctx));
  pi.on('session_start', async (event, ctx) => bridge.handleSessionStart(event, ctx));
  pi.on('input', async (event, ctx) => bridge.handleInput(event, ctx));
  pi.on('before_agent_start', async (event, ctx) => bridge.handleBeforeAgentStart(event, ctx));
  pi.on('agent_end', async (_event, ctx) => {
    try {
      const db = (options.getDb ?? ((hookCtx?: PiLikeContext) => defaultGetDb(options, hookCtx)))(ctx);
      finalizeActivePiFallbackRuns(db, {
        agentId: getPiAwarenessAgentId(ctx),
        sessionId: getPiAwarenessSessionId(ctx),
        workspacePath: ctx?.cwd ?? process.cwd(),
        artifact: artifactFrom(ctx, _event),
      });
      if (process.env.OCTOCODE_NO_VERIFY_GATE === '1') return undefined;
      const result = auditUnverified(db, {
        agentId: getPiAwarenessAgentId(ctx),
        workspacePath: ctx?.cwd ?? process.cwd(),
        artifact: artifactFrom(ctx, _event),
      });
      if (result.count === 0) {
        verifyReminderKeys.clear();
        return undefined;
      }
      const reminderKey = JSON.stringify({
        agentId: getPiAwarenessAgentId(ctx),
        workspacePath: ctx?.cwd ?? process.cwd(),
        artifact: artifactFrom(ctx, _event),
        runIds: [
          ...result.unverified.map((intent) => intent.run_id),
          ...result.stale_active.map((intent) => intent.run_id),
        ].sort(),
      });
      if (verifyReminderKeys.has(reminderKey)) return undefined;
      verifyReminderKeys.add(reminderKey);
      const details = [
        ...result.unverified.map((intent) => `${intent.status}:${intent.run_id}: ${intent.test_plan}`),
        ...result.stale_active.map((intent) => `STALE:${intent.run_id}: ${intent.rationale}`),
      ];
      const shown = details.slice(0, 3).join('; ');
      const omitted = details.length > 3 ? `; +${details.length - 3} omitted` : '';
      pi.sendMessage?.({
        customType: 'octocode-awareness-verify-gate',
        content: [
          'Octocode awareness verify gate: you have unverified edits before concluding.',
          shown ? `Pending: ${shown}${omitted}` : '',
          'Run the stated verification, then use octocode-awareness verify mark to clear the pending runs.',
        ].filter(Boolean).join('\n'),
        display: true,
      }, { deliverAs: 'followUp', triggerTurn: true });
      return undefined;
    } catch (error) {
      notify(ctx, `Octocode awareness verify warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
      return undefined;
    }
  });
  pi.on('session_before_compact', async (event, ctx) => bridge.handleSessionCompact({
    reason: typeof event?.reason === 'string' ? `compact:${event.reason}` : 'compact',
  }, ctx));
  pi.on('session_shutdown', async (event, ctx) => bridge.handleSessionShutdown(event, ctx));

  return bridge;
}
