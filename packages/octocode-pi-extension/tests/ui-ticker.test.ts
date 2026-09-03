import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { hasUiTickSubscriber, setUiTickSubscriber, UI_TICK_MS } from '../src/tui/ui-ticker.js';

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  // Never leak subscriptions between tests — the module is a process singleton.
  setUiTickSubscriber('a', undefined);
  setUiTickSubscriber('b', undefined);
  vi.useRealTimers();
});

test('one shared clock drives all subscribers in the same tick', () => {
  const fired: string[] = [];
  setUiTickSubscriber('a', () => fired.push('a'));
  setUiTickSubscriber('b', () => fired.push('b'));
  expect(hasUiTickSubscriber('a')).toBe(true);
  expect(hasUiTickSubscriber('b')).toBe(true);

  vi.advanceTimersByTime(UI_TICK_MS);
  // Both fire together (coalesced into one repaint), not out of phase.
  expect(fired).toEqual(['a', 'b']);
  expect(vi.getTimerCount()).toBe(1);
});

test('unsubscribing the last subscriber stops the timer entirely', () => {
  setUiTickSubscriber('a', () => {});
  expect(vi.getTimerCount()).toBe(1);
  setUiTickSubscriber('a', undefined);
  expect(hasUiTickSubscriber('a')).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
});

test('a throwing subscriber does not starve the others; self-unsubscribe mid-tick is safe', () => {
  const fired: string[] = [];
  setUiTickSubscriber('a', () => {
    throw new Error('boom');
  });
  setUiTickSubscriber('b', () => {
    fired.push('b');
    setUiTickSubscriber('b', undefined);
  });
  vi.advanceTimersByTime(UI_TICK_MS);
  expect(fired).toEqual(['b']);
  expect(hasUiTickSubscriber('b')).toBe(false);
});
