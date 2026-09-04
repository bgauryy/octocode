import {
  eventId,
  sessionId,
  turnId,
  type AgentEventEnvelope,
  type AgentEventType,
  type EventAuthority,
  type EventPhase,
  type LifecycleBus,
  type LifecycleDecision,
  type LifecycleDispatchResult,
  type RuntimeMode,
} from '@octocodeai/agent-core';
import type { PiContext, PiInstance } from '../types.js';

export type PiLifecycleEvent = keyof typeof PI_LIFECYCLE_MAPPINGS;

export interface PiLifecycleMapping {
  readonly canonical: AgentEventType;
  readonly phase: EventPhase;
  readonly authority: readonly EventAuthority[];
}

export const PI_LIFECYCLE_MAPPINGS = {
  resources_discover: { canonical: 'resources.discovering', phase: 'before', authority: ['rewrite', 'context'] },
  project_trust: { canonical: 'trust.resolving', phase: 'permission', authority: ['allow-deny'] },
  context: { canonical: 'context.preparing', phase: 'before', authority: ['rewrite', 'context'] },
  input: { canonical: 'input.received', phase: 'before', authority: ['rewrite', 'context', 'stop'] },
  before_agent_start: { canonical: 'agent.before-start', phase: 'before', authority: ['rewrite', 'context', 'stop'] },
  session_start: { canonical: 'session.started', phase: 'after', authority: ['context', 'stop'] },
  session_shutdown: { canonical: 'session.stopping', phase: 'before', authority: ['observe'] },
  session_info_changed: { canonical: 'session.metadata-changed', phase: 'after', authority: ['observe'] },
  session_before_switch: { canonical: 'session.before-switch', phase: 'before', authority: ['stop'] },
  session_before_fork: { canonical: 'session.before-fork', phase: 'before', authority: ['stop'] },
  session_before_compact: { canonical: 'context.compaction-started', phase: 'before', authority: ['rewrite', 'context', 'stop'] },
  session_compact: { canonical: 'context.compacted', phase: 'after', authority: ['observe', 'context'] },
  session_compact_failed: { canonical: 'context.compaction-failed', phase: 'after', authority: ['observe'] },
  session_tree: { canonical: 'session.tree-changed', phase: 'after', authority: ['observe'] },
  agent_start: { canonical: 'agent.started', phase: 'after', authority: ['observe'] },
  agent_end: { canonical: 'agent.ended', phase: 'after', authority: ['observe'] },
  turn_start: { canonical: 'turn.started', phase: 'after', authority: ['observe'] },
  turn_end: { canonical: 'turn.ended', phase: 'after', authority: ['observe'] },
  message_start: { canonical: 'message.started', phase: 'after', authority: ['observe'] },
  message_end: { canonical: 'message.ended', phase: 'after', authority: ['rewrite', 'context'] },
  tool_call: { canonical: 'tool.requested', phase: 'permission', authority: ['allow-deny', 'rewrite'] },
  tool_execution_start: { canonical: 'tool.started', phase: 'after', authority: ['observe'] },
  tool_execution_end: { canonical: 'tool.ended', phase: 'after', authority: ['observe'] },
  model_select: { canonical: 'model.selected', phase: 'after', authority: ['observe'] },
  thinking_level_select: { canonical: 'model.thinking-level-selected', phase: 'after', authority: ['observe'] },
  before_provider_request: { canonical: 'provider.request-started', phase: 'before', authority: ['rewrite'] },
  after_provider_response: { canonical: 'provider.response-received', phase: 'after', authority: ['observe'] },
} as const satisfies Record<string, PiLifecycleMapping>;

export function isPiLifecycleEvent(event: string): event is PiLifecycleEvent {
  return Object.prototype.hasOwnProperty.call(PI_LIFECYCLE_MAPPINGS, event);
}

function runtimeMode(mode: PiContext['mode']): RuntimeMode {
  return mode === 'tui' ? 'interactive' : mode ?? 'headless';
}

