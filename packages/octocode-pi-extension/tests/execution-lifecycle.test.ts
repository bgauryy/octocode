import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PiContext, PiInstance } from '../src/types.js';
import {
  EXECUTION_ENTRY_TYPE,
  EXECUTION_TRANSCRIPT_ENTRY_TYPE,
  createExecutionState,
  reduceExecutionEvent,
  type ExecutionEvent,
} from '../src/tools/execution-events.js';
import {
  replayExecutionEvents,
  serializeExecutionEvents,
} from '../src/tools/execution-event-io.js';

const { stores } = vi.hoisted(() => ({
  stores: new WeakMap<object, unknown>(),
}));
// The adapter's host boundary is a store and Pi event callbacks. Exercise the real journal and reducer.
vi.mock('../src/tools/runtime-renderer.js', () => ({
  runtimeStoreFor: (ctx: object) => stores.get(ctx),
}));
import { registerLifecycleUi } from '../src/tools/lifecycle-ui.js';
import { observeExecutionQuestion } from '../src/tools/question-execution.js';
import {
  bindExecutionJournal,
  emitExecution,
  restoreExecutionJournal,
  expireExecutionInteractions,
  syncExecutionEntities,
} from '../src/tools/execution-runtime.js';

function harness(branch: unknown[] = []) {
  const ctx = {
    sessionManager: { getSessionId: () => 'session', getBranch: () => branch },
    hasUI: false,
  } as unknown as PiContext;
  let execution = createExecutionState();
  const statuses = vi.fn();
  const store = {
    getState: () => ({
      phase: 'ready',
      execution,
      setStatus: statuses,
      restoreExecution: (events: ExecutionEvent[]) => {
        execution = events.reduce(reduceExecutionEvent, createExecutionState());
      },
      recordExecution: (event: ExecutionEvent) => {
        execution = reduceExecutionEvent(execution, event);
      },
    }),
  };
  stores.set(ctx, store);
  const events: ExecutionEvent[] = [];
  const persistedTypes: string[] = [];
  const handlers = new Map<string, (event: any, ctx: PiContext) => unknown>();
  const pi = {
    on: (name: string, handler: any) => handlers.set(name, handler),
    appendEntry: (customType: string, data: ExecutionEvent) => {
      events.push(data);
      persistedTypes.push(customType);
      branch.push({ type: 'custom', customType, data });
    },
  } as unknown as PiInstance;
  registerLifecycleUi(pi, vi.fn());
  return {
    ctx,
    pi,
    events,
    persistedTypes,
    store,
    statuses,
    branch,
    fire: async (name: string, event: unknown = {}) => {
      if (name === 'session_start') bindExecutionJournal(pi, ctx);
      return handlers.get(name)?.(event, ctx);
    },
  };
}

