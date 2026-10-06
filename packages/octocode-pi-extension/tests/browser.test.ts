import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { initTheme, type ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NO_REPLY, startCdpStub, type CdpHandler, type CdpStub } from './fixtures/cdp-stub.js';
import { theme } from './fake-pi.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));

const { BrowserTool, registerBrowserTool } = await import('../src/browser/tool.js');
const { formatChanges, formatSnapshot } = await import('../src/browser/snapshot.js');
const { browserSummary } = await import('../src/browser/render.js');
const { pressKey, settle, settleAfterLoad, waitForText } = await import('../src/browser/input.js');
const { openBrowser } = await import('../src/browser/cdp.js');

type Result = { content: Array<{ type: string; text?: string; data?: string }> };
type Tool = {
  execute: (id: string, params: Record<string, unknown>) => Promise<Result>;
  renderCall: (args: Record<string, unknown>, theme: unknown, context: unknown) => { render(width: number): string[] };
  renderResult: (result: Result, options: { expanded: boolean }, theme: unknown, context: unknown) => { render(width: number): string[] };
};

const stubs: CdpStub[] = [];
const tools: InstanceType<typeof BrowserTool>[] = [];

beforeEach(() => {
  launch.mockReset();
});

afterEach(async () => {
  vi.unstubAllEnvs();
  await Promise.all(tools.splice(0).map((tool) => tool.close()));
  await Promise.all(stubs.splice(0).map((stub) => stub.close()));
});

/** A page with one button [1] and one input [2]; actions that change the page fire Page.loadEventFired. */
const pageHandler: CdpHandler = (method, params, page) => {
  const loaded = () => setTimeout(() => page.emit('Page.loadEventFired'), 5);
  if (method === 'Runtime.evaluate') {
    const expression = String(params['expression']);
    if (expression.includes('items.push')) return { result: { value: { title: 'Stub page', url: 'https://example.com/', text: 'Hello from the stub', items: ['[1] button "Go"', '[2] input:text "Name"'] } } };
    const index = /__octoRefs \|\| \[\]\)\[(\d+)\]/.exec(expression)?.[1];
    if (index !== undefined) {
      const known = index === '0' || index === '1';
      if (!known) return { result: { value: null } };
      // The click action reads the element's centre; typing checks focus landed on a text field.
      if (expression.includes('elementFromPoint')) return { result: { value: { x: 10, y: 20, covered: index === '1' ? 'div#overlay "Cookie banner"' : '' } } };
      if (expression.includes("'no-focus'")) return { result: { value: 'field' } };
      return { result: { value: true } };
    }
    if (expression.includes('location.href + " | "')) return { result: { value: 'https://example.com/ | Stub page' } };
    if (expression === 'boom()') return { exceptionDetails: { text: 'Uncaught', exception: { description: 'ReferenceError: boom is not defined' } } };
    if (expression === 'undefined') return { result: {} };
    if (expression === 'document.title') return { result: { value: 'Stub page' } };
    return { result: { value: { answer: 42 } } };
  }
  if (method === 'Page.navigate') {
    const url = String(params['url']);
    if (url.includes('unreachable')) return { frameId: 'f', loaderId: 'l', errorText: 'net::ERR_NAME_NOT_RESOLVED' };
    if (url.includes('#')) return { frameId: 'f' };
    loaded();
    return { frameId: 'f', loaderId: 'l' };
  }
  if (method === 'Page.captureScreenshot') return { data: 'iVBORw0KGgo=' };
  if (method === 'Input.insertText' || (method === 'Input.dispatchKeyEvent' && params['type'] === 'keyUp') || (method === 'Input.dispatchMouseEvent' && params['type'] === 'mouseReleased')) loaded();
  if (method === 'Slow.never') return NO_REPLY;
  if (method === 'Fail.always') return { error: { message: 'no such method' } };
  return {};
};

async function stub(options: Parameters<typeof startCdpStub>[0] = {}): Promise<CdpStub> {
  const started = await startCdpStub({ handle: pageHandler, ...options });
  stubs.push(started);
  return started;
}

