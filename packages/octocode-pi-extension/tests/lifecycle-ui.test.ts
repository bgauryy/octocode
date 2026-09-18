import { describe, expect, it, vi } from 'vitest';
import type { PiContext, PiInstance } from '../src/types.js';
import { registerLifecycleUi } from '../src/tools/lifecycle-ui.js';
import {
  bindRuntimeRenderer,
} from '../src/tools/runtime-renderer.js';
import { runtimeActivityPresentation } from '../src/tools/activity-presentation.js';
import { activeExecutionTools } from '../src/tools/execution-event-io.js';
import { createRuntimeStore } from '../src/tools/runtime-store.js';
import { emitExecution } from '../src/tools/execution-runtime.js';

function harness() {
  const handlers = new Map<
    string,
    (event: any, ctx: PiContext) => Promise<void>
  >();
  const notify = vi.fn();
  const repaint = vi.fn();
  const ctx: PiContext = { hasUI: true, ui: { notify }, sessionManager: { getSessionId: () => 'lifecycle-test' } as PiContext['sessionManager'] };
  const store = createRuntimeStore();
  const dispose = bindRuntimeRenderer(ctx, store);
  registerLifecycleUi(
    {
      on: (name: string, fn: any) => handlers.set(name, fn),
    } as unknown as PiInstance,
    repaint
  );
  return {
    ctx,
    store,
    notify,
    repaint,
    dispose,
    fire: (name: string, event = {}) => handlers.get(name)?.(event, { ...ctx }),
  };
}

describe('lifecycle UI', () => {
  it('tracks overlapping calls independently and restores plan activity without exposing arguments', async () => {
    const h = harness();
    h.store.getState().setActivity({ kind: 'verifying', planScope: 'plan' });
    await h.fire('tool_execution_start', {
      toolCallId: 'a',
      toolName: 'MCPTool',
      args: { secret: 'private-value' },
    });
    await h.fire('tool_execution_start', { toolCallId: 'b', toolName: 'bash' });
    expect(runtimeActivityPresentation(h.store.getState()).status).toBe(
      'Bash · 2 tools'
    );
    await h.fire('tool_execution_end', {
      toolCallId: 'a',
      toolName: 'MCPTool',
    });
    expect(runtimeActivityPresentation(h.store.getState()).status).toBe('Bash');
    await h.fire('tool_execution_end', { toolCallId: 'b', toolName: 'bash' });
    expect(runtimeActivityPresentation(h.store.getState()).status).toBe(
      'Verifying'
    );
    expect(JSON.stringify(h.store.getState())).not.toContain('private-value');
    expect(h.notify).not.toHaveBeenCalled();
    h.dispose();
  });

  it('gives input and compaction clear priority, then clears aborted work', async () => {
    const h = harness();
    await h.fire('tool_execution_start', {
      toolCallId: 'ask',
      toolName: 'askUser',
    });
    emitExecution(h.ctx, 'question.requested', {
      id: 'ask:question:0',
      title: 'Answer the pending question',
    });
    expect(runtimeActivityPresentation(h.store.getState())).toMatchObject({
      visible: false,
      status: 'Input needed · Answer the pending question',
      token: 'warning',
    });
    await h.fire('session_before_compact');
    expect(runtimeActivityPresentation(h.store.getState()).status).toBe(
      'Input needed · Answer the pending question'
    );
    await h.fire('session_compact_failed', { aborted: true });
    expect(runtimeActivityPresentation(h.store.getState()).status).toBe(
      'Input needed · Answer the pending question'
    );
    await h.fire('turn_end');
    expect(
      runtimeActivityPresentation(h.store.getState()).status
    ).toBeUndefined();
    expect(activeExecutionTools(h.store.getState().execution)).toEqual([]);
    h.dispose();
  });

  it('alerts once per failing tool per turn and never repeats private error content', async () => {
    const h = harness();
    for (const id of ['a', 'b']) {
      await h.fire('tool_execution_start', {
        toolCallId: id,
        toolName: 'bash',
      });
      await h.fire('tool_execution_end', {
        toolCallId: id,
        toolName: 'bash',
        isError: true,
        result: 'private output',
      });
    }
    expect(h.notify.mock.calls).toEqual([
      ['Bash failed. See its result for details.', 'warning'],
    ]);
    await h.fire('turn_end');
    await h.fire('tool_execution_start', { toolCallId: 'c', toolName: 'bash' });
    await h.fire('tool_execution_end', {
      toolCallId: 'c',
      toolName: 'bash',
      isError: true,
    });
    expect(h.notify).toHaveBeenCalledTimes(2);
    h.dispose();
  });

  it('ignores late completions and never touches a replaced session context on shutdown', async () => {
    const h = harness();
    await h.fire('tool_execution_end', {
      toolCallId: 'old',
      toolName: 'bash',
      isError: true,
    });
    expect(h.notify).not.toHaveBeenCalled();
    await h.fire('session_shutdown', { reason: 'new' });
    expect(h.repaint).not.toHaveBeenCalled();
    h.dispose();
  });
});
