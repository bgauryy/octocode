import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CdpHandler, CdpStub } from './fixtures/cdp-stub.js';
import { useBrowserHarness } from './browser-harness.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));
vi.mock('node:dns/promises', () => ({ lookup: async () => [{ address: '93.184.216.34', family: 4 }] })); // public: no network

const { stub, register } = useBrowserHarness();

beforeEach(() => {
  launch.mockReset();
});

type FormOptions = { html5Drag?: boolean; late?: () => boolean; restless?: boolean; typing?: string };

/** The stub's answer to evaluating `expression` once no element ref decided it. */
function formAnswer(expression: string, options: FormOptions): unknown {
  if (expression.includes('smallest visible element') || expression.includes('const want =')) return { result: { value: 4 } };
  if (expression.includes('parts.join')) return { result: { value: options.late?.() ? 'Saved!' : 'Form' } };
  if (expression.includes('finish(false)')) return { result: { value: !options.restless } };
  if (expression === 'navigator.userAgent') return { result: { value: 'Mozilla/5.0 HeadlessChrome/130' } };
  return { result: { value: true } };
}

/** The stub's answer to reading element `pointed` (1-based) of the form; undefined when the expression asks something else. */
function refAnswer(pointed: string, expression: string, options: FormOptions): unknown {
  if (Number(pointed) > 4) return { result: { value: null } };
  if (expression.includes('elementFromPoint')) return { result: { value: { x: 10 * Number(pointed), y: 20, covered: '' } } };
  if (expression.includes("'aria-checked'")) return { result: { value: pointed === '2' ? 'unchecked' : 'text' } };
  if (expression.includes("'no-focus'")) return { result: { value: options.typing ?? 'field' } };
  return undefined;
}

/** A page whose [1] is a text field, [2] a checkbox, [3] a button that opens a popup, and [4] a drop zone. */
function interactive(options: FormOptions = {}) {
  let chrome: CdpStub | undefined;
  let pointed = '';
  const evaluate = (expression: string): unknown => {
    if (expression.includes('items.push')) return { result: { value: { title: 'Form', url: 'https://example.com/form', text: options.late?.() ? 'Saved!' : 'Form', items: ['[1] input "Name"', '[2] input:checkbox "Agree" unchecked', '[3] button "Open"', '[4] div "Drop here"'] } } };
    if (expression === 'document.readyState') return { result: { value: 'complete' } };
    const index = /__octoRefs \|\| \[\]\)\[(\d+)\]/.exec(expression)?.[1];
    if (index !== undefined) pointed = String(Number(index) + 1);
    return (index === undefined ? undefined : refAnswer(pointed, expression, options)) ?? formAnswer(expression, options);
  };
  const handle: CdpHandler = (method, params, page) => {
    if (method === 'Runtime.evaluate') return evaluate(String(params['expression']));
    if (method === 'Input.dispatchMouseEvent' && params['type'] === 'mousePressed' && pointed === '3') chrome!.addTarget('popup-1');
    if (method === 'Input.dispatchMouseEvent' && params['type'] === 'mouseMoved' && params['buttons'] === 1 && options.html5Drag) page.emit('Input.dragIntercepted', { data: { items: [], dragOperationsMask: 1 } });
    return {};
  };
  return { handle, set: (value: CdpStub) => (chrome = value) };
}

async function open(options: Parameters<typeof interactive>[0] = {}) {
  const page = interactive(options);
  const cdp = await stub({ handle: page.handle });
  page.set(cdp);
  vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
  const tool = register();
  await tool.text({ action: 'navigate', url: 'https://example.com/form' });
  const mouse = () => cdp.commands.filter((command) => command.method === 'Input.dispatchMouseEvent').map((command) => `${String(command.params['type'])}@${String(command.params['x'])}`);
  return { cdp, ...tool, mouse };
}