function register() {
  const browser = new BrowserTool();
  tools.push(browser);
  let tool: Tool | undefined;
  const handlers = new Map<string, () => Promise<void>>();
  const pi = {
    registerTool: (definition: Tool) => {
      tool = definition;
    },
    on: (event: string, handler: () => Promise<void>) => handlers.set(event, handler),
  } as unknown as ExtensionAPI;
  registerBrowserTool(pi, browser);
  const run = (params: Record<string, unknown>) => tool!.execute('id', params);
  const text = async (params: Record<string, unknown>) => (await run(params)).content.map((part) => part.text ?? `[${part.type}]`).join('\n');
  return { browser, tool: tool!, handlers, run, text };
}

describe('browser tool over a shared Chrome', () => {
  it('opens its own tab and drives the page through every action', async () => {
    const cdp = await stub();
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { text, run } = register();

    expect(await text({ action: 'info' })).toBe('No browser is open. navigate or attach starts one.');
    await text({ action: 'snapshot' }); // info reports a browser; it never starts one
    expect(await text({ action: 'info' })).toBe(`shared Chrome, own tab · DevTools port ${cdp.port} · https://example.com/ | Stub page`);
    expect(cdp.requests).toContain('PUT /json/new?about:blank');
    expect(cdp.commands.slice(0, 2).map((command) => command.method).sort()).toEqual(['Page.enable', 'Runtime.enable']);

    const snapshot = await text({ action: 'navigate', url: 'https://example.com/' });
    expect(snapshot).toContain('# Stub page\nhttps://example.com/\n\n## Elements\n[1] button "Go"');
    expect(snapshot).toContain('## Text\nHello from the stub');
    expect(await text({ action: 'snapshot' })).toContain('[2] input:text "Name"');
    // Same page after the click: only the changes come back, not the whole snapshot.
    expect(await text({ action: 'click', ref: 1 })).toBe('# Stub page\nhttps://example.com/ (same page)\n\n## Elements\nUnchanged (2).\n\n## New text\nNo new text.');
    expect(cdp.commands.filter((command) => command.method === 'Input.dispatchMouseEvent').map((command) => command.params['type'])).toEqual(['mouseMoved', 'mousePressed', 'mouseReleased']);
    expect(cdp.commands.find((command) => command.method === 'Input.dispatchMouseEvent')?.params).toMatchObject({ x: 10, y: 20 });
    expect(await text({ action: 'click', ref: 2 })).toMatch(/^Note: div#overlay "Cookie banner" covers \[2\], so the click landed on it\./);
    await expect(run({ action: 'click', ref: 9 })).rejects.toThrow(/No element \[9\]/);
    await expect(run({ action: 'click' })).rejects.toThrow(/needs ref/);

    await text({ action: 'type', ref: 2, text: 'Ada', submit: true });
    expect(cdp.commands.find((command) => command.method === 'Input.insertText')?.params).toEqual({ text: 'Ada' });
    expect(cdp.commands.filter((command) => command.method === 'Input.dispatchKeyEvent').map((command) => command.params)).toEqual([
      { type: 'keyDown', key: 'Enter', code: 'Enter', modifiers: 0, windowsVirtualKeyCode: 13, text: '\r' },
      { type: 'keyUp', key: 'Enter', code: 'Enter', modifiers: 0, windowsVirtualKeyCode: 13 },
    ]);
    await expect(run({ action: 'type', ref: 7, text: 'x' })).rejects.toThrow(/No element \[7\]/);

    await expect(run({ action: 'type', ref: 2 })).rejects.toThrow(/type needs text/);
    await expect(run({ action: 'press' })).rejects.toThrow(/press needs key/);
    await text({ action: 'press', key: '5' });
    await text({ action: 'press', key: 'a' });
    const codes = cdp.commands.filter((command) => command.method === 'Input.dispatchKeyEvent' && command.params['type'] === 'keyDown').map((command) => command.params['code']);
    expect(codes.slice(-2)).toEqual(['Digit5', 'KeyA']);

    expect(await text({ action: 'evaluate', expression: 'document.title' })).toBe('Stub page');
    expect(JSON.parse(await text({ action: 'evaluate', expression: '({ answer: 42 })' }))).toEqual({ answer: 42 });
    expect(await text({ action: 'evaluate', expression: 'undefined' })).toBe('undefined');
    await expect(run({ action: 'evaluate', expression: 'boom()' })).rejects.toThrow('ReferenceError: boom is not defined');
    await expect(run({ action: 'evaluate' })).rejects.toThrow(/needs expression/);

    const shot = await run({ action: 'screenshot' });
    expect(shot.content).toEqual([{ type: 'image', data: 'iVBORw0KGgo=', mimeType: 'image/png' }]);

    expect(await text({ action: 'console' })).toBe('(no console messages)');
    const page = cdp.pages.get('tab-1')!;
    page.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: 'hello' }, { description: 'Object' }, {}] });
    page.emit('Runtime.exceptionThrown', { exceptionDetails: { exception: { description: 'Error: bad' } } });
    page.emit('Runtime.exceptionThrown', { exceptionDetails: { text: 'Uncaught' } });
    page.emit('Runtime.consoleAPICalled', { type: 'warn' });
    await vi.waitFor(async () => expect(await text({ action: 'console' })).toBe('[log] hello Object \n[exception] Error: bad\n[exception] Uncaught\n[warn] '));

    expect(await text({ action: 'close' })).toBe('Browser closed.');
    expect(cdp.requests).toContain('GET /json/close/tab-1');
  }, 20_000);

  it('reports navigation errors and does not wait for a load event on a same-document navigation', async () => {
    const cdp = await stub();
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { run, text } = register();
    await expect(run({ action: 'navigate' })).rejects.toThrow(/needs url/);
    await expect(run({ action: 'navigate', url: 'https://unreachable.invalid/' })).rejects.toThrow('Navigation failed: net::ERR_NAME_NOT_RESOLVED');
    const started = Date.now();
    expect(await text({ action: 'navigate', url: 'https://example.com/#section' })).toContain('# Stub page');
    expect(Date.now() - started).toBeLessThan(5_000);
  }, 20_000);

  it('stops a navigation when aborted, leaving no pending load wait behind', async () => {
    const cdp = await stub({ handle: (method, params, page) => (method === 'Page.navigate' ? NO_REPLY : pageHandler(method, params, page)) });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { tool } = register();
    const controller = new AbortController();
    const pending = (tool.execute as (id: string, params: Record<string, unknown>, signal: AbortSignal) => Promise<Result>)('id', { action: 'navigate', url: 'https://slow.example/' }, controller.signal);
    await vi.waitFor(() => expect(cdp.commands.some((command) => command.method === 'Page.navigate')).toBe(true));
    controller.abort(new Error('user stopped'));
    await expect(pending).rejects.toThrow('user stopped');
  }, 20_000);

  it('reopens the session after the page connection drops, rejecting what was pending', async () => {
    const cdp = await stub();
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { browser, text } = register();
    const first = await browser.session();
    const pending = first.page.send('Slow.never');
    cdp.pages.get('tab-1')!.drop();
    await expect(pending).rejects.toThrow('Browser connection closed');
    await vi.waitFor(() => expect(first.page.open).toBe(false));
    await expect(first.page.send('Page.enable')).rejects.toThrow('Browser connection closed');
    expect(await text({ action: 'snapshot' })).toContain('# Stub page');
    expect(cdp.requests).toContain('GET /json/close/tab-1');
    expect(cdp.pages.has('tab-2')).toBe(true);
  }, 20_000);

  it('throws when the page cannot be read and closes on session shutdown', async () => {
    const cdp = await stub({ handle: (method, params, page) => (method === 'Runtime.evaluate' && String(params['expression']).includes('items.push') ? { result: {} } : pageHandler(method, params, page)) });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { run, handlers } = register();
    await expect(run({ action: 'snapshot' })).rejects.toThrow(/Could not read the page/);
    await handlers.get('session_shutdown')!();
    expect(cdp.requests).toContain('GET /json/close/tab-1');
  }, 20_000);

  it('strips terminal controls from page output and saves a large evaluate result to a file', async () => {
    const home = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-browser-home-'));
    vi.stubEnv('OCTOCODE_HOME', home);
    const big = Array.from({ length: 5000 }, (_, n) => `row ${n + 1} ${'q'.repeat(40)}`).join('\n');
    const cdp = await stub({
      handle: (method, params, page) => {
        const expression = String(params['expression'] ?? '');
        if (method === 'Runtime.evaluate' && expression === 'hostile') return { result: { value: 'a\u001b[31mred\u001b[0m \u202eevil' } };
        if (method === 'Runtime.evaluate' && expression === 'big') return { result: { value: big } };
        return pageHandler(method, params, page);
      },
    });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { run } = register();
    const textOf = async (args: Record<string, unknown>) => ((await run(args)).content as Array<{ text: string }>)[0]!.text;
    expect(await textOf({ action: 'evaluate', expression: 'hostile' })).toBe('ared evil');
    const saved = await textOf({ action: 'evaluate', expression: 'big' });
    expect(saved).toContain('row 1 ');
    expect(saved).toContain('row 5000 ');
    const file = /Full output: (\S+?\.txt)/.exec(saved)?.[1];
    expect(file?.startsWith(path.join(home, 'agent', 'pi', 'sessions'))).toBe(true);
    expect(fs.readFileSync(file!, 'utf8')).toBe(big);
  }, 20_000);

  it('renders calls and results on one line each', () => {
    initTheme('dark');
    const { tool } = register();
    const line = (args: Record<string, unknown>) => tool.renderCall(args, theme, {}).render(200).join('\n').trim();
    expect(line({ action: 'navigate', url: 'https://example.com' })).toBe('○ Browser(navigate https://example.com)');
    expect(line({ action: 'click', ref: 3 })).toBe('○ Browser(click [3])');
    expect(line({ action: 'type', ref: 2, text: 'hello' })).toBe('○ Browser(type [2] "hello")');
    expect(line({ action: 'evaluate', expression: 'a\nb' })).toBe('○ Browser(evaluate a)');
    expect(line({ action: 'press', key: 'Tab' })).toBe('○ Browser(press Tab)');
    const render = (content: Array<{ type: string; text?: string }>, context: Record<string, unknown> = {}) => tool.renderResult({ content }, { expanded: false }, theme, { isError: false, ...context }).render(200).join('\n');
    const plain = render([{ type: 'image' }, { type: 'text', text: 'one\ntwo\nthree\nfour\nfive' }]);
    expect(plain).toContain('⎿  one');
    expect(plain).toContain('… +1 line');
    expect(render([{ type: 'image' }])).toContain('⎿  Screenshot');
    const snapshot = render([{ type: 'text', text: '# Example\nhttps://example.com\n\n## Elements\n[1] link "More"\n[Full snapshot (every element and all text): /tmp/snap.txt; read it by line range or search it.]' }]);
    expect(snapshot).toContain('⎿  Example — https://example.com');
    expect(snapshot).not.toContain('saved:');
    const snapshotText = '# Example\nhttps://example.com\n\n## Elements\n[1] link "More"\n[Full snapshot (every element and all text): /tmp/snap.txt; read it by line range or search it.]';
    expect(tool.renderResult({ content: [{ type: 'text', text: snapshotText }] }, { expanded: true }, theme, { isError: false, expanded: true }).render(200).join('\n')).toContain('saved: /tmp/snap.txt');
    expect(render([{ type: 'text', text: 'Browser not running\nstart it' }], { isError: true })).toContain('⎿  Error: Browser not running');
    expect(browserSummary('# \nhttps://a.example (same page)\nrest')).toEqual({ summary: '(untitled) — https://a.example (same page)', body: 'rest' });
    expect(browserSummary('Filled 1 of 1 field.\n\n# T\nu')).toMatchObject({ summary: 'Filled 1 of 1 field.' });
  });
});

