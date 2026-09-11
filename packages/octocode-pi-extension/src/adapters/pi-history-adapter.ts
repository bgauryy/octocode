import { createHash } from 'node:crypto';
import {
  createAwarenessHost,
  historyToolEffect,
  loadWorkspacePolicy,
  type AwarenessHost,
} from '@octocodeai/octocode-awareness/host';
import { isPersistentStorageEnabledForExtension as isPersistentStorageEnabled } from '@octocodeai/config';
import type { PiContext } from '../types.js';
import { getAwarenessAgentId } from '../tools/awareness-shared.js';



interface ToolCallEvent {
  toolCallId: string;
  toolName: string;
  input: Record<string, unknown>;
}

interface ToolEndEvent {
  toolCallId: string;
  toolName: string;
  result: unknown;
  isError: boolean;
}

export interface PiHistoryAdapterOptions {
  createHost?: (context: AwarenessHost['context']) => AwarenessHost;
  agentId?: (ctx?: PiContext) => string;
  enabled?: (workspace: string) => boolean;
  onError?: (error: Error) => void;
  onAdvisory?: (message: string) => void;
}

export interface PiHistoryAdapter {
  before(event: ToolCallEvent, ctx?: PiContext): Promise<void>;
  after(event: ToolEndEvent, ctx?: PiContext): Promise<void>;
  pending(): number;
}

function sessionId(ctx?: PiContext): string {
  return ctx?.sessionManager?.getSessionId?.() ?? ctx?.sessionManager?.getSessionFile?.() ?? 'unknown-session';
}

function operationId(workspace: string, agentId: string, session: string, toolCallId: string): string {
  const digest = createHash('sha256').update(workspace).update('\0').update(agentId).update('\0').update(session).update('\0').update(toolCallId).digest('hex').slice(0, 24);
  return `pi_${digest}`;
}

/** Native Pi adapter for canonical Awareness local history. It records state and never restores it. */
export function createPiHistoryAdapter(options: PiHistoryAdapterOptions = {}): PiHistoryAdapter {
  const createHost = options.createHost ?? createAwarenessHost;
  const resolveAgentId = options.agentId ?? getAwarenessAgentId;
  const enabled = options.enabled ?? ((workspace: string) =>
    isPersistentStorageEnabled() && loadWorkspacePolicy(workspace).policy.hooks.profile === 'full');
  const active = new Map<string, {
    operationId: string;
    workspace: string;
    agentId: string;
    sessionId: string;
    host: AwarenessHost;
  }>();
  const report = (error: unknown): void => options.onError?.(error instanceof Error ? error : new Error(String(error)));

  return {
    async before(event, ctx) {
      if (!event.toolCallId || active.has(event.toolCallId)) return;
      const effect = historyToolEffect(event.toolName, event.input);
      if (effect.effect !== 'workspace-write') return;
      const workspace = ctx?.cwd ?? process.cwd();
      if (!enabled(workspace)) return;
      const agentId = resolveAgentId(ctx);
      const session = sessionId(ctx);
      const id = operationId(workspace, agentId, session, event.toolCallId);
      try {
        const host = createHost({ workspace, agentId, sessionId: session });
        const payload = await host.captureHistory({
          phase: 'before', operation_id: id, session_id: session, host: 'pi', label: `${event.toolName} mutation ${id}`, file: effect.files,
        });
        const operation = payload['operation'];
        if (operation && typeof operation === 'object' && ['partial', 'failed'].includes(String((operation as Record<string, unknown>)['status']))) {
          options.onAdvisory?.(`Awareness history before-capture ${id} is ${(operation as Record<string, unknown>)['status']}.`);
        }
        if (active.size >= 256) active.delete(active.keys().next().value as string);
        active.set(event.toolCallId, { operationId: id, workspace, agentId, sessionId: session, host });
      } catch (error) {
        report(error);
      }
    },

    async after(event, ctx) {
      const capture = active.get(event.toolCallId);
      if (!capture) return;
      if ((ctx?.cwd ?? process.cwd()) !== capture.workspace || sessionId(ctx) !== capture.sessionId) {
        report(new Error(`Ignoring stale Pi history completion for ${event.toolCallId}`));
        return;
      }
      try {
        await capture.host.captureHistory({
          phase: 'after', operation_id: capture.operationId, session_id: capture.sessionId, host: 'pi', outcome: event.isError ? 'failure' : 'success',
        });
        active.delete(event.toolCallId);
      } catch (error) {
        report(error);
      }
    },

    pending: () => active.size,
  };
}
