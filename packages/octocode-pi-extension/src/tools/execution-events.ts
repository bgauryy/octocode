import { z } from 'zod';
import { retainRecent } from './execution-retention.js';
import {
  executionAgentMessageSchema,
  executionAgentSchema,
  executionAgentTransitionSchema,
} from './execution-agent-events.js';

/** Semantic execution history. Schemas own transport validation and inferred types. */
export const EXECUTION_ENTRY_TYPE = 'octocode-execution-event';
/** Transcript-visible execution facts. Still a CustomEntry: durable UI, zero model-context cost. */
export const EXECUTION_TRANSCRIPT_ENTRY_TYPE =
  'octocode-execution-transcript-event';
const nonnegative = z.number().finite().nonnegative();
const outputReferenceSchema = z.discriminatedUnion('kind', [
  z.strictObject({
    kind: z.literal('tool-result'),
    toolCallId: z.string().min(1),
  }),
  z.strictObject({
    kind: z.literal('message'),
    role: z.enum(['user', 'assistant']),
    timestamp: nonnegative,
  }),
]);
const usageSchema = z.strictObject({
  input: nonnegative.optional(),
  output: nonnegative.optional(),
  cacheRead: nonnegative.optional(),
  cacheWrite: nonnegative.optional(),
  cost: nonnegative.optional(),
});
const fileSchema = z.strictObject({
  path: z.string().min(1),
  operation: z.enum(['create', 'modify', 'delete', 'rename']),
  additions: nonnegative.optional(),
  deletions: nonnegative.optional(),
});
const planSchema = z.strictObject({
  id: z.string().min(1),
  phase: z.string(),
  revision: z.string().optional(),
  tasks: z.array(
    z.strictObject({
      id: z.string().min(1),
      title: z.string(),
      status: z.string(),
    })
  ),
});
const sessionSchema = z.strictObject({
  cwd: z.string().optional(),
  model: z.string().optional(),
  provider: z.string().optional(),
});
const toolSchema = z.strictObject({
  toolCallId: z.string().min(1),
  tool: z.string().min(1),
  title: z.string(),
});
const outcomeSchema = z.strictObject({
  toolCallId: z.string().min(1),
  summary: z.string(),
  outputRef: outputReferenceSchema.optional(),
  exitCode: z.number().int().optional(),
});
const skillSchema = z.strictObject({
  name: z.string().min(1),
  source: z.string().optional(),
  scope: z.string().optional(),
});
const resolutionSchema = z.strictObject({
  id: z.string().min(1),
  decision: z.string(),
});
const messageSchema = z.strictObject({
  messageId: z.string().min(1),
  outputRef: outputReferenceSchema.optional(),
});
const payloadSchemas = {
  'session.started': sessionSchema,
  'session.updated': sessionSchema,
  'session.completed': z.strictObject({ reason: z.string().optional() }),
  'turn.started': z.strictObject({}),
  'turn.completed': z.strictObject({
    status: z.enum(['completed', 'failed', 'cancelled']),
    usage: usageSchema.optional(),
  }),
  'user.message': messageSchema,
  'assistant.started': z.strictObject({ messageId: z.string().min(1) }),
  'assistant.completed': messageSchema.extend({
    status: z.enum(['completed', 'interrupted', 'failed']),
  }),
  'assistant.progress': z.strictObject({
    message: z.string(),
    phase: z.string().optional(),
  }),
  'tool.requested': toolSchema,
  'tool.started': toolSchema,
  'tool.completed': outcomeSchema,
  'tool.failed': outcomeSchema,
  'tool.cancelled': outcomeSchema,
  'skill.activated': skillSchema,
  'skill.failed': skillSchema,
  'file.changed': fileSchema,
  'plan.updated': planSchema,
  'agent.updated': executionAgentSchema,
  'agent.message': executionAgentMessageSchema,
  'agent.transition': executionAgentTransitionSchema,
  'permission.requested': z.strictObject({
    id: z.string().min(1),
    title: z.string(),
    persistent: z.boolean().optional(),
    expiresAt: nonnegative.optional(),
  }),
  'permission.resolved': resolutionSchema,
  'question.requested': z.strictObject({
    id: z.string().min(1),
    title: z.string(),
    persistent: z.boolean().optional(),
    expiresAt: nonnegative.optional(),
  }),
  'question.resolved': resolutionSchema,
  'context.started': z.strictObject({}),
  'context.compacted': z.strictObject({
    tokensBefore: nonnegative.optional(),
    tokensAfter: nonnegative.optional(),
  }),
  'context.failed': z.strictObject({ cancelled: z.boolean() }),
  warning: z.strictObject({ message: z.string() }),
  error: z.strictObject({ message: z.string() }),
};
const envelopeSchema = z.strictObject({
  version: z.literal(1),
  id: z.string().min(1),
  sessionId: z.string().min(1),
  runId: z.string().min(1),
  parentRunId: z.string().optional(),
  turnId: z.string().optional(),
  sequence: z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
  timestamp: nonnegative,
  visibility: z.enum(['transcript', 'activity', 'debug']),
  type: z.string(),
  payload: z.unknown(),
});
export type OutputReference = z.infer<typeof outputReferenceSchema>;
export type ExecutionUsage = z.infer<typeof usageSchema>;
export type ExecutionFile = z.infer<typeof fileSchema>;
export type ExecutionAgent = z.infer<typeof executionAgentSchema>;
export type ExecutionPlan = z.infer<typeof planSchema>;
export type ExecutionStatus =
  'requested' | 'running' | 'succeeded' | 'failed' | 'cancelled';
