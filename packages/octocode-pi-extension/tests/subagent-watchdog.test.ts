import { describe, expect, it, vi } from 'vitest';
import { exitReason, runSubagent, USER_STOP } from '../src/subagents/process.js';
import { tmp } from './helpers.js';
import { setup, useScriptedPi } from './subagent-tool-harness.js';

useScriptedPi();

/**
 * Starts `task`, waits until the child shows it is busy (its first progress), lets `ms` pass on a fake clock for the
 * watchdog (its interval and `Date`; the child and its pipes stay real), then aborts and returns how the run ended.
 * A watchdog stop rejects with "Subagent stopped", so the outcome says whether it fired before the abort.
 */
const runThenAbort = async (task: string, id: string, ms: number, limits: { idleMs: number; busyMs: number }) => {
  vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'Date'] });
  try {
    const controller = new AbortController();
    let busy!: () => void;
    const started = new Promise<void>((resolve) => void (busy = resolve));
    const run = runSubagent(task, [], tmp(), undefined, { id }, controller.signal, () => busy(), undefined, limits);
    const outcome = Promise.allSettled([run]);
    await started;
    await vi.advanceTimersByTimeAsync(ms);
    vi.useRealTimers();
    controller.abort();
    return (await outcome)[0];
  } finally {
    vi.useRealTimers();
  }
};

describe('subagent watchdog and exits', () => {
  it('treats a child settling after its answer as busy: no watchdog stop, and the answer stays', async () => {
    // Four idle limits pass while it settles. The answer was complete before the abort: it is kept, and says the run did not settle.
    expect(await runThenAbort('answer then stalled shutdown', 'a-shutdown', 1_200, { idleMs: 300, busyMs: 60_000 })).toMatchObject({
      status: 'fulfilled',
      value: { text: 'COMPLETED REPORT', error: 'Subagent cancelled before its run settled' },
    });
  });

  it('kills a child that emits nothing for the idle limit and says what it last did', async () => {
    await expect(runSubagent('hang', [], tmp(), undefined, { id: 'a-hang' }, undefined, () => undefined, undefined, { idleMs: 300, busyMs: 60_000 }))
      .rejects.toThrow(/^Subagent stopped: no output for .*; last activity: started, .* ago\. Its process tree was killed\.$/);
  });

  it('never stops a child for a silent running tool (the tool has its own deadline) nor for its run time', async () => {
    expect(await runThenAbort('slow tool', 'a-slow', 1_500, { idleMs: 300, busyMs: 60_000 })).toMatchObject({ status: 'rejected', reason: { message: 'Subagent cancelled' } });
  });

  it('stops a busy child once a call runs past its own deadline (busyMs when it declares none)', async () => {
    // On the watchdog's fake clock, like runThenAbort: the idle limit counts from spawn, so on a real clock a child that
    // takes longer than `idleMs` to start (a loaded machine) is stopped as idle before its tool call is ever seen.
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'Date'] });
    try {
      let busy!: () => void;
      const started = new Promise<void>((resolve) => void (busy = resolve));
      const run = runSubagent('slow tool', [], tmp(), undefined, { id: 'a-slow-hung' }, undefined, () => busy(), undefined, { idleMs: 200, busyMs: 200 });
      const outcome = Promise.allSettled([run]);
      await started;
      await vi.advanceTimersByTimeAsync(1_000); // past the call's deadline (busyMs + idleMs) and a watchdog tick
      vi.useRealTimers();
      const settled = (await outcome)[0];
      if (settled.status !== 'rejected') throw new Error(`expected the run to be rejected, got ${settled.status}`);
      expect(settled.reason.message).toMatch(/^Subagent stopped: → bash sleep 60 ran past its own deadline; last output .* ago\. Its process tree was killed\.$/);
    } finally {
      vi.useRealTimers();
    }
  });

  it.skipIf(process.platform === 'win32')('gives a stop the user asked for a short grace: a child ignoring SIGTERM is killed in about a second', async () => {
    const controller = new AbortController();
    let ignoring!: () => void;
    const ignores = new Promise<void>((resolve) => void (ignoring = resolve));
    const run = runSubagent('stubborn', [], tmp(), undefined, { id: 'a-stubborn' }, controller.signal, () => ignoring());
    // The child reported that it ignores SIGTERM: only now is a stop a test of the grace.
    await ignores;
    const started = Date.now();
    controller.abort(USER_STOP);
    await expect(run).rejects.toThrow('Subagent cancelled');
    expect(Date.now() - started).toBeLessThan(3_000);
  }, 10_000);

  it('names the signal that killed a child instead of "code null"', () => {
    expect(exitReason(null, 'SIGTERM')).toBe('was killed by SIGTERM');
    expect(exitReason(3, null)).toBe('exited with code 3');
  });

  it('lets /agents kill stop a foreground run without aborting the turn', async () => {
    const s = setup();
    const running = s.run({ task: 'slow foreground' });
    let id = '';
    await vi.waitFor(async () => {
      await s.commands.get('agents').handler('kill', s.ctx);
      id = /Running: (\S+)/.exec(s.ctx.ui.notes.at(-1)!.message)?.[1] ?? '';
      expect(id).not.toBe('');
    }, { timeout: 5_000 });
    await s.commands.get('agents').handler(`kill ${id}`, s.ctx);
    await expect(running).rejects.toThrow(`Subagent ${id} was stopped by the user.`);
  }, 15_000);
});