async function trustState(ctx: PiContext | undefined): Promise<'trusted' | 'untrusted' | 'unknown'> {
  try {
    if (!ctx?.isProjectTrusted) return 'unknown';
    return await ctx.isProjectTrusted() ? 'trusted' : 'untrusted';
  } catch {
    return 'unknown';
  }
}

export async function createPiEventEnvelope<T extends Record<string, unknown>>(
  piEvent: PiLifecycleEvent,
  payload: T,
  ctx: PiContext | undefined,
  sequence: number,
): Promise<AgentEventEnvelope<AgentEventType, T>> {
  const mapping = PI_LIFECYCLE_MAPPINGS[piEvent];
  const cwd = ctx?.cwd ?? process.cwd();
  const currentSessionId = ctx?.sessionManager?.getSessionId?.() ?? ctx?.sessionManager?.getSessionFile?.() ?? `pi:${cwd}`;
  const turnIndex = typeof payload['turnIndex'] === 'number' ? String(payload['turnIndex']) : undefined;
  return Object.freeze({
    schemaVersion: 1,
    eventVersion: 1,
    id: eventId(`pi:${sequence}`),
    type: mapping.canonical,
    phase: mapping.phase,
    sessionId: sessionId(currentSessionId),
    ...(turnIndex ? { turnId: turnId(turnIndex) } : {}),
    timestamp: Date.now(),
    cwd,
    mode: runtimeMode(ctx?.mode),
    ...(ctx?.model?.id ? { model: { providerId: ctx.model.provider ?? 'unknown', modelId: ctx.model.id } } : {}),
    trust: { workspace: await trustState(ctx), managedOnly: false },
    payload: Object.freeze({ ...payload }),
  });
}

function mapDecisionToPi(piEvent: PiLifecycleEvent, result: LifecycleDispatchResult<Record<string, unknown>>): unknown {
  const decision = result.decision;
  if (piEvent === 'tool_call' && (decision.kind === 'deny' || decision.kind === 'stop')) return { block: true, reason: decision.reason };
  if ((piEvent === 'session_before_switch' || piEvent === 'session_before_fork' || piEvent === 'session_before_compact') && decision.kind === 'stop') return { cancel: true };
  if (piEvent === 'project_trust' && decision.kind === 'deny') return { trusted: 'no' };
  if (piEvent === 'project_trust' && decision.kind === 'allow') return { trusted: 'yes' };
  const rewrote = decision.kind === 'rewrite' || result.receipts.some((receipt) => receipt.decision === 'rewrite');
  if (!rewrote) return piEvent === 'input' ? { action: 'continue' } : undefined;
  if (piEvent === 'input') return { action: 'transform', text: result.payload['text'], images: result.payload['images'] };
  if (piEvent === 'before_agent_start') {
    return {
      ...(result.payload['systemPrompt'] === undefined ? {} : { systemPrompt: result.payload['systemPrompt'] }),
      ...(result.payload['message'] === undefined ? {} : { message: result.payload['message'] }),
    };
  }
  if (piEvent === 'context') return { messages: result.payload['messages'] };
  if (piEvent === 'resources_discover') return result.payload['skillPaths'] === undefined ? {} : { skillPaths: result.payload['skillPaths'] };
  if (piEvent === 'message_end') return { message: result.payload['message'] };
  if (piEvent === 'before_provider_request') return result.payload;
  if (piEvent === 'session_before_compact') return { compaction: result.payload['compaction'] };
  return undefined;
}