export type ExecutionPayloads = {
  [K in keyof typeof payloadSchemas]: z.infer<(typeof payloadSchemas)[K]>;
};
export type ExecutionEvent = {
  [K in keyof ExecutionPayloads]: Omit<
    z.infer<typeof envelopeSchema>,
    'type' | 'payload'
  > & { type: K; payload: ExecutionPayloads[K] };
}[keyof ExecutionPayloads];

export interface ExecutionTool {
  id: string;
  tool: string;
  title: string;
  status: ExecutionStatus;
  turnId?: string;
  startedAt: number;
  durationMs?: number;
  summary?: string;
  outputRef?: OutputReference;
}
export interface ExecutionInteraction {
  id: string;
  kind: 'question' | 'permission';
  title: string;
  status: string;
  turnId?: string;
  startedAt: number;
  persistent?: boolean;
  expiresAt?: number;
}
export interface ExecutionState {
  sequence: number;
  sessionId?: string;
  runId?: string;
  startedAt?: number;
  cwd?: string;
  model?: string;
  provider?: string;
  activeTurnId?: string;
  activeTurnStartedAt?: number;
  completedTurns: number;
  lastTurnMs?: number;
  compacting: boolean;
  progress?: { message: string; since: number; phase?: string };
  tools: Record<string, ExecutionTool>;
  activeToolIds: string[];
  toolCount: number;
  messages: Record<
    string,
    { role: 'user' | 'assistant'; status: string; outputRef?: OutputReference }
  >;
  skills: Record<
    string,
    {
      name: string;
      source?: string;
      scope?: string;
      status: 'active' | 'failed';
    }
  >;
  files: Record<string, ExecutionFile>;
  interactions: Record<string, ExecutionInteraction>;
  agents: Record<string, ExecutionAgent>;
  plan?: ExecutionPlan;
  usage: ExecutionUsage;
}

export function createExecutionState(): ExecutionState {
  return {
    sequence: 0,
    completedTurns: 0,
    compacting: false,
    tools: {},
    activeToolIds: [],
    toolCount: 0,
    messages: {},
    skills: {},
    files: {},
    interactions: {},
    agents: {},
    usage: {},
  };
}

