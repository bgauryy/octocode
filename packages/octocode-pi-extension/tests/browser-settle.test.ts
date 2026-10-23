import { describe, expect, it, vi } from 'vitest';
import { pressKey, settle, settleAfterLoad, waitForText } from '../src/browser/input.js';
import type { BrowserSession } from '../src/browser/session-state.js';
import { answering, stub } from './doubles.js';

const events = () => {
  const listeners = new Set<(method: string, params: Record<string, unknown>) => void>();
  const evaluated: string[] = [];
  return {
    on: (listener: (method: string, params: Record<string, unknown>) => void) => (listeners.add(listener), () => listeners.delete(listener)),
    emit: (method: string, params: Record<string, unknown> = {}) => [...listeners].forEach((listener) => listener(method, params)),
    count: () => listeners.size,
    evaluate: answering((expression: string) => (evaluated.push(expression), true)),
    evaluated,
  };
};

const aborting = (_expression: string, signal?: AbortSignal) =>
  new Promise<never>((_resolve, reject) => signal?.addEventListener('abort', () => reject(signal.reason), { once: true }));

describe('settle', () => {
  const timing = { probeMs: 30, loadMs: 5_000, quietMs: 10, domMaxMs: 100 };

  it('returns right after the probe when nothing navigates, then waits for a quiet DOM', async () => {
    const page = events();
    const started = Date.now();
    await settle(page, undefined, timing);
    expect(Date.now() - started).toBeLessThan(1_000);
    expect(page.count()).toBe(0);
    expect(page.evaluated.at(-1)).toContain('MutationObserver');
  });

  it('waits for a navigation the action started, until its frame stops loading', async () => {
    const page = events();
    let done = false;
    const waiting = settle(page, undefined, timing).then(() => (done = true));
    page.emit('Page.frameStartedLoading', { frameId: 'main' });
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(done).toBe(false);
    page.emit('Page.frameStoppedLoading', { frameId: 'main' });
    await waiting;
    expect(page.count()).toBe(0);
  });

  it('rejects when aborted while waiting for a load, and survives a page torn down during the DOM wait', async () => {
    const page = events();
    const controller = new AbortController();
    const waiting = settle(page, controller.signal, timing);
    page.emit('Page.frameStartedLoading', { frameId: 'main' });
    await new Promise((resolve) => setTimeout(resolve, 40));
    controller.abort(new Error('stop'));
    await expect(waiting).rejects.toThrow('stop');
    expect(page.count()).toBe(0);
    await expect(settle({ ...events(), evaluate: async () => Promise.reject(new Error('context destroyed')) }, undefined, timing)).resolves.toBeUndefined();
  });

  it('rejects when aborted during the final DOM wait', async () => {
    const controller = new AbortController();
    const waiting = settle({ ...events(), evaluate: aborting }, controller.signal, timing);
    await new Promise((resolve) => setTimeout(resolve, 60));
    controller.abort(new Error('stop'));
    await expect(waiting).rejects.toThrow('stop');
  });
});

describe('settleAfterLoad', () => {
  it('waits in the page for quiet content and requests, and reports a timeout', async () => {
    const expressions: string[] = [];
    const quiet = await settleAfterLoad({ evaluate: answering((expression: string) => (expressions.push(expression), true)) }, undefined, 500, 6_000);
    expect(quiet).toMatchObject({ settled: true, failed: false });
    expect(expressions[0]).toContain('MutationObserver');
    expect(expressions[0]).toContain("perf.observe({ type: 'resource' })");
    expect(expressions[0]).toMatch(/setTimeout\(\(\) => finish\(false\), 6000\)/);
    expect(await settleAfterLoad({ evaluate: answering(() => false) })).toMatchObject({ settled: false, failed: false });
  });

  it('reports a wait that could not run as not settled, and rethrows an abort', async () => {
    expect(await settleAfterLoad({ evaluate: async () => Promise.reject(new Error('context destroyed')) })).toMatchObject({ settled: false, failed: true });
    const controller = new AbortController();
    controller.abort(new Error('stop'));
    await expect(settleAfterLoad({ evaluate: async () => Promise.reject(controller.signal.reason) }, controller.signal)).rejects.toThrow('stop');
  });
});

describe('keyboard and waits', () => {
  it('presses combos with modifiers, editing commands and key codes', async () => {
    const sent: Array<Record<string, unknown>> = [];
    const page = stub<BrowserSession['page']>({ send: answering((_method: string, params: Record<string, unknown>) => (sent.push(params), {})) });
    await pressKey(page, 'Control+A');
    expect(sent[0]).toMatchObject({ type: 'rawKeyDown', key: 'A', code: 'KeyA', modifiers: 2, windowsVirtualKeyCode: 65, commands: ['selectAll'] });
    expect(sent[0]).not.toHaveProperty('text');
    await pressKey(page, 'Shift+Tab');
    expect(sent[2]).toMatchObject({ type: 'rawKeyDown', key: 'Tab', modifiers: 8, windowsVirtualKeyCode: 9 });
    await pressKey(page, 'Enter');
    expect(sent[4]).toMatchObject({ type: 'keyDown', key: 'Enter', text: '\r', windowsVirtualKeyCode: 13 });
    await pressKey(page, 'Shift+a');
    expect(sent[6]).toMatchObject({ type: 'keyDown', text: 'A', modifiers: 8 });
    await expect(pressKey(page, 'Hyper+A')).rejects.toThrow(/Unknown modifier "Hyper"/);
  });

  it('waits for text to appear or go, and times out naming it', async () => {
    vi.useFakeTimers();
    try {
      let body = 'loading';
      const page = { evaluate: answering(() => body), untilDialog: <T>(work: Promise<T>) => work };
      setTimeout(() => (body = 'Ready now'), 30);
      const appeared = waitForText(page, 'Ready', false, 2_000, undefined, 10);
      await vi.advanceTimersByTimeAsync(40);
      expect(await appeared).toBe(30);
      expect(await waitForText(page, 'loading', true, 2_000, undefined, 10)).toBe(0);
      const never = expect(waitForText(page, 'never', false, 50, undefined, 10)).rejects.toThrow('Timed out after 0s waiting for "never" to appear.');
      await vi.advanceTimersByTimeAsync(60);
      await never;
    } finally {
      vi.useRealTimers();
    }
  });
});