describe('browser attach', () => {
  it('drives the existing page of the browser on a given port', async () => {
    const cdp = await stub({ targets: [{ id: 'worker', type: 'service_worker' }, { id: 'nows', type: 'page', ws: false }, { id: 'page-1', type: 'page' }] });
    const { text, run } = register();
    await expect(run({ action: 'attach' })).rejects.toThrow(/attach needs port/);

    const attached = await text({ action: 'attach', port: cdp.port });
    expect(attached).toContain(`Attached to the browser on DevTools port ${cdp.port}.`);
    expect(attached).toContain('# Stub page');
    expect(cdp.pages.has('page-1')).toBe(true);
    expect(cdp.requests.some((request) => request.startsWith('PUT /json/new'))).toBe(false);
    expect(await text({ action: 'info' })).toMatch(/^attached window · DevTools port \d+ · /);

    expect(await text({ action: 'attach', port: cdp.port })).toContain('Attached');
    await text({ action: 'close' });
    // Someone else's page: never closed by us.
    expect(cdp.requests.some((request) => request.startsWith('GET /json/close'))).toBe(false);
  }, 20_000);

  it('fails when nothing listens on the port', async () => {
    const cdp = await stub();
    const port = cdp.port;
    await cdp.close();
    await expect(openBrowser({}, { port, reuseTab: true })).rejects.toThrow(`No browser is listening on DevTools port ${port}.`);
  });

  it('opens a tab when the browser lists no page', async () => {
    const cdp = await stub({ targets: [] });
    const session = await openBrowser({}, { port: cdp.port, reuseTab: true });
    expect(session.kind).toBe('attached');
    expect(cdp.pages.has('tab-1')).toBe(true);
    session.page.close();
  });
});

