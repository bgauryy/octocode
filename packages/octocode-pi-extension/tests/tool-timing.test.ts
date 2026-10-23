import type { ExtensionToolContext } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { resultBlock, toolHeader } from '../src/shared/render.js';
import { stopRowTimers, timed, timedTool, timingOf, type ToolRow } from '../src/shared/tool-timing.js';
import { recordCheck } from '../src/shared/locks.js';
import { stub } from './doubles.js';
import { rendered, theme } from './fake-pi.js';

const row = (overrides: Record<string, unknown> = {}) => ({ state: {} as ToolRow, invalidate: vi.fn(), executionStarted: false, isPartial: true, isError: false, expanded: false, ...overrides });

/** A tool with no parameters, and the context its calls run in: the timing wrappers read neither. */
const parameters = Type.Object({});
const ctx = stub<ExtensionToolContext>({});

afterEach(() => vi.useRealTimers());

describe('the live row clock', () => {
  it('ticks while running and stops the ticker on the final result', () => {
    vi.useFakeTimers();
    const context = row({ executionStarted: true });
    const header = toolHeader(theme, context, 'Bash', 'sleep 2');
    expect(rendered(header)).toBe('● Bash(sleep 2) · 0ms');
    expect(context.state.ticking).toBe(true);
    vi.advanceTimersByTime(2_100);
    expect(context.invalidate).toHaveBeenCalledTimes(2);
    const doneContext = { ...context, isPartial: false, durationMs: 1_900 };
    const done = toolHeader(theme, { ...doneContext, lastComponent: header }, 'Bash', 'sleep 2');
    expect(done).toBe(header);
    expect(rendered(done)).toBe('● Bash(sleep 2) · 1.9s');
    expect(context.state.ticking).toBe(false);
    vi.advanceTimersByTime(5_000);
    expect(context.invalidate).toHaveBeenCalledTimes(2);
    expect(rendered(toolHeader(theme, doneContext, 'Bash', 'sleep 2'))).toBe('● Bash(sleep 2) · 1.9s');
  });

  it('ticks only the newest running row: an older row drops its time once and never ticks again', async () => {
    vi.useFakeTimers();
    try {
      const older = row({ executionStarted: true });
      toolHeader(theme, older, 'Bash', 'build');
      vi.advanceTimersByTime(1_100);
      expect(older.invalidate).toHaveBeenCalledTimes(1);
      expect(rendered(toolHeader(theme, older, 'Bash', 'build'))).toBe('● Bash(build) · 1.1s');
      const newer = row({ executionStarted: true });
      toolHeader(theme, newer, 'Read', 'a.ts');
      await Promise.resolve();
      // One redraw to drop the time while the row still sits above the new one; then it stays put.
      expect(older.invalidate).toHaveBeenCalledTimes(2);
      expect(rendered(toolHeader(theme, older, 'Bash', 'build'))).toBe('● Bash(build)');
      vi.advanceTimersByTime(3_000);
      expect(older.invalidate).toHaveBeenCalledTimes(2);
      expect(newer.invalidate).toHaveBeenCalledTimes(3);
      // The newest finishing does not hand the clock back: the older row may have scrolled off by then.
      toolHeader(theme, { ...newer, isPartial: false, durationMs: 3_000 }, 'Read', 'a.ts');
      vi.advanceTimersByTime(3_000);
      expect(older.invalidate).toHaveBeenCalledTimes(2);
      expect(rendered(toolHeader(theme, older, 'Bash', 'build'))).toBe('● Bash(build)');
    } finally {
      stopRowTimers();
      vi.useRealTimers();
    }
  });

});

describe('a row waiting to run', () => {
  it('says waiting while the call waits for its checks or approval, and starts the clock when its run begins', async () => {
    vi.useFakeTimers();
    try {
      const context = row({ executionStarted: true, toolCallId: 'call-wait' });
      expect(rendered(toolHeader(theme, context, 'Bash', 'ls'))).toBe('○ Bash(ls) · waiting');
      vi.advanceTimersByTime(5_000);
      // Nothing to tick while waiting.
      expect(context.invalidate).not.toHaveBeenCalled();
      expect(rendered(toolHeader(theme, context, 'Bash', 'ls'))).toBe('○ Bash(ls) · waiting');
      const run = timed(async (_id: string) => {
        await new Promise((resolve) => setTimeout(resolve, 2_000));
        return { content: [] };
      });
      const done = run('call-wait');
      // The run's start redraws the row at once.
      expect(context.invalidate).toHaveBeenCalledTimes(1);
      expect(rendered(toolHeader(theme, context, 'Bash', 'ls'))).toBe('● Bash(ls) · 0ms');
      await vi.advanceTimersByTimeAsync(2_000);
      await done;
      expect(rendered(toolHeader(theme, context, 'Bash', 'ls'))).toBe('● Bash(ls) · 2.0s');
      // Pi's duration (the run only) matches the live clock.
      expect(rendered(toolHeader(theme, { ...context, isPartial: false, durationMs: 2_000 }, 'Bash', 'ls'))).toBe('● Bash(ls) · 2.0s');
    } finally {
      stopRowTimers();
      vi.useRealTimers();
    }
  });

  it('starts an exclusive tool clock before its lock, as Pi times execute', async () => {
    const tool = timedTool({ name: 't', label: 'T', description: '', parameters, execute: async () => ({ content: [], details: {} }) }, { exclusive: true });
    const context = row({ executionStarted: true, toolCallId: 'call-lock' });
    toolHeader(theme, context, 'T', 'x');
    const pending = tool.execute('call-lock', {}, undefined, undefined, ctx);
    expect(rendered(toolHeader(theme, context, 'T', 'x'))).toMatch(/^● T\(x\) · \d+ms$/);
    await pending;
    stopRowTimers();
  });
});

