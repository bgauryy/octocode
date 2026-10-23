import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { afterEach, vi } from 'vitest';
import { NO_REPLY, startCdpStub, type CdpHandler, type CdpStub } from './fixtures/cdp-stub.js';
import { toolContext } from './doubles.js';
import { fakeCtx, fakePermission, fakePi } from './fake-pi.js';
import { BrowserTool, registerBrowserTool } from '../src/browser/tool.js';

/**
 * The `browser` tool over a stub Chrome (`fixtures/cdp-stub.ts`). A test file using it mocks `chrome-launcher` (and
 * `node:dns/promises`, for a public answer without network) itself: vitest hoists those mocks above this import.
 */
export type Result = { content: Array<{ type: string; text?: string; data?: string }> };
export type Tool = {
  execute: (...args: [id: string, params: Record<string, unknown>, signal: AbortSignal | undefined, onUpdate: undefined, ctx: ExtensionContext]) => Promise<Result>;
  renderCall: (args: Record<string, unknown>, theme: unknown, context: unknown) => { render(width: number): string[] };
  renderResult: (result: Result, options: { expanded: boolean }, theme: unknown, context: unknown) => { render(width: number): string[] };
};

type Reply = ReturnType<CdpHandler>;

/** The stub's answer to reading element `index` of the page's refs: [1] is covered by a banner, [2] is a text field. */
function refAnswer(index: string, expression: string): Reply {
  if (index !== '0' && index !== '1') return { result: { value: null } };
  // The click action reads the element's centre; typing checks focus landed on a text field.
  if (expression.includes('elementFromPoint')) return { result: { value: { x: 10, y: 20, covered: index === '1' ? 'div#overlay "Cookie banner"' : '' } } };
  if (expression.includes("'no-focus'")) return { result: { value: 'field' } };
  return { result: { value: true } };
}

/** Fixed answers to the expressions tests evaluate by name; anything else evaluates to `{ answer: 42 }`. */
const EVALUATED: Record<string, Reply> = {
  'boom()': { exceptionDetails: { text: 'Uncaught', exception: { description: 'ReferenceError: boom is not defined' } } },
  'bad()': { exceptionDetails: { text: 'Uncaught', exception: { description: 'Error: \u001b[2Jwiped\u202e' } } },
  undefined: { result: {} },
  'document.title': { result: { value: 'Stub page' } },
};

/** The stub's answer to Runtime.evaluate of `expression`. */
function evaluate(expression: string): Reply {
  if (expression.includes('items.push')) return { result: { value: { title: 'Stub page', url: 'https://example.com/', text: 'Hello from the stub', items: ['[1] button "Go"', '[2] input:text "Name"'] } } };
  const index = /__octoRefs \|\| \[\]\)\[(\d+)\]/.exec(expression)?.[1];
  if (index !== undefined) return refAnswer(index, expression);
  if (expression.includes('location.href + " | "')) return { result: { value: 'https://example.com/ | Stub page' } };
  return Object.hasOwn(EVALUATED, expression) ? EVALUATED[expression] : { result: { value: { answer: 42 } } };
}

/** The stub's answer to Page.navigate to `url`: an unreachable host fails, a fragment stays on the page, any other loads. */
function navigate(url: string, loaded: () => void): Reply {
  if (url.includes('unreachable')) return { frameId: 'f', loaderId: 'l', errorText: 'net::ERR_NAME_NOT_RESOLVED' };
  if (url.includes('#')) return { frameId: 'f' };
  loaded();
  return { frameId: 'f', loaderId: 'l' };
}

/** Whether `method` with `params` is an input that changes the page: inserted text, a released key or mouse button. */
const changesPage = (method: string, params: Record<string, unknown>) =>
  method === 'Input.insertText' || (method === 'Input.dispatchKeyEvent' && params['type'] === 'keyUp') || (method === 'Input.dispatchMouseEvent' && params['type'] === 'mouseReleased');

/** A page with one button [1] and one input [2]; actions that change the page fire Page.loadEventFired. */
export const pageHandler: CdpHandler = (method, params, page) => {
  const loaded = () => setTimeout(() => page.emit('Page.loadEventFired'), 5);
  if (method === 'Runtime.evaluate') return evaluate(String(params['expression']));
  if (method === 'Page.navigate') return navigate(String(params['url']), loaded);
  if (method === 'Page.captureScreenshot') return { data: 'iVBORw0KGgo=' };
  if (changesPage(method, params)) loaded();
  if (method === 'Slow.never') return NO_REPLY;
  if (method === 'Fail.always') return { error: { message: 'no such method' } };
  return {};
};

/** Registers the cleanup of every stub and tool a test starts; `stub` starts a stub Chrome (`pageHandler` unless given), `register` the tool. */
export function useBrowserHarness() {
  const stubs: CdpStub[] = [];
  const tools: BrowserTool[] = [];

  afterEach(async () => {
    vi.unstubAllEnvs();
    await Promise.all(tools.splice(0).map((browser) => browser.close()));
    await Promise.all(stubs.splice(0).map((cdp) => cdp.close()));
  });

  async function stub(options: Parameters<typeof startCdpStub>[0] = {}): Promise<CdpStub> {
    const started = await startCdpStub({ handle: pageHandler, ...options });
    stubs.push(started);
    return started;
  }

  function register() {
    const browser = new BrowserTool(fakePermission());
    tools.push(browser);
    const fake = fakePi();
    registerBrowserTool(fake.pi, browser);
    const tool: Tool = fake.tools.get('browser');
    // The tool registers one handler per event; a test calls it with no event.
    const handlers = new Map([...fake.handlers].map(([event, [handler, ...more]]) => {
      if (!handler || more.length > 0) throw new Error(`expected one ${event} handler`);
      return [event, async () => handler(undefined, undefined)];
    }));
    const run = (params: Record<string, unknown>) => tool.execute('id', params, undefined, undefined, toolContext(fakeCtx({ cwd: process.cwd(), hasUI: false })));
    const text = async (params: Record<string, unknown>) => (await run(params)).content.map((part) => part.text ?? `[${part.type}]`).join('\n');
    return { browser, tool, handlers, run, text };
  }

  return { stub, register };
}