describe('browser launching its own Chrome', () => {
  async function closedPort(): Promise<number> {
    const cdp = await startCdpStub();
    await cdp.close();
    return cdp.port;
  }

  it('launches headless Chrome when none is listening, and kills it on close', async () => {
    const chrome = await stub();
    const kill = vi.fn(async () => undefined);
    launch.mockResolvedValue({ port: chrome.port, kill });
    const session = await openBrowser({ OCTOCODE_CHROME_PORT: String(await closedPort()) });
    expect(session).toMatchObject({ kind: 'own', headless: true, port: chrome.port });
    expect(launch.mock.calls[0]![0].chromeFlags).toContain('--headless=new');
    const { browser } = register();
    session.page.close();

    vi.stubEnv('OCTOCODE_CHROME_PORT', String(await closedPort()));
    vi.stubEnv('OCTOCODE_BROWSER_HEADLESS', '0');
    const own = await browser.session();
    expect(own.headless).toBe(false);
    expect(launch.mock.calls[1]![0].chromeFlags).not.toContain('--headless=new');
    await browser.close();
    expect(kill).toHaveBeenCalledTimes(1);
  }, 20_000);

  it('falls back to launching when the running Chrome refuses a new tab', async () => {
    const refusing = await stub({ newTabStatus: 500 });
    const chrome = await stub();
    launch.mockResolvedValue({ port: chrome.port, kill: vi.fn(async () => undefined) });
    const session = await openBrowser({ OCTOCODE_CHROME_PORT: String(refusing.port) });
    expect(session.kind).toBe('own');
    session.page.close();
  });

  it('kills the launched Chrome when its page cannot be reached', async () => {
    const kill = vi.fn(async () => undefined);
    launch.mockResolvedValue({ port: await closedPort(), kill });
    await expect(openBrowser({ OCTOCODE_CHROME_PORT: String(await closedPort()) })).rejects.toThrow();
    expect(kill).toHaveBeenCalledTimes(1);
  });

  it('kills the launched Chrome, and closes a tab it opened, when the page session fails to start', async () => {
    const failing = await stub({ handle: (method) => (method === 'Page.enable' ? { error: { message: 'Page domain unavailable' } } : {}) });
    const kill = vi.fn(async () => undefined);
    launch.mockResolvedValue({ port: failing.port, kill });
    await expect(openBrowser({ OCTOCODE_CHROME_PORT: String(await closedPort()) })).rejects.toThrow('Page domain unavailable');
    expect(kill).toHaveBeenCalledTimes(1);

    await expect(openBrowser({ OCTOCODE_CHROME_PORT: String(failing.port) })).rejects.toThrow('Page domain unavailable');
    await vi.waitFor(() => expect(failing.requests.some((request) => request.startsWith('GET /json/close/tab-'))).toBe(true));
  });
});