describe('timed', () => {
  it('leaves results untouched when no pre-run waits were recorded', async () => {
    const plain = { content: [], details: undefined };
    expect(await timed(async () => plain)()).toBe(plain);
    const detailed = { content: [], details: { value: 3 } };
    expect(await timed(async () => detailed)()).toBe(detailed);
  });

  it('lets errors propagate', async () => {
    await expect(timed(async () => Promise.reject(new Error('boom')))()).rejects.toThrow('boom');
  });

  it('adds the check and queue waits recorded for the call id, once', async () => {
    recordCheck('call-wait', 400, Date.now() - 300);
    const run = timed(async (_id: string): Promise<{ content: never[]; details: Record<string, unknown> }> => ({ content: [], details: { kept: true } }));
    const first = await run('call-wait');
    expect(first.details.checkMs).toBe(400);
    expect(first.details.queuedMs).toBeGreaterThanOrEqual(300);
    expect(first.details['kept']).toBe(true);
    expect(first.details).not.toHaveProperty('durationMs');
    const again = await run('call-wait');
    expect(again.details.checkMs).toBeUndefined();
    recordCheck('call-fast', 5);
    const fast = await run('call-fast');
    expect(fast.details.checkMs).toBeUndefined();
    expect(fast.details.queuedMs).toBeUndefined();
  });
});

describe('timingOf', () => {
  it('reads only waits and combines them with Pi duration in the header', async () => {
    expect(timingOf(undefined)).toEqual({});
    expect(timingOf({ durationMs: 12 })).toEqual({});
    const timing = timingOf({ durationMs: 1200, checkMs: 3400, queuedMs: 150 });
    expect(timing).toEqual({ waits: 'checks 3.4s' });
    const context = { state: {} as ToolRow, isPartial: false, executionStarted: true, durationMs: 900, invalidate: vi.fn() };
    resultBlock(theme, context, { summary: 'ok', ...timing });
    await Promise.resolve();
    expect(context.invalidate).toHaveBeenCalledOnce();
    resultBlock(theme, context, { summary: 'ok', ...timing });
    await Promise.resolve();
    expect(context.invalidate).toHaveBeenCalledOnce();
    const header = rendered(toolHeader(theme, context, 'Tool', 'x'));
    expect(header).toContain('900ms');
    expect(header).not.toContain('1.2s');
    expect(header).toContain('checks 3.4s');
    expect(header).not.toContain('queued');
    const slowQueue = { state: {} as ToolRow, isPartial: false, executionStarted: true };
    resultBlock(theme, slowQueue, { summary: 'ok', ...timingOf({ durationMs: 10, queuedMs: 2000 }) });
    expect(rendered(toolHeader(theme, slowQueue, 'Tool', 'x'))).toContain('queued 2.0s');
  });
});

describe('timedTool', () => {
  it('records the exclusive lock wait without adding an execution duration', async () => {
    vi.useFakeTimers();
    let release!: () => void;
    let entered!: () => void;
    const held = new Promise<void>((resolve) => { release = resolve; });
    const started = new Promise<void>((resolve) => { entered = resolve; });
    const tool = timedTool({
      name: 't', label: 'T', description: '', parameters,
      execute: async (id) => {
        if (id === 'holder') {
          entered();
          await held;
        }
        return { content: [], details: { id } };
      },
    }, { exclusive: true });
    const first = tool.execute('holder', {}, undefined, undefined, ctx);
    await started;
    recordCheck('waiter', 400);
    const second = tool.execute('waiter', {}, undefined, undefined, ctx);
    await vi.advanceTimersByTimeAsync(300);
    release();
    const [, result] = await Promise.all([first, second]);
    expect(result.details).toEqual({ id: 'waiter', checkMs: 400, queuedMs: 300 });
  });

  it('is opt-in on timedTool and never sets the batch-wide sequential mode', async () => {
    let running = 0;
    let peak = 0;
    const tool = (exclusiveCalls: boolean) =>
      timedTool({ name: 't', label: 'T', description: '', parameters, execute: async () => {
        peak = Math.max(peak, ++running);
        await new Promise((resolve) => setTimeout(resolve, 5));
        running -= 1;
        return { content: [], details: {} };
      } }, { exclusive: exclusiveCalls });
    const serial = tool(true);
    expect(serial.executionMode).toBeUndefined();
    await Promise.all([serial.execute('1', {}, undefined, undefined, ctx), serial.execute('2', {}, undefined, undefined, ctx)]);
    expect(peak).toBe(1);
    const parallel = tool(false);
    await Promise.all([parallel.execute('1', {}, undefined, undefined, ctx), parallel.execute('2', {}, undefined, undefined, ctx)]);
    expect(peak).toBe(2);
  });
});
