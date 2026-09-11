import type { AssembledContextV1 } from './context-segments.js';
import { createStore, type StoreApi } from 'zustand/vanilla';
import { createExecutionState, reduceExecutionEvent, type ExecutionEvent, type ExecutionState } from './execution-events.js';

export type RuntimePhase = 'idle' | 'initializing' | 'ready' | 'degraded' | 'failed' | 'disposing' | 'disposed';
export type RuntimeTaskStatus = 'idle' | 'running' | 'ready' | 'degraded' | 'failed';
export type RuntimeNoticeLevel = 'info' | 'warning' | 'error';
export type RuntimeBackgroundJobStatus = 'running' | 'succeeded' | 'failed' | 'timed_out' | 'killed';

export interface RuntimeBackgroundJobState {
  id: string;
  title: string;
  status: RuntimeBackgroundJobStatus;
  startedAt: number;
  endedAt?: number;
  updatedAt: number;
  exitCode?: number | null;
}

export type ForegroundActivity =
  | { kind: 'idle' }
  | { kind: 'thinking'; since: number }
  | { kind: 'researching'; since: number; planScope: string; detail?: string }
  | { kind: 'awaiting_input'; since: number; planScope: string; question: string }
  | { kind: 'planning'; since: number; planScope: string; detail?: string }
  | { kind: 'reviewing'; since: number; planScope: string; revision?: string }
  | { kind: 'awaiting_start'; since: number; planScope: string; revision: string }
  | { kind: 'ready_to_work'; since: number; planScope: string; label: string }
  | { kind: 'working'; since: number; planScope?: string; stepId?: string; label: string }
  | { kind: 'verifying'; since: number; planScope: string; label?: string }
  | { kind: 'blocked'; since: number; label: string }
  | { kind: 'complete'; since: number; label?: string }
  | { kind: 'failed'; since: number; label: string };

export type ForegroundActivityInput = ForegroundActivity extends infer Activity
  ? Activity extends { since: number }
    ? Omit<Activity, 'since'>
    : Activity
  : never;

export interface RuntimeTaskState {
  status: RuntimeTaskStatus;
  critical?: boolean;
  message?: string;
  startedAt?: number;
  finishedAt?: number;
  error?: string;
}

export interface RuntimeMcpState {
  status: RuntimeTaskStatus;
  source?: 'cache' | 'generated' | 'deterministic' | 'none';
  servers: number;
  tools: number;
  totalServers: number;
  completedServers: number;
  failedServers: string[];
  currentServer?: string;
  message?: string;
}

export interface RuntimeNotice {
  id: number;
  level: RuntimeNoticeLevel;
  message: string;
}

export interface RuntimeContextState {
  /** Projection readiness; provider cache hits are separate usage receipts. */
  status: 'pending' | 'ready' | 'stale';
  mode: 'exact' | 'compact';
  systemPromptChars: number;
  mcpChars: number;
  dynamicChars: number;
  directToolChars: number;
  providerSubtotalChars: number;
  estimatedTokens: number;
  /** Payload-free character estimates, distinct from provider billing. */
  contextAwarenessEstimates?: AssembledContextV1['estimates'];
  /** Latest delivered body estimate only; replay replaces rather than accumulating. */
  lastPeerDeliveryEstimate?: { method: 'ceil-utf16-chars/4'; sequence: number; tokens: number };
  mcpServers: number;
  mcpTools: number;
  skills: number;
  promptDigest?: string;
  liveDigest?: string;
}

export interface RuntimeFooterState {
  sessionStartedAt: number;
  activeTurnStartedAt?: number;
  lastTurnMs?: number;
  completedTurns: number;
  gitDirty?: boolean;
  gitDirtyFiles?: number;
  gitAdditions?: number;
  gitDeletions?: number;
  usage?: { tokens?: number; contextWindow: number };
  githubAuth: { status: 'checking' | 'authenticated' | 'missing' | 'error'; source?: string; message?: string };
}

