import {
  PiRuntimeObservationSchema,
  type PiRuntimeObservation,
} from '@octocodeai/agent-contracts/physiology';
import type { PiContext, PiInstance } from '../types.js';

type ToolOutcome = 'succeeded' | 'failed' | 'cancelled' | 'blocked';
type ToolEvent = { toolCallId?: unknown; toolName?: unknown; args?: unknown; [key: string]: unknown };

export interface PiPhysiologyOptions {
  now?: () => number;
  onObservation?: (observation: PiRuntimeObservation) => void | Promise<void>;
  isInternalTool?: (event: Readonly<ToolEvent>) => boolean;
}

export interface PiPhysiologyObserver {
  sessionStart(ctx: PiContext): Promise<void>;
  sessionShutdown(ctx?: PiContext): Promise<void>;
  sampleContext(ctx: PiContext): Promise<void>;
  invalidateContext(ctx: PiContext): Promise<void>;
  toolStart(event: ToolEvent, ctx: PiContext): Promise<void>;
  toolTerminal(event: ToolEvent, ctx: PiContext): Promise<void>;
  compactionStart(event: unknown, ctx: PiContext): Promise<void>;
  compactionSucceeded(event: unknown, ctx: PiContext): Promise<void>;
  compactionFailed(event: unknown, ctx: PiContext): Promise<void>;
  read(ctx: PiContext): PiRuntimeObservation | undefined;
}

interface ActiveSession {
  id: string;
  generation: number;
  observedAt: number;
  context?: PiRuntimeObservation['context'];
  outcomes: ToolOutcome[];
  compactionsCommitted: number;
  compactionsFailed: number;
  hasCompactionMeasurement: boolean;
  activeCompaction?: string;
  compactionSequence: number;
}

const observersByContext = new WeakMap<object, PiPhysiologyObserver>();
const observersBySessionManager = new WeakMap<object, PiPhysiologyObserver>();
const MAX_TRACKED_IDS = 256;

function record(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === 'object' ? value as Record<string, unknown> : undefined;
}

function boundedSetAdd(set: Set<string>, value: string): void {
  set.add(value);
  if (set.size <= MAX_TRACKED_IDS) return;
  const oldest = set.values().next().value as string | undefined;
  if (oldest !== undefined) set.delete(oldest);
}

function boundedMapSet<T>(map: Map<string, T>, key: string, value: T): void {
  map.set(key, value);
  if (map.size <= MAX_TRACKED_IDS) return;
  const oldest = map.keys().next().value as string | undefined;
  if (oldest !== undefined) map.delete(oldest);
}

function explicitFlag(event: ToolEvent, names: readonly string[]): boolean {
  const result = record(event['result']);
  const details = record(result?.['details']);
  return names.some((name) => event[name] === true || result?.[name] === true || details?.[name] === true);
}

function terminalOutcome(event: ToolEvent): ToolOutcome | undefined {
  if (explicitFlag(event, ['cancelled', 'canceled', 'aborted'])) return 'cancelled';
  if (explicitFlag(event, ['blocked', 'denied'])) return 'blocked';
  if (event['isError'] === true || event['is_error'] === true) return 'failed';
  if (event['isError'] === false || event['is_error'] === false) return 'succeeded';
  return undefined;
}

function defaultInternalTool(event: Readonly<ToolEvent>): boolean {
  const name = typeof event.toolName === 'string' ? event.toolName.toLowerCase() : '';
  if (name.includes('awareness') || name.startsWith('__')) return true;
  if (name !== 'bash') return false;
  const toolInput = record(event['input']) ?? record(event.args);
  const isAwarenessCliInvocation = (value: unknown): boolean => typeof value === 'string'
    && /(?:OCTOCODE_AWARENESS_CLI|@octocodeai\/octocode-awareness|\boctocode-awareness\b)/i.test(value);
  if (isAwarenessCliInvocation(toolInput?.['command'])) return true;
  const queries = toolInput?.['queries'];
  if (!Array.isArray(queries) || queries.length === 0 || queries.length > 100) return false;
  return queries.every((query) => isAwarenessCliInvocation(record(query)?.['command']));
}

