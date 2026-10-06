import { AsyncLocalStorage } from 'node:async_hooks';

/**
 * Locks for tool calls that Pi runs in parallel, and the time a call spent before it ran.
 *
 * Pi runs the calls of one assistant message in parallel unless one of their tools is `executionMode: 'sequential'`,
 * which serializes the whole message (MCP, bash and every other call batched beside it). These locks hold back only
 * the calls that share state: one tool's own calls (`exclusive`), or every interaction that opens a dialog
 * (`withDialog`).
 */

/** The locks the current async call chain holds, so a holder that re-enters its own lock does not wait on itself. */
const held = new AsyncLocalStorage<ReadonlySet<object>>();

function holding<R>(lock: object, run: () => Promise<R>): Promise<R> {
  const current = held.getStore();
  return held.run(new Set([...(current ?? []), lock]), run);
}

/** A FIFO lock: `acquire` resolves once every earlier holder released. */
class Queue {
  private tail: Promise<unknown> = Promise.resolve();
  waiting = 0;

  constructor(private readonly onChange?: () => void) {}

  /** Runs `run` after the earlier holders; `skip` resolves early to run without waiting (an aborted call). */
  async run<R>(run: () => Promise<R>, skip?: Promise<void>): Promise<R> {
    if (held.getStore()?.has(this)) return run();
    let release!: () => void;
    const previous = this.tail;
    this.tail = new Promise<void>((resolve) => (release = resolve));
    this.waiting += 1;
    this.onChange?.();
    try {
      await (skip ? Promise.race([previous, skip]) : previous);
    } finally {
      this.waiting -= 1;
      this.onChange?.();
    }
    try {
      return await holding(this, run);
    } finally {
      // A holder that skipped the line (aborted) must not let the next one past a holder still running.
      void previous.then(release);
    }
  }
}

/**
 * Wrap a tool's `execute` so its calls run one at a time, in call order. A call aborted while it waited still runs and
 * answers its abort the tool's own way (a cancelled result, an error), as it would have unqueued. A call the holder
 * makes itself (through `ctx.executeTool`) runs at once instead of deadlocking on its own lock.
 *
 * Use this, not `executionMode: 'sequential'`, for tools with shared state (one browser page, one store): Pi runs a
 * whole assistant message sequentially when any of its calls is a sequential tool.
 */
export function exclusive<A extends unknown[], R>(execute: (...args: A) => Promise<R>): (...args: A) => Promise<R> {
  const queue = new Queue();
  return (...args: A) => queue.run(() => execute(...args));
}

const dialogListeners = new Set<() => void>();
const dialogs = new Queue(() => {
  for (const listener of dialogListeners) listener();
});

function abortedPromise(signal: AbortSignal | undefined): Promise<void> | undefined {
  if (!signal) return undefined;
  if (signal.aborted) return Promise.resolve();
  return new Promise((resolve) => signal.addEventListener('abort', () => resolve(), { once: true }));
}

/**
 * Run one user interaction (every select, confirm and input of one `askUser` call, one file review, one confirm) while
 * no other interaction shows. Pi's TUI shows one dialog at a time: a second dialog replaces the first without
 * resolving it, which leaves that call waiting for an answer nobody can give. Interactions queue in call order; one
 * nested in another (an interaction that opens a follow-up) runs at once. Aborting a queued interaction runs it at
 * once, so its own aborted dialog returns without showing.
 */
export function withDialog<R>(run: () => Promise<R>, signal?: AbortSignal): Promise<R> {
  return dialogs.run(run, abortedPromise(signal));
}

/** How many interactions wait for another one's dialog to close. */
export const dialogsWaiting = (): number => dialogs.waiting;

/** Called when `dialogsWaiting` may have changed; returns the unsubscribe. */
export function onDialogsChange(listener: () => void): () => void {
  dialogListeners.add(listener);
  return () => dialogListeners.delete(listener);
}

/** Pre-run time of one tool call: its `tool_call` checks, and when they ended. */
interface Checked {
  checkMs: number;
  endedAt: number;
}

const checks = new Map<string, Checked>();
/** Bounded: calls of tools that never reach `takeTiming` (Pi's own, MCP) leave entries, dropped oldest first. */
const CHECKS_MAX = 512;

/** Record how long a call's `tool_call` checks (reservations, the bash guard, PreToolUse hooks) took. */
export function recordCheck(toolCallId: string, checkMs: number, endedAt = Date.now()): void {
  checks.delete(toolCallId);
  checks.set(toolCallId, { checkMs, endedAt });
  if (checks.size > CHECKS_MAX) checks.delete(checks.keys().next().value!);
}

/** Drop a call's check record (its run ended). */
export function forgetCheck(toolCallId: string): void {
  checks.delete(toolCallId);
}

/** Shorter waits are not recorded: every call spends a few milliseconds there. */
export const WAIT_RECORD_MS = 100;

/**
 * The time a call spent before its run started at `startedAt`: `checkMs` in its own checks, `queuedMs` after them (Pi
 * runs no call of a batch until every call's checks are done; a tool lock adds its own wait). Only waits of at least
 * `WAIT_RECORD_MS` are returned.
 */
export function takeTiming(toolCallId: string, startedAt = Date.now()): { checkMs?: number; queuedMs?: number } {
  const entry = checks.get(toolCallId);
  if (!entry) return {};
  checks.delete(toolCallId);
  const queuedMs = Math.max(0, startedAt - entry.endedAt);
  return { ...(entry.checkMs >= WAIT_RECORD_MS ? { checkMs: entry.checkMs } : {}), ...(queuedMs >= WAIT_RECORD_MS ? { queuedMs } : {}) };
}