export interface RuntimeState {
  generation: number;
  phase: RuntimePhase;
  stage: string;
  startedAt?: number;
  readyAt?: number;
  tasks: Record<string, RuntimeTaskState>;
  statuses: Record<string, string | undefined>;
  mcp: RuntimeMcpState;
  context: RuntimeContextState;
  footer: RuntimeFooterState;
  backgroundJobs: RuntimeBackgroundJobState[];
  activity: ForegroundActivity;
  execution: ExecutionState;
  recordExecution(event: ExecutionEvent): void;
  restoreExecution(events: readonly ExecutionEvent[]): void;
  notice?: RuntimeNotice;
  begin(stage?: string): number;
  setStage(stage: string): void;
  startTask(name: string, message?: string, critical?: boolean): void;
  finishTask(name: string, message?: string): void;
  degradeTask(name: string, error: unknown, message?: string): void;
  failTask(name: string, error: unknown, message?: string): void;
  setMcp(patch: Partial<RuntimeMcpState>): void;
  setContext(patch: Partial<RuntimeContextState>): void;
  setFooter(patch: Partial<RuntimeFooterState>): void;
  setBackgroundJobs(jobs: readonly RuntimeBackgroundJobState[], generation?: number): void;
  setActivity(activity: ForegroundActivityInput): void;
  setStatus(name: string, text: string | undefined): void;
  announce(message: string, level?: RuntimeNoticeLevel): void;
  ready(message?: string): void;
  degraded(message: string): void;
  failed(error: unknown): void;
  disposing(): void;
  disposed(): void;
}

export type RuntimeStore = StoreApi<RuntimeState>;

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function initialState(): Pick<RuntimeState, 'generation' | 'phase' | 'stage' | 'tasks' | 'statuses' | 'mcp' | 'context' | 'footer' | 'backgroundJobs' | 'activity' | 'execution'> {
  return {
    generation: 0,
    phase: 'idle',
    stage: 'idle',
    tasks: {},
    statuses: {},
    mcp: {
      status: 'idle',
      servers: 0,
      tools: 0,
      totalServers: 0,
      completedServers: 0,
      failedServers: [],
    },
    context: {
      status: 'pending',
      mode: 'exact',
      systemPromptChars: 0,
      mcpChars: 0,
      dynamicChars: 0,
      directToolChars: 0,
      providerSubtotalChars: 0,
      estimatedTokens: 0,
      mcpServers: 0,
      mcpTools: 0,
      skills: 0,
    },
    footer: {
      sessionStartedAt: 0,
      completedTurns: 0,
      githubAuth: { status: 'checking' },
    },
    backgroundJobs: [],
    activity: { kind: 'idle' },
    execution: createExecutionState(),
  };
}

function sameBackgroundJobs(
  left: readonly RuntimeBackgroundJobState[],
  right: readonly RuntimeBackgroundJobState[],
): boolean {
  return left.length === right.length && left.every((job, index) => {
    const other = right[index];
    return other !== undefined
      && job.id === other.id
      && job.title === other.title
      && job.status === other.status
      && job.startedAt === other.startedAt
      && job.endedAt === other.endedAt
      && job.updatedAt === other.updatedAt
      && job.exitCode === other.exitCode;
  });
}