describe('CDP page session', () => {
  it('ignores malformed messages and throwing listeners, and surfaces command errors', async () => {
    const cdp = await stub();
    const session = await openBrowser({}, { port: cdp.port });
    const page = cdp.pages.get('tab-1')!;
    const off = session.page.on(() => {
      throw new Error('listener bug');
    });
    page.raw('not json');
    page.emit('Some.event');
    await expect(session.page.send('Fail.always')).rejects.toThrow('no such method');
    off();
    const waited = session.page.waitFor('Never.fired', 20);
    await expect(waited).resolves.toBeUndefined();
    await session.closeTab?.();
    session.page.close();
  });

  it('stops waiting for commands and events when the tool call is aborted', async () => {
    const cdp = await stub();
    const session = await openBrowser({}, { port: cdp.port });
    const controller = new AbortController();
    const pending = session.page.send('Slow.never', {}, controller.signal);
    const waiting = session.page.waitFor('Never.fired', 60_000, controller.signal);
    controller.abort(new Error('user cancelled'));
    await expect(pending).rejects.toThrow('user cancelled');
    await expect(waiting).rejects.toThrow('user cancelled');
    await expect(session.page.evaluate('1', controller.signal)).rejects.toThrow('user cancelled');
    await session.closeTab?.();
    session.page.close();
  });
});

