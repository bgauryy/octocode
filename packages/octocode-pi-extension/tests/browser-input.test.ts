import { afterEach, describe, expect, it, vi } from 'vitest';
import type { BrowserSession } from '../src/browser/cdp.js';
import { click, drag, fillFields, hover, refForText, settle, typeInto, waitForText } from '../src/browser/input.js';

type Page = BrowserSession['page'];
type Sent = { method: string; params: Record<string, unknown> };

/** A page whose `evaluate` returns `answer(expression)` (throwing it when it is an Error), recording every command sent. */
function fakePage(answer: (expression: string) => unknown) {
  const listeners = new Set<(method: string, params: Record<string, unknown>) => void>();
  const sent: Sent[] = [];
  const expressions: string[] = [];
  const page = {
    on: (listener: (method: string, params: Record<string, unknown>) => void) => (listeners.add(listener), () => listeners.delete(listener)),
    send: async (method: string, params: Record<string, unknown> = {}) => (sent.push({ method, params }), {}),
    evaluate: async (expression: string) => {
      expressions.push(expression);
      const value = answer(expression);
      if (value instanceof Error) throw value;
      return value;
    },
    untilDialog: async <T>(work: Promise<T>) => work,
  };
  return { page: page as unknown as Page, sent, expressions, emit: (method: string, params: Record<string, unknown> = {}) => listeners.forEach((listener) => listener(method, params)) };
}

const inOrder = (...values: unknown[]) => () => values.shift();
const mouse = (sent: Sent[]) => sent.filter((item) => item.method === 'Input.dispatchMouseEvent').map(({ params }) => `${String(params['type'])}@${String(params['x'])},${String(params['y'])}`);

afterEach(() => vi.useRealTimers());

describe('pointing at elements', () => {
  it('clicks an element with no box directly, in the page, without pressing the mouse', async () => {
    const { page, sent, expressions } = fakePage(() => 'hidden');
    expect(await click(page, 4)).toBe('[4] has no visible box; it was clicked directly.');
    expect(expressions[0]).toContain("el.click(); return 'hidden'");
    expect(sent).toEqual([]);
  });

  it('follows an element the mouse move shifted, and notes what covers it', async () => {
    const { page, sent } = fakePage(inOrder({ x: 10, y: 10, covered: '' }, { x: 20, y: 30, covered: 'div#banner "Cookies"' }));
    expect(await click(page, 2)).toBe('Note: div#banner "Cookies" covers [2], so the click landed on it.');
    expect(mouse(sent)).toEqual(['mouseMoved@10,10', 'mouseMoved@20,30', 'mousePressed@20,30', 'mouseReleased@20,30']);
  });

  it('keeps the first point when the element loses its box after the move', async () => {
    const { page, sent } = fakePage(inOrder({ x: 1, y: 2, covered: '' }, 'hidden'));
    expect(await click(page, 1)).toBe('');
    expect(mouse(sent)).toEqual(['mouseMoved@1,2', 'mousePressed@1,2', 'mouseReleased@1,2']);
  });

  it('refuses gone and boxless elements for click, hover and drag', async () => {
    await expect(click(fakePage(() => null).page, 3)).rejects.toThrow('No element [3] — take a new snapshot.');
    const hidden = fakePage(() => 'hidden');
    await expect(hover(hidden.page, 5)).rejects.toThrow('[5] has no visible box to hover.');
    expect(hidden.expressions[0]).not.toContain('el.click()');
    await expect(drag(hidden.page, 5, 6)).rejects.toThrow('[5] has no visible box to drag.');
    const point = { x: 1, y: 1, covered: '' };
    const target = fakePage(inOrder(point, point, 'hidden'));
    await expect(drag(target.page, 5, 6)).rejects.toThrow('[6] has no visible box to drop on.');
    expect(target.sent.some((item) => item.params['type'] === 'mousePressed')).toBe(false);
  });

  it('throws when no visible element contains the text', async () => {
    await expect(refForText(fakePage(() => null).page, 'Drop here')).rejects.toThrow('No visible element contains "Drop here".');
  });
});