export function createRuntimeStore(now: () => number = Date.now): RuntimeStore {
  let noticeId = 0;
  return createStore<RuntimeState>()((set, get) => ({
    ...initialState(),
    restoreExecution: (events) => set((state) => {
      const execution = events.reduce(reduceExecutionEvent, createExecutionState());
      return { execution, footer: { ...state.footer,
        sessionStartedAt: execution.startedAt ?? state.footer.sessionStartedAt,
        activeTurnStartedAt: execution.activeTurnStartedAt,
        completedTurns: execution.completedTurns,
        lastTurnMs: execution.lastTurnMs,
      } };
    }),
    recordExecution: (event) => set((state) => {
      const execution = reduceExecutionEvent(state.execution, event);
      if (execution === state.execution) return state;
      return { execution, footer: {
        ...state.footer,
        sessionStartedAt: execution.startedAt ?? state.footer.sessionStartedAt,
        activeTurnStartedAt: execution.activeTurnStartedAt,
        completedTurns: execution.completedTurns,
        lastTurnMs: execution.lastTurnMs,
      } };
    }),
    begin: (stage = 'starting') => {
      const generation = get().generation + 1;
      set({
        ...initialState(),
        generation,
        phase: 'initializing',
        stage,
        startedAt: now(),
        readyAt: undefined,
        notice: undefined,
      });
      return generation;
    },
    setStage: (stage) => set({ stage }),
    startTask: (name, message, critical = false) => set((state) => ({
      tasks: {
        ...state.tasks,
        [name]: { status: 'running', message, critical, startedAt: now() },
      },
    })),
    finishTask: (name, message) => set((state) => ({
      tasks: {
        ...state.tasks,
        [name]: {
          ...state.tasks[name],
          status: 'ready',
          message: message ?? state.tasks[name]?.message,
          finishedAt: now(),
          error: undefined,
        },
      },
    })),
    degradeTask: (name, error, message) => set((state) => {
      const taskMessage = message ?? state.tasks[name]?.message;
      return {
        tasks: {
          ...state.tasks,
          [name]: {
            ...state.tasks[name],
            status: 'degraded',
            message: taskMessage,
            finishedAt: now(),
            error: errorText(error),
          },
        },
        ...(state.phase === 'ready'
          ? { phase: 'degraded' as const, stage: 'ready with warnings', notice: { id: ++noticeId, level: 'warning' as const, message: `Octocode ready with warnings: ${taskMessage ?? name}` } }
          : {}),
      };
    }),
    failTask: (name, error, message) => set((state) => {
      const taskMessage = message ?? state.tasks[name]?.message;
      return {
        tasks: {
          ...state.tasks,
          [name]: {
            ...state.tasks[name],
            status: 'failed',
            message: taskMessage,
            finishedAt: now(),
            error: errorText(error),
          },
        },
        ...(state.phase === 'ready' || state.phase === 'degraded'
          ? { phase: 'failed' as const, stage: 'runtime task failed', notice: { id: ++noticeId, level: 'error' as const, message: `Octocode runtime task failed: ${taskMessage ?? name}` } }
          : {}),
      };
    }),
    setMcp: (patch) => set((state) => ({ mcp: { ...state.mcp, ...patch } })),
    setContext: (patch) => set((state) => ({ context: { ...state.context, ...patch } })),
    setFooter: (patch) => set((state) => ({ footer: { ...state.footer, ...patch } })),
    setBackgroundJobs: (jobs, generation = get().generation) => set((state) => {
      if (state.generation !== generation || sameBackgroundJobs(state.backgroundJobs, jobs)) return state;
      return { backgroundJobs: jobs.map((job) => ({ ...job })) };
    }),
    setActivity: (activity) => set({
      activity: activity.kind === 'idle'
        ? activity
        : { ...activity, since: now() } as ForegroundActivity,
    }),
    setStatus: (name, text) => set((state) => {
      if (state.statuses[name] === text) return state;
      return { statuses: { ...state.statuses, [name]: text } };
    }),
    announce: (message, level = 'info') => set({ notice: { id: ++noticeId, level, message } }),
    ready: (message = 'Octocode ready') => set({
      phase: 'ready',
      stage: 'ready',
      readyAt: now(),
      notice: { id: ++noticeId, level: 'info', message },
    }),
    degraded: (message) => set({
      phase: 'degraded',
      stage: 'ready with warnings',
      readyAt: now(),
      notice: { id: ++noticeId, level: 'warning', message },
    }),
    failed: (error) => set({
      phase: 'failed',
      stage: 'initialization failed',
      notice: { id: ++noticeId, level: 'error', message: `Octocode initialization failed: ${errorText(error)}` },
    }),
    disposing: () => set({ phase: 'disposing', stage: 'shutting down' }),
    disposed: () => set({ phase: 'disposed', stage: 'stopped', statuses: {} }),
  }));
}

export async function runRuntimeTask<T>(
  store: RuntimeStore,
  name: string,
  message: string,
  run: () => Promise<T> | T,
  opts: { critical?: boolean; readyMessage?: string; signal?: AbortSignal; generation?: number } = {},
): Promise<T | undefined> {
  const generation = opts.generation ?? store.getState().generation;
  const canPublish = (): boolean => !opts.signal?.aborted
    && store.getState().generation === generation
    && store.getState().phase !== 'disposing'
    && store.getState().phase !== 'disposed';
  if (!canPublish()) return undefined;
  store.getState().startTask(name, message, opts.critical);
  try {
    const value = await run();
    if (canPublish()) store.getState().finishTask(name, opts.readyMessage);
    return value;
  } catch (error) {
    if (!canPublish()) return undefined;
    if (opts.critical) {
      store.getState().failTask(name, error, message);
      throw error;
    }
    store.getState().degradeTask(name, error, message);
    return undefined;
  }
}