describe('settle', () => {
  const events = () => {
    const listeners = new Set<(method: string, params: Record<string, unknown>) => void>();
    const evaluated: string[] = [];
    return {
      on: (listener: (method: string, params: Record<string, unknown>) => void) => (listeners.add(listener), () => listeners.delete(listener)),
      emit: (method: string, params: Record<string, unknown> = {}) => [...listeners].forEach((listener) => listener(method, params)),
      count: () => listeners.size,
      evaluate: async (expression: string) => (evaluated.push(expression), true) as never,
      evaluated,
    };
  };
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
});

describe('settleAfterLoad', () => {
  it('waits in the page for quiet content and requests, reports a timeout, and survives a torn-down page', async () => {
    const expressions: string[] = [];
    const quiet = await settleAfterLoad({ evaluate: async (expression: string) => (expressions.push(expression), true) as never }, undefined, 500, 6_000);
    expect(quiet.settled).toBe(true);
    expect(expressions[0]).toContain('MutationObserver');
    expect(expressions[0]).toContain("perf.observe({ type: 'resource' })");
    expect(expressions[0]).toMatch(/setTimeout\(\(\) => finish\(false\), 6000\)/);
    expect((await settleAfterLoad({ evaluate: async () => false as never })).settled).toBe(false);
    expect((await settleAfterLoad({ evaluate: async () => Promise.reject(new Error('context destroyed')) })).settled).toBe(true);
  });
});

describe('keyboard and waits', () => {
  it('presses combos with modifiers, editing commands and key codes', async () => {
    const sent: Array<Record<string, unknown>> = [];
    const page = { send: async (_method: string, params: Record<string, unknown>) => (sent.push(params), {}) } as never;
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
    let body = 'loading';
    const page = { evaluate: async () => body as never, untilDialog: <T>(work: Promise<T>) => work };
    setTimeout(() => (body = 'Ready now'), 30);
    expect(await waitForText(page, 'Ready', false, 2_000, undefined, 10)).toBeGreaterThanOrEqual(0);
    expect(await waitForText(page, 'loading', true, 2_000, undefined, 10)).toBe(0);
    await expect(waitForText(page, 'never', false, 50, undefined, 10)).rejects.toThrow('Timed out after 0s waiting for "never" to appear.');
  });
});

