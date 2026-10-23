import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import type { TSchema } from 'typebox';
import type { Component } from '@earendil-works/pi-tui';
import { formatDuration } from './format.js';
import { exclusive, takeTiming } from './locks.js';
import { isRecord } from './util.js';

/**
 * Tool-row timing for the shared presentation (`render.ts`): when each call's run began, the one row whose live clock
 * ticks, and the pre-run waits (`checks`, `queued`) a call spent before it ran.
 */

/** Renderer state kept per tool row (`context.state`). */
export interface ToolRow {
  /** When a row without a call id first drew as running (see `runStart`). */
  startedAt?: number;
  /** The row holds the clock now: it is the newest running row (see `track`). */
  ticking?: boolean;
  /** The row took the clock once; a row that lost it to a newer one never takes it back. */
  clocked?: boolean;
  /** `checks 1.2s · queued 3s`: the time the call waited before it ran (see `timingOf`). */
  waits?: string;
  /** The call was interrupted (Esc, a kill, a requested stop), not failed: drawn `◼` in the warning colour. */
  stopped?: boolean;
}

/** The part of Pi's ToolRenderContext these helpers read. */
export interface RenderContext {
  lastComponent?: Component | undefined;
  state?: unknown;
  invalidate?: () => void;
  executionStarted?: boolean;
  isPartial?: boolean;
  isError?: boolean;
  expanded?: boolean;
  /** Pi's recorded execution time, including when a completed row is restored. */
  durationMs?: number | undefined;
  /** The call's arguments (Pi passes them to both renderers). */
  args?: unknown;
  /** The tool call the row draws; its run start is what `timed` recorded under it. */
  toolCallId?: string;
}

export type Phase = 'pending' | 'running' | 'done' | 'error' | 'stopped';

export function phaseOf(context: RenderContext): Phase {
  if (context.isPartial === false) return context.isError ? 'error' : 'done';
  return context.executionStarted ? 'running' : 'pending';
}

export function rowState(context: RenderContext): ToolRow {
  return isRecord(context.state) ? context.state : {};
}

/**
 * When each call's run began, recorded by `timed`. Pi marks a batch's calls started before their `tool_call` checks
 * and approval dialogs ran, and its final duration counts only the run: the live clock starts here to match it.
 */
const runStarts = new Map<string, number>();
/** Bounded: nested calls (`file` running Pi's tools) record starts no row ever reads; dropped oldest first. */
const RUN_STARTS_MAX = 512;
/** Rows drawn as waiting, each redrawn once when its run begins. */
const waitingRows = new Map<string, () => void>();

function markRun(toolCallId: string): void {
  if (runStarts.has(toolCallId)) return;
  runStarts.set(toolCallId, Date.now());
  if (runStarts.size > RUN_STARTS_MAX) runStarts.delete(runStarts.keys().next().value!);
  const redraw = waitingRows.get(toolCallId);
  waitingRows.delete(toolCallId);
  redraw?.();
}

/** When the row's run began, or undefined while it still waits for its checks, approval or a lock. */
export function runStart(context: RenderContext, state: ToolRow): number | undefined {
  return typeof context.toolCallId === 'string' ? runStarts.get(context.toolCallId) : (state.startedAt ??= Date.now());
}

/**
 * The one ticking row. A changed line above the viewport makes pi-tui clear and redraw the whole scrollback, so a
 * clock ticking on a row that scrolled off would flicker and yank the user out of scrollback every second. Only the
 * newest running row ticks (it is the last row, nearest the viewport); the working line shows how long the run took.
 */
let clock: { state: ToolRow; invalidate: () => void; timer: ReturnType<typeof setInterval> } | undefined;

function stopClock(): void {
  if (!clock) return;
  clearInterval(clock.timer);
  clock.state.ticking = false;
  clock = undefined;
}

/** Stop the clock and forget run starts (on `agent_end` and `session_shutdown`: no row is still running). */
export function stopRowTimers(): void {
  stopClock();
  waitingRows.clear();
  runStarts.clear();
}