describe('browser input, forms, waits and tabs', () => {
  it('hovers with a bare mouse move, and drags as a pointer drag or an intercepted HTML5 drag', async () => {
    const plain = await open();
    await plain.text({ action: 'hover', ref: 3 });
    expect(plain.mouse()).toEqual(['mouseMoved@30']);
    expect(await plain.text({ action: 'drag', ref: 1, to: 4 })).toMatch(/^Dragged \[1\] onto \[4\] \(a mouse drag\)\./);
    expect(plain.mouse().slice(1, 3)).toEqual(['mouseMoved@10', 'mousePressed@10']);
    expect(plain.mouse().at(-1)).toBe('mouseReleased@40');
    expect(plain.cdp.commands.some((command) => command.method === 'Input.dispatchDragEvent')).toBe(false);
    await expect(plain.run({ action: 'drag', ref: 1 })).rejects.toThrow(/drag needs to/);
    // A drop zone named by its text.
    expect(await plain.text({ action: 'drag', ref: 1, text: 'Drop here' })).toMatch(/onto \[4\]/);

    const html5 = await open({ html5Drag: true });
    expect(await html5.text({ action: 'drag', ref: 1, to: 4 })).toMatch(/\(an HTML5 drag\)/);
    expect(html5.cdp.commands.filter((command) => command.method === 'Input.dispatchDragEvent').map((command) => command.params['type'])).toEqual(['dragEnter', 'dragOver', 'drop']);
    expect(html5.cdp.commands.filter((command) => command.method === 'Input.setInterceptDrags').map((command) => command.params['enabled'])).toEqual([true, false]);
  }, 20_000);

  it('fills text fields and toggles checkboxes in one call, reporting fields that failed', async () => {
    const { cdp, text, run } = await open();
    const result = await text({ action: 'fill', fields: [{ ref: 1, value: 'Ada' }, { ref: 2, value: 'true' }] });
    expect(result).toMatch(/^Filled 2 of 2 field\(s\)\./);
    expect(cdp.commands.find((command) => command.method === 'Input.insertText')?.params).toEqual({ text: 'Ada' });
    expect(cdp.commands.filter((command) => command.method === 'Input.dispatchMouseEvent' && command.params['type'] === 'mousePressed')).toHaveLength(1);
    await expect(run({ action: 'fill', fields: [{ ref: 9, value: 'x' }] })).rejects.toThrow(/No field was filled:\n\[9\]: No element \[9\]/);
    await expect(run({ action: 'fill' })).rejects.toThrow(/fill needs fields/);
  }, 20_000);

  it('shows page text in a failed field or a refused type as visible escapes, and keeps a CRLF in a partial fill', async () => {
    const { text, run } = await open({ typing: 'no-option:Red\r\nBlue\u001b[2J\u202e' });
    const partial = await text({ action: 'fill', fields: [{ ref: 1, value: 'x' }, { ref: 2, value: 'true' }] });
    expect(partial).toContain('Filled 1 of 2 field(s).');
    // The CRLF is evidence and stays; the terminal sequence and bidi override show as escapes.
    expect(partial).toContain('Options: Red\r\nBlue\\u{1b}[2J\\u{202e}');
    expect(partial).not.toContain('\u001b');
    await expect(run({ action: 'type', ref: 1, text: 'x' })).rejects.toThrow(/Options: Red\r\nBlue\\u\{1b\}\[2J\\u\{202e\}/);
    await expect(run({ action: 'fill', fields: [{ ref: 1, value: 'x' }] })).rejects.toThrow(/No field was filled:\n\[1\]: .*\\u\{1b\}/s);
  }, 20_000);

  it('navigates until text appears, notes a page that keeps changing, and leaves the system locale alone', async () => {
    let saved = false;
    const { cdp, text } = await open({ late: () => saved, restless: true });
    // Chrome follows the machine's language and time zone: nothing pins them.
    expect(cdp.commands.some((command) => command.method.startsWith('Emulation.set'))).toBe(false);

    expect(await text({ action: 'navigate', url: 'https://example.com/form' })).toMatch(/Note: the page was still changing 6s after loading/);
    setTimeout(() => (saved = true), 100);
    const waited = await text({ action: 'navigate', url: 'https://example.com/form', text: 'Saved!' });
    expect(waited).toMatch(/^"Saved!" appeared after \d+(?:ms|\.\ds|s)\.\n\n# Form/);
    expect(waited).not.toContain('still changing');
    // The text is already there: the wait is as short as the machine allows, so assert the format (`formatDuration(ms, true)`), not the speed.
    expect(await text({ action: 'snapshot', text: 'Saved!' })).toMatch(/^"Saved!" appeared after \d+(?:ms|\.\ds|s)\./);
  }, 20_000);

  it('waits for text, and follows, lists and switches tabs', async () => {
    let saved = false;
    const { cdp, text, run } = await open({ late: () => saved });
    setTimeout(() => (saved = true), 100);
    expect(await text({ action: 'wait', text: 'Saved!' })).toMatch(/^"Saved!" appeared after \d+(?:ms|\.\ds|s)\./);
    await expect(run({ action: 'wait' })).rejects.toThrow(/wait needs text/);

    // The button opens a popup: the session moves to it.
    expect(await text({ action: 'click', ref: 3 })).toMatch(/^This action opened a new tab; now driving it/);
    expect(cdp.commands.at(-1)?.target === 'popup-1' || cdp.commands.some((command) => command.target === 'popup-1' && command.method === 'Page.bringToFront')).toBe(true);
    const tabs = await text({ action: 'tabs' });
    expect(tabs).toMatch(/^Tabs \(3; \* = driven\):/);
    expect(tabs).toContain('* [3]');
    await text({ action: 'tab', index: 1 });
    expect(await text({ action: 'tabs' })).toContain('* [1]');
    await expect(run({ action: 'tab', index: 9 })).rejects.toThrow(/No tab 9\./);
    await text({ action: 'tab', url: 'https://example.com/other' });
    // The tab opens blank and is navigated once driven (its page guarded), so the guard sees its first load.
    const opened = cdp.commands.find((command) => command.method === 'Page.navigate' && command.params['url'] === 'https://example.com/other');
    expect(cdp.commands.filter((command) => command.target === opened?.target).map((command) => command.method)).toEqual(expect.arrayContaining(['Fetch.enable', 'Page.navigate']));
  }, 20_000);
});