describe('typing and filling', () => {
  it('refuses a gone element, a missing option and a non-text target', async () => {
    await expect(typeInto(fakePage(() => null).page, 1, 'x', false)).rejects.toThrow('No element [1] — take a new snapshot.');
    await expect(typeInto(fakePage(() => 'no-option:Red | Blue').page, 1, 'Green', false)).rejects.toThrow('[1] has no option "Green". Options: Red | Blue');
    await expect(typeInto(fakePage(() => 'no-focus').page, 1, 'x', false)).rejects.toThrow(/\[1\] is not a text field, so typing was refused/);
  });

  it('inserts text into a field (not into a select it already set), then presses Enter on submit', async () => {
    const select = fakePage(() => 'select');
    await typeInto(select.page, 1, 'Blue', false);
    expect(select.sent).toEqual([]);
    const field = fakePage(() => 'field');
    await typeInto(field.page, 1, 'hello', true);
    expect(field.sent.map((item) => `${item.method}:${String(item.params['text'] ?? item.params['type'])}`)).toEqual(['Input.insertText:hello', 'Input.dispatchKeyEvent:\r', 'Input.dispatchKeyEvent:keyUp']);
  });

  it('clicks a toggle only when its state differs, and lists the fields that failed', async () => {
    const states: Record<string, unknown> = { '0': 'checked', '1': 'unchecked', '2': null };
    const { page, sent } = fakePage((expression) => {
      const index = /__octoRefs \|\| \[\]\)\[(\d+)\]/.exec(expression)?.[1] ?? '';
      if (expression.includes('aria-checked')) return states[index];
      return { x: 3, y: 3, covered: '' };
    });
    const failures = await fillFields(page, [
      { ref: 1, value: 'false' },
      { ref: 2, value: 'off' },
      { ref: 3, value: 'x' },
    ]);
    expect(failures).toEqual(['[3]: No element [3] — take a new snapshot.']);
    expect(mouse(sent).filter((event) => event.startsWith('mousePressed'))).toEqual(['mousePressed@3,3']);
  });
});

describe('waits that honour abort', () => {
  const timing = { probeMs: 1_000, loadMs: 1_000, quietMs: 1, domMaxMs: 1 };

  it('settle rejects with the reason of a signal aborted before it starts', async () => {
    const controller = new AbortController();
    controller.abort(new Error('stop now'));
    await expect(settle(fakePage(() => true).page, controller.signal, timing)).rejects.toThrow('stop now');
  });

  it('settle rejects with "Aborted" when aborted mid-probe without an Error reason', async () => {
    const controller = new AbortController();
    const { page, expressions } = fakePage(() => true);
    const settling = settle(page, controller.signal, timing);
    controller.abort('string reason');
    await expect(settling).rejects.toThrow(/^Aborted$/);
    expect(expressions).toEqual([]);
  });

  it('waitForText stops polling with the abort reason', async () => {
    const controller = new AbortController();
    const { page } = fakePage(() => 'other text');
    controller.abort(new Error('user stopped'));
    await expect(waitForText(page, 'wanted', false, 10_000, controller.signal, 1)).rejects.toThrow('user stopped');
  });

  it('waitForText treats a failed probe as unknown and times out naming the text', async () => {
    const { page, expressions } = fakePage(() => new Error('Execution context was destroyed'));
    await expect(waitForText(page, 'gone soon', true, 30, undefined, 5)).rejects.toThrow('Timed out after 0s waiting for "gone soon" to disappear.');
    expect(expressions.length).toBeGreaterThan(1);
  });

  it('waitForText resolves undefined when a dialog interrupts the probe', async () => {
    const { page } = fakePage(() => 'x');
    const blocked = Object.assign(Object.create(page) as Page, { untilDialog: async () => undefined });
    expect(await waitForText(blocked, 'x', false, 1_000)).toBeUndefined();
  });
});