/**
 * The newest running row takes the clock; the row that held it drops its time (redrawn once, while it still sits just
 * above the new row) and never ticks again. The clock stops on the final result.
 */
export function track(context: RenderContext, phase: Phase): ToolRow {
  const state = rowState(context);
  const id = context.toolCallId;
  if (phase === 'running') {
    if (id !== undefined && context.invalidate && runStart(context, state) === undefined) waitingRows.set(id, context.invalidate);
    if (!state.clocked && context.invalidate) {
      state.clocked = true;
      const previous = clock?.invalidate;
      stopClock();
      // Redrawn after this pass: invalidating another row from inside this row's render would re-enter Pi's display.
      if (previous) queueMicrotask(previous);
      const invalidate = context.invalidate;
      const timer = setInterval(() => {
        if (runStart(context, state) !== undefined) invalidate();
      }, 1_000);
      timer.unref?.();
      clock = { state, invalidate, timer };
      state.ticking = true;
    }
  } else if (phase !== 'pending') {
    if (clock?.state === state) stopClock();
    if (id !== undefined) {
      runStarts.delete(id);
      waitingRows.delete(id);
    }
  }
  return state;
}

/** Waits shorter than this are not shown: they are routine. */
const WAIT_SHOW_MS = 250;

/**
 * The pre-run waits `timed` recorded (`checks 1.2s` in `tool_call` checks such as PreToolUse
 * hooks, `queued 3s` waiting for the rest of the batch's checks or the tool's lock), for `resultBlock`'s options.
 */
export function timingOf(details: unknown): { waits?: string } {
  const wait = (key: string, label: string): string => {
    const value = isRecord(details) ? details[key] : undefined;
    return typeof value === 'number' && value >= WAIT_SHOW_MS ? `${label} ${formatDuration(value, true)}` : '';
  };
  const waits = [wait('checkMs', 'checks'), wait('queuedMs', 'queued')].filter(Boolean).join(' · ');
  return waits ? { waits } : {};
}

/**
 * Keep `checkMs` / `queuedMs` in the result when a call waited before it ran (see `takeTiming`), and start the row's
 * live clock. Pi owns execution duration; this wrapper records only the approval and queue waits. Existing details
 * are merged, never replaced. Pi passes the tool call id first; that is how the wait is found.
 */
export function timed<A extends unknown[], R>(execute: (...args: A) => Promise<R>): (...args: A) => Promise<R> {
  return async (...args: A) => {
    if (typeof args[0] === 'string') markRun(args[0]);
    const waits = typeof args[0] === 'string' ? takeTiming(args[0]) : {};
    const result = await execute(...args);
    if (!isRecord(result) || Object.keys(waits).length === 0) return result;
    const details = result['details'];
    if (details === undefined || details === null) return { ...result, details: waits };
    if (isRecord(details) && !Array.isArray(details)) return { ...result, details: { ...waits, ...details } };
    return result;
  };
}

/**
 * `pi.registerTool(timedTool({ ... }))`: the tool with its `execute` wrapped by `timed`. Prefer this to
 * `execute: timed(...)`, which loses the parameter types Pi's generic `registerTool` would infer. `exclusive` adds the
 * one-call-at-a-time lock of `exclusive()`; its wait is included in `queuedMs`.
 */
export function timedTool<TParams extends TSchema, TDetails, TState>(
  tool: ToolDefinition<TParams, TDetails, TState>,
  options: { exclusive?: boolean } = {},
): ToolDefinition<TParams, TDetails, TState> {
  const execute = timed(tool.execute.bind(tool));
  if (!options.exclusive) return { ...tool, execute };
  const queued = exclusive(execute);
  // Pi's duration counts the lock wait (it times `execute`), so the live clock starts before the lock too.
  return {
    ...tool,
    execute: (...args: Parameters<typeof execute>) => {
      if (typeof args[0] === 'string') markRun(args[0]);
      return queued(...args);
    },
  };
}