describe('Pi execution journal adapter', () => {
  beforeEach(() => vi.useRealTimers());
  it('routes transcript events to a rendered entry type and replays both durable channels', async () => {
    const h = harness();
    await h.fire('session_start');
    emitExecution(
      h.ctx,
      'plan.updated',
      {
        id: 'plan-1',
        phase: 'executing',
        tasks: [{ id: 'step-1', title: 'Implement UI', status: 'doing' }],
      },
      'transcript'
    );
    emitExecution(h.ctx, 'assistant.progress', { message: 'working' }, 'activity');

    expect(h.persistedTypes).toContain(EXECUTION_TRANSCRIPT_ENTRY_TYPE);
    expect(h.persistedTypes).toContain(EXECUTION_ENTRY_TYPE);

    const resumed = harness([...h.branch]);
    restoreExecutionJournal(resumed.ctx);
    expect(resumed.store.getState().execution.plan?.tasks[0]).toMatchObject({
      title: 'Implement UI',
      status: 'doing',
    });
    expect(resumed.store.getState().execution.progress?.message).toBe('working');
  });

  it('publishes worker messages and state transitions once from canonical ledger facts', async () => {
    const h = harness();
    await h.fire('session_start');
    const plan = {
      planId: 'plan-1',
      phase: 'executing',
      tasks: [],
    } as never;
    const worker = {
      agentId: 'agent-1',
      name: 'atlas',
      status: 'running',
      task: 'Implement UI',
      planStep: 'Render progress events',
      startedAt: new Date(1_000).toISOString(),
      updatedAt: new Date(2_000).toISOString(),
      pendingMessages: 0,
      lastMessage: {
        direction: 'from-agent' as const,
        action: 'reply' as const,
        preview: 'Focused tests pass',
        timestamp: 2_000,
      },
    };

    syncExecutionEntities(h.ctx, plan, [worker]);
    const firstCount = h.events.length;
    syncExecutionEntities(h.ctx, plan, [worker]);
    expect(h.events).toHaveLength(firstCount);
    expect(h.events.filter(event => event.type === 'agent.message')).toHaveLength(1);
    expect(h.events.filter(event => event.type === 'agent.transition')).toHaveLength(1);
    expect(h.persistedTypes.filter(type => type === EXECUTION_TRANSCRIPT_ENTRY_TYPE)).toHaveLength(2);

    syncExecutionEntities(h.ctx, plan, [
      { ...worker, status: 'exited', normalizedStatus: 'done', updatedAt: new Date(3_000).toISOString() },
    ]);
    expect(
      h.events.filter(event => event.type === 'agent.transition').at(-1)?.payload
    ).toMatchObject({ from: 'running', to: 'done' });
  });
  it('observes programmatic plan prompts and expires durable requests without inventing approval', async () => {
    const h = harness();
    await h.fire('session_start');
    const expiresAt = Date.now() + 1000;
    await observeExecutionQuestion(
      h.ctx,
      'prompt',
      'Review plan',
      async () => ({
        status: 'pending',
        interaction: {
          interactionId: 'plan-review',
          expiresAt: new Date(expiresAt).toISOString(),
        },
      }),
      'permission'
    );
    expect(
      h.store.getState().execution.interactions['plan-review']
    ).toMatchObject({ kind: 'permission', status: 'waiting', expiresAt });
    expireExecutionInteractions(h.ctx, expiresAt - 1);
    expect(
      h.store.getState().execution.interactions['plan-review'].status
    ).toBe('waiting');
    expireExecutionInteractions(h.ctx, expiresAt);
    expect(
      h.store.getState().execution.interactions['plan-review'].status
    ).toBe('timed_out');
    const count = h.events.length;
    expireExecutionInteractions(h.ctx, expiresAt + 1);
    expect(h.events).toHaveLength(count);
    expect(replayExecutionEvents(serializeExecutionEvents(h.events))).toEqual(
      h.store.getState().execution
    );
  });
  it('keeps a durable headless question waiting and never treats a dismissed prompt as answered', async () => {
    const h = harness();
    await h.fire('session_start');
    await h.fire('turn_start');
    await h.fire('tool_execution_start', {
      toolCallId: 'q',
      toolName: 'askUser',
      args: { queries: [{ question: 'Choose storage' }] },
    });
    const pendingResult = await observeExecutionQuestion(
      h.ctx,
      'q:question:0',
      'Choose storage',
      async () => ({
        content: [],
        details: {
          status: 'pending',
          interaction: { interactionId: 'durable-q' },
        },
      })
    );
    await h.fire('tool_execution_end', {
      toolCallId: 'q',
      result: pendingResult,
      isError: false,
    });
    await h.fire('turn_end');
    expect(
      h.store.getState().execution.interactions['durable-q']
    ).toMatchObject({ status: 'waiting', persistent: true });
    await h.fire('turn_start');
    await h.fire('tool_execution_start', {
      toolCallId: 'q2',
      toolName: 'askUser',
    });
    const cancelledResult = await observeExecutionQuestion(
      h.ctx,
      'q2:question:0',
      'Choose storage',
      async () => ({
        content: [],
        details: { status: 'cancelled' },
      })
    );
    await h.fire('tool_execution_end', {
      toolCallId: 'q2',
      result: cancelledResult,
      isError: false,
    });
    expect(
      h.store.getState().execution.interactions['q2:question:0'].status
    ).toBe('cancelled');
  });

  it('tracks each actual question in a batch and preserves timeout and failure outcomes', async () => {
    const h = harness();
    await h.fire('session_start');
    for (const [index, status] of ['selected', 'timed_out'].entries()) {
      await observeExecutionQuestion(
        h.ctx,
        `q:${index}`,
        `Question ${index}`,
        async () => {
          const waiting = Object.values(
            h.store.getState().execution.interactions
          ).filter(item => item.status === 'waiting');
          expect(waiting.map(item => item.title)).toEqual([
            `Question ${index}`,
          ]);
          return { content: [], details: { status } };
        }
      );
    }
    await expect(
      observeExecutionQuestion(h.ctx, 'q:2', 'Failed prompt', async () => {
        throw new Error('UI closed');
      })
    ).rejects.toThrow('UI closed');
    expect(
      Object.values(h.store.getState().execution.interactions).map(
        item => item.status
      )
    ).toEqual(['answered', 'timed_out', 'failed']);
  });
  it('records headless host events, tool outcomes, skills and messages without private content', async () => {
    const h = harness();
    await h.fire('session_start');
    await h.fire('turn_start');
    await h.fire('message_start', {
      message: { role: 'user', timestamp: 1, content: 'private user content' },
    });
    await h.fire('message_start', {
      message: {
        role: 'assistant',
        timestamp: 2,
        content: [{ type: 'thinking', thinking: 'private reasoning' }],
      },
    });
    await h.fire('tool_execution_start', {
      toolCallId: 's',
      toolName: 'skill',
      args: { queries: [{ name: 'architect', reason: 'private reason' }] },
    });
    await h.fire('tool_execution_end', {
      toolCallId: 's',
      toolName: 'skill',
      result: { details: { name: 'architect', dir: '/skills/architect' } },
      isError: false,
    });
    await h.fire('message_end', {
      message: {
        role: 'assistant',
        timestamp: 2,
        content: 'Finished',
        stopReason: 'stop',
      },
    });
    await h.fire('turn_end', {
      message: {
        usage: { input: 7, output: 3, totalTokens: 10, cost: { total: 0.005 } },
      },
    });
    expect(h.store.getState().execution.usage).toEqual({
      input: 7,
      output: 3,
      cost: 0.005,
    });
    expect(h.events.every(event => event.runId === 'session:main')).toBe(true);
    expect(
      h.events.find(event => event.type === 'tool.started')?.turnId
    ).toBeTruthy();
    expect(h.store.getState().execution.skills.architect.status).toBe('active');
    expect(serializeExecutionEvents(h.events)).not.toContain('private');
    expect(replayExecutionEvents(serializeExecutionEvents(h.events))).toEqual(
      h.store.getState().execution
    );
  });

  it('cancels unfinished tools and drops late completions without false success', async () => {
    const h = harness();
    await h.fire('session_start');
    await h.fire('turn_start');
    await h.fire('tool_execution_start', {
      toolCallId: 'a',
      toolName: 'bash',
      args: { command: 'yarn test' },
    });
    await h.fire('turn_end', { message: { stopReason: 'aborted' } });
    const size = h.events.length;
    await h.fire('tool_execution_end', {
      toolCallId: 'a',
      result: {},
      isError: false,
    });
    expect(h.events).toHaveLength(size);
    expect(h.store.getState().execution.tools.a.status).toBe('cancelled');
  });

  it('records only successful file mutations in a partially failed batch', async () => {
    const h = harness();
    await h.fire('session_start');
    await h.fire('turn_start');
    await h.fire('tool_execution_start', {
      toolCallId: 'files',
      toolName: 'file',
      args: {
        queries: [
          { type: 'write', path: 'new.ts' },
          { type: 'delete', path: 'keep.ts' },
        ],
      },
    });
    await h.fire('tool_execution_end', {
      toolCallId: 'files',
      isError: true,
      result: {
        details: {
          results: [
            {
              index: 0,
              status: 'success',
              result: { path: 'new.ts', operation: 'write', created: true },
            },
            {
              index: 1,
              status: 'failed',
              result: { path: 'keep.ts', operation: 'delete' },
            },
          ],
        },
      },
    });
    expect(h.store.getState().execution.files).toEqual({
      'new.ts': { path: 'new.ts', operation: 'create' },
    });
    expect(h.store.getState().execution.tools.files.status).toBe('failed');
  });

  it('restores the active branch and resolves interrupted work on resume', async () => {
    const original = harness();
    await original.fire('session_start');
    await original.fire('turn_start');
    await original.fire('tool_execution_start', {
      toolCallId: 'a',
      toolName: 'bash',
    });
    const resumed = harness([...original.branch]);
    bindExecutionJournal(resumed.pi, resumed.ctx);
    expect(resumed.store.getState().execution.tools.a.status).toBe('cancelled');
    expect(resumed.events[0].sequence).toBe(
      original.events.at(-1)!.sequence + 1
    );
    expect(resumed.store.getState().execution.activeTurnId).toBeUndefined();
  });

  it('surfaces persistence failures while keeping live execution usable', () => {
    const h = harness();
    h.pi.appendEntry = () => {
      throw new Error('disk full');
    };
    bindExecutionJournal(h.pi, h.ctx);
    emitExecution(h.ctx, 'tool.started', {
      toolCallId: 'a',
      tool: 'bash',
      title: 'Bash yarn test',
    });
    expect(h.statuses).toHaveBeenCalledWith(
      'octocode-event-log',
      expect.stringContaining('unavailable')
    );
    expect(h.store.getState().execution.tools.a.status).toBe('running');
  });

  it('replaces future history on tree navigation and resumes the selected sequence', async () => {
    const h = harness();
    await h.fire('session_start');
    await h.fire('turn_start');
    await h.fire('turn_end');
    const forkPoint = h.branch.length;
    const forkSequence = h.store.getState().execution.sequence;
    await h.fire('turn_start');
    await h.fire('tool_execution_start', {
      toolCallId: 'future',
      toolName: 'bash',
    });
    h.branch.splice(forkPoint);
    restoreExecutionJournal(h.ctx);
    expect(h.store.getState().execution.tools.future).toBeUndefined();
    expect(h.store.getState().execution.activeTurnId).toBeUndefined();
    emitExecution(h.ctx, 'turn.started', {});
    expect(h.events.at(-1)?.sequence).toBe(forkSequence + 1);
    expect(
      replayExecutionEvents(
        serializeExecutionEvents(h.branch.map((entry: any) => entry.data))
      )
    ).toEqual(h.store.getState().execution);
  });
});