function eventId(event: ToolEvent): string | undefined {
  return typeof event.toolCallId === 'string' && event.toolCallId.length > 0 && event.toolCallId.length <= 512
    ? event.toolCallId
    : undefined;
}

function compactionEventId(event: unknown): string | undefined {
  const entry = record(record(event)?.['compactionEntry']);
  const id = entry?.['id'];
  return typeof id === 'string' && id.length > 0 && id.length <= 512 ? `entry:${id}` : undefined;
}

export function createPiPhysiologyObserver(options: PiPhysiologyOptions = {}): PiPhysiologyObserver {
  const now = options.now ?? Date.now;
  const contextGenerations = new WeakMap<object, number>();
  const pendingTools = new Map<string, boolean>();
  const terminalToolIds = new Set<string>();
  const terminalCompactionIds = new Set<string>();
  let generation = 0;
  let active: ActiveSession | undefined;

  const safeNow = (): number => {
    const value = now();
    return Number.isSafeInteger(value) && value >= 0 ? value : Date.now();
  };

  const boundGeneration = (ctx: PiContext): number | undefined => (
    contextGenerations.get(ctx as object)
    ?? (ctx.sessionManager ? contextGenerations.get(ctx.sessionManager as object) : undefined)
  );

  const matches = (ctx: PiContext): boolean => {
    if (!active) return false;
    const bound = boundGeneration(ctx);
    if (bound !== undefined && bound !== active.generation) return false;
    try {
      if (ctx.sessionManager?.getSessionId?.() !== active.id) return false;
      if (bound === undefined) contextGenerations.set(ctx as object, active.generation);
      return true;
    } catch {
      return false;
    }
  };

  const snapshot = (): PiRuntimeObservation | undefined => {
    if (!active) return undefined;
    const counts = { failed: 0, cancelled: 0, blocked: 0 };
    for (const outcome of active.outcomes) {
      if (outcome !== 'succeeded') counts[outcome]++;
    }
    return PiRuntimeObservationSchema.parse({
      schema_version: 1,
      source: 'pi_runtime',
      session: {
        owner: 'pi',
        session_id: active.id,
        generation: active.generation,
        observed_at: active.observedAt,
      },
      ...(active.context === undefined ? {} : { context: active.context }),
      ...(active.outcomes.length === 0 ? {} : {
        tools: { window: 32, observed: active.outcomes.length, ...counts },
      }),
      ...(active.hasCompactionMeasurement ? {
        compaction: {
          owner: 'pi',
          committed: active.compactionsCommitted,
          failed: active.compactionsFailed,
        },
      } : {}),
    });
  };

  const emit = (signal?: AbortSignal): Promise<void> => {
    if (signal?.aborted) return Promise.resolve();
    const observation = snapshot();
    if (!observation || !options.onObservation) return Promise.resolve();
    try {
      void Promise.resolve(options.onObservation(observation)).catch(() => undefined);
    } catch {
      // Host observation callbacks cannot interrupt Pi lifecycle delivery.
    }
    return Promise.resolve();
  };

  const touch = (): void => {
    if (active) active.observedAt = safeNow();
  };

  const sample = (ctx: PiContext): void => {
    if (!active || !matches(ctx)) return;
    active.context = undefined;
    let usage: ReturnType<NonNullable<PiContext['getContextUsage']>>;
    try {
      usage = ctx.getContextUsage?.();
    } catch {
      return;
    }
    const tokens = usage?.tokens;
    const inputLimit = usage?.contextWindow;
    const modelLimit = ctx.model?.contextWindow;
    if (!Number.isSafeInteger(tokens) || (tokens as number) < 0
      || !Number.isSafeInteger(inputLimit) || (inputLimit as number) <= 0
      || inputLimit !== modelLimit) return;
    const measuredAt = safeNow();
    active.context = {
      measurement: 'host_reported',
      current_tokens: tokens as number,
      measured_at: measuredAt,
      input_limit_tokens: inputLimit as number,
      remaining_input_tokens: Math.max(0, (inputLimit as number) - (tokens as number)),
      saturation_basis_points: Math.min(10_000, Math.floor(((tokens as number) / (inputLimit as number)) * 10_000)),
    };
    active.observedAt = measuredAt;
  };

  const observer: PiPhysiologyObserver = {
    async sessionStart(ctx) {
      let id: string | undefined;
      try {
        id = ctx.sessionManager?.getSessionId?.();
      } catch {
        id = undefined;
      }
      const nextGeneration = Math.min(Number.MAX_SAFE_INTEGER, generation + 1);
      const parsed = PiRuntimeObservationSchema.shape.session.safeParse({
        owner: 'pi', session_id: id, generation: nextGeneration, observed_at: safeNow(),
      });
      active = undefined;
      pendingTools.clear();
      terminalToolIds.clear();
      terminalCompactionIds.clear();
      if (!parsed.success) return;
      generation = nextGeneration;
      active = {
        id: parsed.data.session_id,
        generation,
        observedAt: parsed.data.observed_at,
        outcomes: [],
        compactionsCommitted: 0,
        compactionsFailed: 0,
        hasCompactionMeasurement: false,
        compactionSequence: 0,
      };
      contextGenerations.set(ctx as object, generation);
      if (ctx.sessionManager) contextGenerations.set(ctx.sessionManager as object, generation);
      sample(ctx);
      await emit(ctx.signal);
    },

    async sessionShutdown(ctx) {
      if (ctx !== undefined && active
        && boundGeneration(ctx) !== active.generation) return;
      active = undefined;
      pendingTools.clear();
      terminalToolIds.clear();
      terminalCompactionIds.clear();
    },

    async sampleContext(ctx) {
      if (!matches(ctx)) return;
      touch();
      sample(ctx);
      await emit(ctx.signal);
    },

    async invalidateContext(ctx) {
      if (!matches(ctx) || !active) return;
      active.context = undefined;
      touch();
      await emit(ctx.signal);
    },

    async toolStart(event, ctx) {
      if (!matches(ctx)) return;
      const id = eventId(event);
      if (!id || terminalToolIds.has(id)) return;
      let internal = defaultInternalTool(event);
      try {
        internal ||= options.isInternalTool?.(event) === true;
      } catch {
        internal = true;
      }
      boundedMapSet(pendingTools, id, internal);
    },

    async toolTerminal(event, ctx) {
      if (!matches(ctx) || !active) return;
      const id = eventId(event);
      if (!id || terminalToolIds.has(id)) return;
      const pendingInternal = pendingTools.get(id);
      let internal = pendingInternal ?? defaultInternalTool(event);
      try {
        internal ||= options.isInternalTool?.(event) === true;
      } catch {
        internal = true;
      }
      if (internal) {
        pendingTools.delete(id);
        boundedSetAdd(terminalToolIds, id);
        return;
      }
      const outcome = terminalOutcome(event);
      if (!outcome) return;
      pendingTools.delete(id);
      boundedSetAdd(terminalToolIds, id);
      active.outcomes.push(outcome);
      if (active.outcomes.length > 32) active.outcomes.shift();
      touch();
      sample(ctx);
      await emit(ctx.signal);
    },

    async compactionStart(_event, ctx) {
      if (!matches(ctx) || !active) return;
      active.context = undefined;
      if (active.activeCompaction === undefined) {
        active.compactionSequence++;
        active.activeCompaction = `attempt:${active.compactionSequence}`;
      }
      touch();
      await emit(ctx.signal);
    },

    async compactionSucceeded(event, ctx) {
      if (!matches(ctx) || !active) return;
      const id = compactionEventId(event) ?? active.activeCompaction;
      if (!id || terminalCompactionIds.has(id)) return;
      boundedSetAdd(terminalCompactionIds, id);
      active.activeCompaction = undefined;
      active.compactionsCommitted++;
      active.hasCompactionMeasurement = true;
      active.context = undefined;
      touch();
      sample(ctx);
      await emit(ctx.signal);
    },

    async compactionFailed(_event, ctx) {
      if (!matches(ctx) || !active || !active.activeCompaction) return;
      const id = active.activeCompaction;
      if (terminalCompactionIds.has(id)) return;
      boundedSetAdd(terminalCompactionIds, id);
      active.activeCompaction = undefined;
      active.compactionsFailed++;
      active.hasCompactionMeasurement = true;
      active.context = undefined;
      touch();
      await emit(ctx.signal);
    },

    read(ctx) {
      return matches(ctx) ? snapshot() : undefined;
    },
  };
  return observer;
}