function addUsage(
  before: ExecutionUsage,
  usage: ExecutionUsage = {}
): ExecutionUsage {
  const next = { ...before };
  for (const key of [
    'input',
    'output',
    'cacheRead',
    'cacheWrite',
    'cost',
  ] as const) {
    const value = usage[key];
    if (typeof value === 'number' && Number.isFinite(value) && value >= 0)
      next[key] = (next[key] ?? 0) + value;
  }
  return next;
}

/** Pure replay reducer. Sequence fences reject duplicate and late publications. */
export function reduceExecutionEvent(
  state: ExecutionState,
  event: ExecutionEvent
): ExecutionState {
  if (event.sequence <= state.sequence) return state;
  const next: ExecutionState = {
    ...state,
    sequence: event.sequence,
    sessionId: event.sessionId,
    runId: event.runId,
  };
  switch (event.type) {
    case 'session.started':
      return {
        ...next,
        cwd: event.payload.cwd ?? state.cwd,
        model: event.payload.model ?? state.model,
        provider: event.payload.provider ?? state.provider,
        startedAt: state.startedAt ?? event.timestamp,
      };
    case 'session.updated':
      return {
        ...next,
        cwd: event.payload.cwd ?? state.cwd,
        model: event.payload.model ?? state.model,
        provider: event.payload.provider ?? state.provider,
      };
    case 'turn.started':
      return {
        ...next,
        activeTurnId: event.turnId,
        activeTurnStartedAt: event.timestamp,
        progress: undefined,
      };
    case 'session.completed':
    case 'turn.completed': {
      const tools = Object.fromEntries(
        Object.entries(state.tools).map(([id, tool]) => [
          id,
          (tool.status === 'running' || tool.status === 'requested') &&
          (event.type === 'session.completed' || tool.turnId === event.turnId)
            ? {
                ...tool,
                status: 'cancelled' as const,
                durationMs: Math.max(0, event.timestamp - tool.startedAt),
              }
            : tool,
        ])
      );
      const interactions = Object.fromEntries(
        Object.entries(state.interactions).map(([id, item]) => [
          id,
          item.status === 'waiting' &&
          !item.persistent &&
          (event.type === 'session.completed' || item.turnId === event.turnId)
            ? { ...item, status: 'cancelled' }
            : item,
        ])
      );
      return {
        ...next,
        tools: retainRecent(
          tools,
          tool => tool.status === 'running' || tool.status === 'requested'
        ),
        activeToolIds: state.activeToolIds.filter(
          id =>
            tools[id]?.status === 'running' || tools[id]?.status === 'requested'
        ),
        interactions,
        messages: retainRecent(
          Object.fromEntries(
            Object.entries(state.messages).map(([id, message]) => [
              id,
              message.status === 'streaming'
                ? { ...message, status: 'interrupted' }
                : message,
            ])
          ),
          message => message.status === 'streaming'
        ),
        compacting: false,
        activeTurnId: undefined,
        activeTurnStartedAt: undefined,
        progress: undefined,
        completedTurns:
          state.completedTurns + Number(event.type === 'turn.completed'),
        lastTurnMs:
          state.activeTurnStartedAt === undefined
            ? state.lastTurnMs
            : Math.max(0, event.timestamp - state.activeTurnStartedAt),
        usage:
          event.type === 'turn.completed'
            ? addUsage(state.usage, event.payload.usage)
            : state.usage,
      };
    }
    case 'assistant.progress':
      return {
        ...next,
        progress: { ...event.payload, since: event.timestamp },
      };
    case 'user.message':
      return {
        ...next,
        messages: retainRecent(
          {
            ...state.messages,
            [event.payload.messageId]: {
              role: 'user',
              status: 'completed',
              outputRef: event.payload.outputRef,
            },
          },
          message => message.status === 'streaming'
        ),
      };
    case 'assistant.started':
      return {
        ...next,
        messages: retainRecent(
          {
            ...state.messages,
            [event.payload.messageId]: {
              role: 'assistant',
              status: 'streaming',
            },
          },
          message => message.status === 'streaming'
        ),
      };
    case 'assistant.completed':
      return {
        ...next,
        messages: retainRecent(
          {
            ...state.messages,
            [event.payload.messageId]: {
              role: 'assistant',
              status: event.payload.status,
              outputRef: event.payload.outputRef,
            },
          },
          message => message.status === 'streaming'
        ),
      };
    case 'tool.requested':
    case 'tool.started': {
      const p = event.payload;
      const previous = state.tools[p.toolCallId];
      if (previous && previous.status !== 'requested') return next;
      return {
        ...next,
        toolCount: state.toolCount + Number(!previous),
        activeToolIds: previous
          ? state.activeToolIds
          : [...state.activeToolIds, p.toolCallId],
        tools: retainRecent(
          {
            ...state.tools,
            [p.toolCallId]: {
              id: p.toolCallId,
              tool: p.tool,
              title: p.title,
              status: event.type === 'tool.started' ? 'running' : 'requested',
              turnId: event.turnId,
              startedAt: event.timestamp,
            },
          },
          tool => tool.status === 'running' || tool.status === 'requested'
        ),
      };
    }
    case 'tool.completed':
    case 'tool.failed':
    case 'tool.cancelled': {
      const p = event.payload;
      const previous = state.tools[p.toolCallId];
      if (
        !previous ||
        (previous.status !== 'running' && previous.status !== 'requested')
      )
        return next;
      return {
        ...next,
        activeToolIds: state.activeToolIds.filter(id => id !== p.toolCallId),
        tools: retainRecent(
          {
            ...state.tools,
            [p.toolCallId]: {
              ...previous,
              status:
                event.type === 'tool.completed'
                  ? 'succeeded'
                  : event.type === 'tool.failed'
                    ? 'failed'
                    : 'cancelled',
              summary: p.summary,
              outputRef: p.outputRef,
              durationMs: Math.max(0, event.timestamp - previous.startedAt),
            },
          },
          tool => tool.status === 'running' || tool.status === 'requested'
        ),
      };
    }
    case 'skill.activated':
    case 'skill.failed':
      return {
        ...next,
        skills: {
          ...state.skills,
          [event.payload.name]: {
            ...event.payload,
            status: event.type === 'skill.activated' ? 'active' : 'failed',
          },
        },
      };
    case 'file.changed':
      return {
        ...next,
        files: { ...state.files, [event.payload.path]: { ...event.payload } },
      };
    case 'plan.updated':
      return {
        ...next,
        plan: {
          ...event.payload,
          tasks: event.payload.tasks.map(task => ({ ...task })),
        },
      };
    case 'agent.updated':
      return {
        ...next,
        agents: { ...state.agents, [event.payload.id]: { ...event.payload } },
      };
    case 'agent.message':
    case 'agent.transition':
      return next;
    case 'question.requested':
    case 'permission.requested':
      return {
        ...next,
        interactions: {
          ...state.interactions,
          [event.payload.id]: {
            ...event.payload,
            kind:
              event.type === 'question.requested' ? 'question' : 'permission',
            status: 'waiting',
            turnId: event.turnId,
            startedAt: event.timestamp,
          },
        },
      };
    case 'question.resolved':
    case 'permission.resolved': {
      const previous = state.interactions[event.payload.id];
      return previous
        ? {
            ...next,
            interactions: {
              ...state.interactions,
              [previous.id]: { ...previous, status: event.payload.decision },
            },
          }
        : next;
    }
    case 'context.started':
      return { ...next, compacting: true };
    case 'context.compacted':
    case 'context.failed':
      return { ...next, compacting: false };
    case 'warning':
    case 'error':
      return next;
  }
}

/** Validate persisted input against the same contract used by producers. */
export function isExecutionEvent(value: unknown): value is ExecutionEvent {
  const envelope = envelopeSchema.safeParse(value);
  if (!envelope.success || !Object.hasOwn(payloadSchemas, envelope.data.type))
    return false;
  return payloadSchemas[
    envelope.data.type as keyof ExecutionPayloads
  ].safeParse(envelope.data.payload).success;
}