describe('formatSnapshot', () => {
  it('keeps the element list first and fills the rest of 8 KB with page text', () => {
    const items = Array.from({ length: 1_000 }, (_, index) => `[${index + 1}] button "Action number ${index + 1}"`);
    const out = formatSnapshot({ title: 'Big', url: 'https://example.com', text: 'word '.repeat(10_000), items });
    expect(out.length).toBeLessThanOrEqual(8_200);
    expect(out.indexOf('## Elements')).toBeLessThan(out.indexOf('## Text'));
    expect(out).toContain('[1] button');
    expect(out).toMatch(/more elements; click by number still works/);
    expect(out).toMatch(/more characters cut\]$/);
    const small = formatSnapshot({ title: 'T', url: 'u', text: 'hello', items: [] });
    expect(small).toBe('# T\nu\n\n## Elements\n(none)\n\n## Text\nhello');
  });

  it('names the locale the page rendered for and lists footer elements compactly', () => {
    const footer = Array.from({ length: 200 }, (_, index) => `[${index + 10}] Footer link ${index}`);
    const out = formatSnapshot({ title: 'Plans', url: 'https://wix.test/plans', text: '₪ 327 mo', items: ['[1] button "Start"'], footer, locale: 'page lang en · browser en-US · time zone Asia/Jerusalem · currency ₪×41' });
    expect(out).toMatch(/^# Plans\nhttps:\/\/wix\.test\/plans\nLocale: page lang en · browser en-US · time zone Asia\/Jerusalem · currency ₪×41\n\n## Elements\n\[1\] button "Start"\n\n## Footer \(200\)\n\[10\] Footer link 0 · \[11\] Footer link 1/);
    expect(out).toMatch(/more characters cut; click by number still works\]\n\n## Text\n₪ 327 mo$/);
  });
});

describe('formatChanges', () => {
  const page = { title: 'Shop', url: 'https://shop.test/', text: 'Cart\nEmpty', items: ['[1] button "Add"', '[2] input "Qty" value="1"'] };
  it('reports new text, and only the elements that are new, changed or gone (numbers are stable)', () => {
    expect(formatChanges(page, { ...page, text: 'Cart\n1 item' })).toBe('# Shop\nhttps://shop.test/ (same page)\n\n## Elements\nUnchanged (2).\n\n## New text (1 line gone)\n1 item');
    const changed = formatChanges(page, { ...page, items: ['[2] input "Qty" value="3"', '[5] button "Checkout"'] });
    expect(changed).toContain('## Elements (new or changed)\n[2] input "Qty" value="3"\n[5] button "Checkout"\nNo longer shown: [1]\n\n## New text\nNo new text.');
    expect(formatChanges(page, { ...page, text: `Cart\n${'new line\n'.repeat(1_000)}` })).toMatch(/more characters cut; take a snapshot to see the whole page\]$/);
  });
});