export function mapPiHookResultToDecision(
  piEvent: PiLifecycleEvent,
  payload: Record<string, unknown>,
  result: unknown,
): LifecycleDecision<Record<string, unknown>> | undefined {
  if (!result || typeof result !== 'object' || Array.isArray(result)) return undefined;
  const value = result as Record<string, unknown>;
  if (piEvent === 'tool_call' && value['block'] === true) {
    return { kind: 'deny', reason: typeof value['reason'] === 'string' ? value['reason'] : 'Blocked by Octocode policy' };
  }
  if ((piEvent === 'session_before_switch' || piEvent === 'session_before_fork' || piEvent === 'session_before_compact') && value['cancel'] === true) {
    return { kind: 'stop', reason: 'Cancelled by Octocode lifecycle handler' };
  }
  if (piEvent === 'input' && value['action'] === 'handled') return { kind: 'stop', reason: 'Input handled by extension' };
  if (piEvent === 'input' && value['action'] === 'transform') {
    return { kind: 'rewrite', payload: { ...payload, ...(typeof value['text'] === 'string' ? { text: value['text'] } : {}), ...(Array.isArray(value['images']) ? { images: value['images'] } : {}) } };
  }
  if (piEvent === 'resources_discover') {
    return { kind: 'rewrite', payload: { ...payload, ...(Array.isArray(value['skillPaths']) ? { skillPaths: value['skillPaths'] } : {}) } };
  }
  if (piEvent === 'before_agent_start' && (value['systemPrompt'] !== undefined || value['message'] !== undefined)) {
    return {
      kind: 'rewrite',
      payload: {
        ...payload,
        ...(value['systemPrompt'] === undefined ? {} : { systemPrompt: value['systemPrompt'] }),
        ...(value['message'] === undefined ? {} : { message: value['message'] }),
      },
    };
  }
  const rewriteKeys: Partial<Record<PiLifecycleEvent, string>> = {
    context: 'messages',
    message_end: 'message',
    session_before_compact: 'compaction',
  };
  const rewriteKey = rewriteKeys[piEvent];
  if (rewriteKey && value[rewriteKey] !== undefined) return { kind: 'rewrite', payload: { ...payload, [rewriteKey]: value[rewriteKey] } };
  return undefined;
}

export function bindPiLifecycleBus(
  pi: PiInstance,
  piEvent: PiLifecycleEvent,
  bus: LifecycleBus<Record<string, unknown>>,
  options: {
    onEnvelope?: (envelope: AgentEventEnvelope<AgentEventType, Record<string, unknown>>, ctx: PiContext | undefined) => void;
    onComplete?: (envelope: AgentEventEnvelope<AgentEventType, Record<string, unknown>>) => void;
  } = {},
): void {
  let sequence = 0;
  // LifecycleBus.dispatch() guards against reentrant calls on the same event
  // type via an internal #active Set. In parallel tool mode, Pi can fire
  // tool_execution_end (and potentially other events) concurrently for multiple
  // in-flight tool calls. Without serialization the second concurrent handler
  // enters bus.dispatch() while the first is still awaiting, which throws
  // "Recursive intercepting event: <type>". This queue chains every dispatch
  // behind the previous one so at most one is running at a time.
  // The resolved promise is returned to pi so control-flow events (tool_call,
  // input, etc.) still deliver their decisions; observe-only events ignore it.
  let dispatchQueue: Promise<unknown> = Promise.resolve();

  pi.on(piEvent, (rawEvent: unknown, rawContext: unknown) => {
    const payload = rawEvent && typeof rawEvent === 'object' && !Array.isArray(rawEvent)
      ? rawEvent as Record<string, unknown>
      : { value: rawEvent };
    // Pi deliberately supplies a stale replacement-session proxy for non-quit
    // shutdowns. Never inspect or forward that context across the adapter seam.
    const context = piEvent === 'session_shutdown' && payload['reason'] !== 'quit'
      ? undefined
      : rawContext as PiContext | undefined;
    // Capture sequence number in arrival order before entering the queue.
    const seq = sequence++;

    const dispatched = dispatchQueue.then(async () => {
      const envelope = await createPiEventEnvelope(piEvent, payload, context, seq);
      options.onEnvelope?.(envelope, context);
      try {
        return mapDecisionToPi(piEvent, await bus.dispatch(envelope));
      } finally {
        options.onComplete?.(envelope);
      }
    });

    // Advance the tail without propagating rejections — a failed dispatch must
    // not block subsequent events from being processed.
    dispatchQueue = dispatched.then(() => undefined, () => undefined);
    return dispatched;
  });
}