export function registerPiPhysiology(pi: Pick<PiInstance, 'on'>, options: PiPhysiologyOptions = {}): PiPhysiologyObserver {
  const observer = createPiPhysiologyObserver(options);
  const associate = (ctx: PiContext): void => {
    if (!observer.read(ctx)) return;
    observersByContext.set(ctx as object, observer);
    if (ctx.sessionManager) observersBySessionManager.set(ctx.sessionManager as object, observer);
  };
  pi.on('session_start', async (_event, ctx) => {
    await observer.sessionStart(ctx);
    associate(ctx);
  });
  pi.on('session_shutdown', async (_event, ctx) => {
    await observer.sessionShutdown(ctx);
    observersByContext.delete(ctx as object);
    if (ctx.sessionManager) observersBySessionManager.delete(ctx.sessionManager as object);
  });
  pi.on('input', async (_event, ctx) => { await observer.invalidateContext(ctx); associate(ctx); });
  pi.on('model_select', async (_event, ctx) => { await observer.invalidateContext(ctx); associate(ctx); });
  pi.on('before_agent_start', async (_event, ctx) => {
    if (ctx) { await observer.sampleContext(ctx); associate(ctx); }
  });
  pi.on('agent_start', async (_event, ctx) => { await observer.invalidateContext(ctx); associate(ctx); });
  pi.on('agent_end', async (_event, ctx) => { await observer.sampleContext(ctx); associate(ctx); });
  pi.on('turn_end', async (_event, ctx) => { await observer.sampleContext(ctx); associate(ctx); });
  pi.on('after_provider_response', async (_event, ctx) => { await observer.sampleContext(ctx); associate(ctx); });
  pi.on('tool_execution_start', async (event, ctx) => { await observer.toolStart(event, ctx); associate(ctx); });
  pi.on('tool_execution_end', async (event, ctx) => { await observer.toolTerminal(event, ctx); associate(ctx); });
  pi.on('tool_result', async (event, ctx) => { await observer.toolTerminal(record(event) ?? {}, ctx); associate(ctx); });
  pi.on('session_before_compact', async (event, ctx) => { await observer.compactionStart(event, ctx); associate(ctx); });
  pi.on('session_compact', async (event, ctx) => { await observer.compactionSucceeded(event, ctx); associate(ctx); });
  pi.on('session_compact_failed', async (event, ctx) => { await observer.compactionFailed(event, ctx); associate(ctx); });
  return observer;
}

export function readPiPhysiology(ctx: PiContext): PiRuntimeObservation | undefined {
  const observer = observersByContext.get(ctx as object)
    ?? (ctx.sessionManager ? observersBySessionManager.get(ctx.sessionManager as object) : undefined);
  const observation = observer?.read(ctx);
  if (observation && observer) observersByContext.set(ctx as object, observer);
  return observation;
}