describe('browser input, forms, waits and tabs', () => {
  /** A page whose [1] is a text field, [2] a checkbox, [3] a button that opens a popup, and [4] a drop zone. */
  function interactive(options: { html5Drag?: boolean; late?: () => boolean; restless?: boolean } = {}) {
    let stub: CdpStub | undefined;
    let pointed = '';
    const handle: CdpHandler = (method, params, page) => {
      if (method === 'Runtime.evaluate') {
        const expression = String(params['expression']);
        if (expression.includes('items.push')) return { result: { value: { title: 'Form', url: 'https://example.com/form', text: options.late?.() ? 'Saved!' : 'Form', items: ['[1] input "Name"', '[2] input:checkbox "Agree" unchecked', '[3] button "Open"', '[4] div "Drop here"'] } } };
        if (expression === 'document.readyState') return { result: { value: 'complete' } };
        const index = /__octoRefs \|\| \[\]\)\[(\d+)\]/.exec(expression)?.[1];
        if (index !== undefined) {
          pointed = String(Number(index) + 1);
          if (Number(pointed) > 4) return { result: { value: null } };
          if (expression.includes('elementFromPoint')) return { result: { value: { x: 10 * Number(pointed), y: 20, covered: '' } } };
          if (expression.includes("'aria-checked'")) return { result: { value: pointed === '2' ? 'unchecked' : 'text' } };
          if (expression.includes("'no-focus'")) return { result: { value: 'field' } };
        }
        if (expression.includes('smallest visible element') || expression.includes('const want =')) return { result: { value: 4 } };
        if (expression.includes('parts.join')) return { result: { value: options.late?.() ? 'Saved!' : 'Form' } };
        if (expression.includes('finish(false)')) return { result: { value: !options.restless } };
        if (expression === 'navigator.userAgent') return { result: { value: 'Mozilla/5.0 HeadlessChrome/130' } };
        return { result: { value: true } };
      }
      if (method === 'Input.dispatchMouseEvent' && params['type'] === 'mousePressed' && pointed === '3') stub!.addTarget('popup-1');
      if (method === 'Input.dispatchMouseEvent' && params['type'] === 'mouseMoved' && params['buttons'] === 1 && options.html5Drag) page.emit('Input.dragIntercepted', { data: { items: [], dragOperationsMask: 1 } });
      return {};
    };
    return { handle, set: (value: CdpStub) => (stub = value) };
  }

  async function open(options: Parameters<typeof interactive>[0] = {}) {
    const page = interactive(options);
    const cdp = await stub({ handle: page.handle });
    page.set(cdp);
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const tool = register();
    await tool.text({ action: 'navigate', url: 'https://example.com/form' });
    const mouse = () => cdp.commands.filter((command) => command.method === 'Input.dispatchMouseEvent').map((command) => `${command.params['type']}@${command.params['x']}`);
    return { cdp, ...tool, mouse };
  }

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

  it('navigates until text appears, notes a page that keeps changing, and pins the locale', async () => {
    vi.stubEnv('OCTOCODE_BROWSER_LOCALE', 'en-US');
    vi.stubEnv('OCTOCODE_BROWSER_TIMEZONE', 'America/New_York');
    let saved = false;
    const { cdp, text } = await open({ late: () => saved, restless: true });
    expect(cdp.commands.find((command) => command.method === 'Emulation.setLocaleOverride')?.params).toEqual({ locale: 'en-US' });
    expect(cdp.commands.find((command) => command.method === 'Emulation.setUserAgentOverride')?.params).toEqual({ userAgent: 'Mozilla/5.0 HeadlessChrome/130', acceptLanguage: 'en-US,en;q=0.9' });
    expect(cdp.commands.find((command) => command.method === 'Emulation.setTimezoneOverride')?.params).toEqual({ timezoneId: 'America/New_York' });

    expect(await text({ action: 'navigate', url: 'https://example.com/form' })).toMatch(/Note: the page was still changing 6s after loading/);
    setTimeout(() => (saved = true), 100);
    const waited = await text({ action: 'navigate', url: 'https://example.com/form', text: 'Saved!' });
    expect(waited).toMatch(/^"Saved!" appeared after \d+\.\ds\.\n\n# Form/);
    expect(waited).not.toContain('still changing');
    expect(await text({ action: 'snapshot', text: 'Saved!' })).toMatch(/^"Saved!" appeared after 0\.\ds\./);
  }, 20_000);

  it('waits for text, and follows, lists and switches tabs', async () => {
    let saved = false;
    const { cdp, text, run } = await open({ late: () => saved });
    setTimeout(() => (saved = true), 100);
    expect(await text({ action: 'wait', text: 'Saved!' })).toMatch(/^"Saved!" appeared after \d+\.\ds\./);
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
    expect(cdp.requests.some((request) => request.startsWith('PUT /json/new?https://example.com/other'))).toBe(true);
  }, 20_000);
});
